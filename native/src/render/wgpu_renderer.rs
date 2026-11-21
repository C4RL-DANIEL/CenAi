//! WGPU renderer hosting CenAI's native egui screens over the animated GPU background.
use std::{collections::HashMap, path::PathBuf, sync::{mpsc, Arc}};
use tokio::runtime::Handle;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::{core::{agents::TaskPlan, brain::CentralBrain, context::{ContextKind, ContextRecord, ContextRequest}, guardian::{DiagnosticEvent, DiagnosticKind, GuardianMonitor}, mcp::{McpHttpClient, McpServerManifest, McpTool}, network::CenAiApi, providers::{ChatMessage, ChatRequest}, workspace::WorkspaceStore}, ui::{model::{AppState, MessageRole}, screen}};
use super::RenderBackend;

fn new_session_id() -> String {
    format!(
        "session-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
    )
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShaderUniforms {
    resolution: [f32; 2],
    time_seconds: f32,
    intensity: f32,
}

enum ChatEvent {
    Delta(u64, u64, String),
    Finished(u64, u64, Result<String, String>),
    SignedIn(u64, Result<String, String>),
    SignedOut(u64, Result<(), String>),
    OtpSent(u64, Result<String, String>),
    OtpVerified(u64, Result<String, String>),
    PlanFinished(u64, u64, Result<TaskPlan, String>),
    McpConnected(u64, Result<(McpServerManifest, Arc<McpHttpClient>, Vec<McpTool>), String>),
    McpCallFinished(u64, Result<serde_json::Value, String>),
    CheckoutReady(u64, Result<String, String>),
}

impl ChatEvent {
    fn account_generation(&self) -> u64 {
        match self {
            Self::Delta(generation, ..)
            | Self::Finished(generation, ..)
            | Self::SignedIn(generation, ..)
            | Self::SignedOut(generation, ..)
            | Self::OtpSent(generation, ..)
            | Self::OtpVerified(generation, ..)
            | Self::PlanFinished(generation, ..)
            | Self::McpConnected(generation, ..)
            | Self::McpCallFinished(generation, ..)
            | Self::CheckoutReady(generation, ..) => *generation,
        }
    }
}

pub struct WgpuRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniforms: ShaderUniforms,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    started_at: std::time::Instant,
    egui_context: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    app_state: AppState,
    api: Option<CenAiApi>,
    runtime: Handle,
    last_dispatched_message_id: u64,
    session_id: String,
    active_user_id: String,
    account_generation: u64,
    response_tx: mpsc::Sender<ChatEvent>,
    response_rx: mpsc::Receiver<ChatEvent>,
    active_streams: HashMap<u64, u64>,
    workspace: Option<WorkspaceStore>,
    last_saved_message_count: usize,
    workspace_root: PathBuf,
    guardian: GuardianMonitor,
    mcp_clients: HashMap<String, Arc<McpHttpClient>>,
}

impl WgpuRenderer {
    pub fn new(window: Arc<Window>, api: Option<CenAiApi>, runtime: Handle, workspace: Option<WorkspaceStore>, workspace_root: PathBuf) -> anyhow::Result<Self> {
        pollster::block_on(Self::new_async(window, api, runtime, workspace, workspace_root))
    }

    async fn new_async(window: Arc<Window>, api: Option<CenAiApi>, runtime: Handle, workspace: Option<WorkspaceStore>, workspace_root: PathBuf) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("No compatible GPU adapter was found"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("CenAI GPU device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
            }, None)
            .await?;
        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("GPU surface has no supported configuration"))?;
        surface.configure(&device, &config);

        let uniforms = ShaderUniforms {
            resolution: [config.width as f32, config.height as f32],
            time_seconds: 0.0,
            intensity: 0.82,
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CenAI ambient uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("CenAI ambient bind layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("CenAI ambient bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("CenAI ambient gradient shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ambient.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("CenAI ambient pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("CenAI ambient surface pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let egui_context = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_context.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );
        let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, None, 1, false);

        let (response_tx, response_rx) = mpsc::channel();
        let mut app_state = AppState::default();
        if let Some(store) = &workspace {
            if let Ok(messages) = store.load_conversation("primary") {
                app_state.conversation.restore_messages(messages);
            }
            app_state.projects = store.projects(false).unwrap_or_default();
        }
        let last_saved_message_count = app_state.conversation.messages().count();
        let session_id = new_session_id();
        let active_user_id = api.as_ref().and_then(|service| service.account_id().ok()).unwrap_or_default();
        app_state.account_email = api.as_ref().and_then(|service| service.session_email().ok()).unwrap_or_default();
        app_state.guardian_consent = api.as_ref().is_some_and(CenAiApi::guardian_consent);
        let mut guardian = GuardianMonitor::default();
        guardian.set_telemetry_consent(app_state.guardian_consent);
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            uniforms,
            uniform_buffer,
            bind_group,
            started_at: std::time::Instant::now(),
            egui_context,
            egui_state,
            egui_renderer,
            app_state,
            api,
            runtime,
            last_dispatched_message_id: 0,
            session_id,
            active_user_id,
            account_generation: 0,
            response_tx,
            response_rx,
            active_streams: HashMap::new(),
            workspace,
            last_saved_message_count,
            workspace_root,
            guardian,
            mcp_clients: HashMap::new(),
        })
    }

    fn dispatch_new_message(&mut self) {
        let latest = self.app_state.conversation.messages()
            .filter(|message| message.role == MessageRole::User)
            .map(|message| (message.id, message.body.clone()))
            .last();
        let Some((message_id, _)) = latest else { return; };
        if message_id <= self.last_dispatched_message_id { return; }
        self.last_dispatched_message_id = message_id;
        self.app_state.conversation.sending = true;

        let Some(api) = self.api.clone() else {
            let _ = self.response_tx.send(ChatEvent::Finished(self.account_generation, message_id, Err("The account service is not initialized.".into())));
            return;
        };
        let latest_goal = self.app_state.conversation.messages()
            .filter(|message| message.role == MessageRole::User)
            .map(|message| message.body.clone()).last().unwrap_or_default();
        if self.app_state.planning_mode {
            let recent_messages = self.app_state.conversation.messages()
                .filter(|message| message.role != MessageRole::System)
                .collect::<Vec<_>>();
            let context = recent_messages.iter().rev().take(8).collect::<Vec<_>>().into_iter().rev()
                .map(|message| format!("{}: {}", if message.role == MessageRole::User { "User" } else { "Assistant" }, message.body))
                .collect::<Vec<_>>().join("\n");
            let user_id = api.account_id();
            let session_id = self.session_id.clone();
            let generation = self.account_generation;
            let brain = CentralBrain::new(api);
            let sender = self.response_tx.clone();
            self.runtime.spawn(async move {
                let result = match user_id {
                    Ok(user_id) => {
                        let request = ContextRequest::new(user_id.clone(), "conversation", session_id.clone());
                        let records = [ContextRecord {
                            owner_user_id: user_id,
                            feature_id: "conversation".into(),
                            session_id: Some(session_id),
                            kind: ContextKind::SessionState,
                            content: context,
                        }];
                        brain.plan(&request, &latest_goal, &records).await
                    }
                    Err(error) => Err(error),
                };
                let _ = sender.send(ChatEvent::PlanFinished(generation, message_id, result));
            });
            return;
        }
        let messages: Vec<ChatMessage> = self.app_state.conversation.messages()
            .map(|message| ChatMessage {
                role: match message.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                }.into(),
                content: message.body.clone(),
            })
            .collect();
        let provider = self.app_state.provider_key_provider.clone();
        let key = api.provider_key(&provider).unwrap_or_default();
        let generation = self.account_generation;
        let sender = self.response_tx.clone();

        if !key.is_empty() {
            let (endpoint, model) = match provider.as_str() {
                "deepseek" => ("https://api.deepseek.com", "deepseek-flash"),
                _ => ("https://api.openai.com/v1", "gpt-4.1-mini"),
            };
            let request = ChatRequest { model: model.into(), messages, max_tokens: 2048, stream: true };
            self.runtime.spawn(async move {
                let delta_sender = sender.clone();
                let result = api.stream_openai_compatible(endpoint, &key, &request, move |delta| {
                    let _ = delta_sender.send(ChatEvent::Delta(generation, message_id, delta.to_owned()));
                }).await;
                let _ = sender.send(ChatEvent::Finished(generation, message_id, result));
            });
        } else {
            let payload = serde_json::json!({
                "model": "cloudflare-gemma-4-26b-a4b-it",
                "messages": messages,
                "max_tokens": 1024,
                "stream": false,
            });
            self.runtime.spawn(async move {
                let result = api.cloud_chat(&payload).await.and_then(|data| {
                    data.pointer("/choices/0/message/content")
                        .and_then(serde_json::Value::as_str)
                        .filter(|answer| !answer.is_empty())
                        .map(str::to_owned)
                        .ok_or_else(|| "CenAI Cloud returned an empty response.".to_owned())
                });
                let _ = sender.send(ChatEvent::Finished(generation, message_id, result));
            });
        }
    }

    fn reset_user_scoped_state(&mut self) {
        self.workspace = None;
        self.app_state.conversation.restore_messages(Vec::new());
        self.app_state.projects.clear();
        self.app_state.provider_key_draft.clear();
        self.app_state.provider_key_status.clear();
        self.app_state.guardian_consent = false;
        self.app_state.mcp_tools.clear();
        self.app_state.mcp_reviewed = false;
        self.app_state.mcp_status.clear();
        self.app_state.mcp_call_result.clear();
        self.last_saved_message_count = 0;
        self.active_streams.clear();
        self.mcp_clients.clear();
        self.guardian = GuardianMonitor::default();
        self.session_id = new_session_id();
        self.account_generation = self.account_generation.saturating_add(1);
    }

    pub fn take_open_url(&mut self) -> Option<String> {
        self.app_state.pending_open_url.take()
    }

    pub fn record_diagnostic(&mut self, kind: DiagnosticKind, component: &str, code: &str) {
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_millis() as u64;
        self.guardian.record(DiagnosticEvent::sanitized(kind, component, code, timestamp));
    }

    pub fn handle_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.egui_state.on_window_event(window, event).repaint
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.uniforms.resolution = [size.width as f32, size.height as f32];
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, window: &Window) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let time = if self.app_state.reduced_motion { 0.0 } else { self.started_at.elapsed().as_secs_f32() };
        self.uniforms.time_seconds = time;
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniforms));

        let raw_input = self.egui_state.take_egui_input(window);
        let full_output = self.egui_context.run(raw_input, |ctx| {
            screen::render(ctx, &mut self.app_state, time);
        });
        if let Some(plan) = self.app_state.pending_checkout_plan.take() {
            if let Some(api) = self.api.clone() {
                let sender = self.response_tx.clone();
                let generation = self.account_generation;
                self.runtime.spawn(async move { let _ = sender.send(ChatEvent::CheckoutReady(generation, api.billing_checkout(&plan).await)); });
            } else { self.app_state.checkout_status = "Sign in before purchasing a membership.".into(); }
        }
        if let Some((email, password)) = self.app_state.pending_sign_in.take() {
            if let Some(api) = self.api.clone() {
                let sender = self.response_tx.clone();
                let generation = self.account_generation;
                self.runtime.spawn(async move { let _ = sender.send(ChatEvent::SignedIn(generation, api.sign_in(&email, &password).await)); });
            } else {
                self.app_state.auth_status = "Account service is unavailable.".into();
            }
        }
        if self.app_state.pending_sign_out {
            self.app_state.pending_sign_out = false;
            if let Some(api) = self.api.clone() {
                let sender = self.response_tx.clone();
                let generation = self.account_generation;
                self.runtime.spawn(async move { let _ = sender.send(ChatEvent::SignedOut(generation, api.sign_out().await)); });
            }
        }
        if self.app_state.pending_send_otp {
            self.app_state.pending_send_otp = false;
            if let Some(api) = self.api.clone() {
                let sender = self.response_tx.clone();
                let generation = self.account_generation;
                self.runtime.spawn(async move { let _ = sender.send(ChatEvent::OtpSent(generation, api.send_email_otp().await)); });
            } else { self.app_state.otp_status = "Account service is unavailable.".into(); }
        }
        if let Some(code) = self.app_state.pending_verify_otp.take() {
            if let Some(api) = self.api.clone() {
                let sender = self.response_tx.clone();
                let generation = self.account_generation;
                self.runtime.spawn(async move { let _ = sender.send(ChatEvent::OtpVerified(generation, api.verify_email_otp(&code).await)); });
            } else { self.app_state.otp_status = "Account service is unavailable.".into(); }
        }
        if let Some(enabled) = self.app_state.pending_guardian_consent.take() {
            self.guardian.set_telemetry_consent(enabled);
            if let Some(api) = self.api.clone() {
                let _ = api.set_guardian_consent(enabled);
                self.runtime.spawn(async move {
                    let _ = api.telemetry(serde_json::json!({ "action": "consent", "usageAnalytics": false, "guardian": enabled })).await;
                });
            }
        }
        if let Some((server_id, tool_name, raw_arguments)) = self.app_state.pending_mcp_call.take() {
            match serde_json::from_str::<serde_json::Value>(&raw_arguments) {
                Ok(arguments) => {
                    if let Some(client) = self.mcp_clients.get(&server_id).cloned() {
                        let sender = self.response_tx.clone();
                        self.runtime.spawn(async move {
                            let result = client.call_tool(&tool_name, arguments, true).await;
                            let _ = sender.send(ChatEvent::McpCallFinished(result));
                        });
                    } else { self.app_state.mcp_status = "MCP server is no longer connected.".into(); }
                }
                Err(error) => self.app_state.mcp_status = format!("Tool arguments are not valid JSON: {error}"),
            }
        }
        if let Some(manifest) = self.app_state.pending_mcp_connect.take() {
            let sender = self.response_tx.clone();
            let generation = self.account_generation;
            self.runtime.spawn(async move {
                let result = async {
                    let client = Arc::new(McpHttpClient::new(manifest.clone(), None)?);
                    let _server_info = client.initialize().await?;
                    let tools = client.list_tools().await?;
                    Ok((manifest, client, tools))
                }.await;
                let _ = sender.send(ChatEvent::McpConnected(generation, result));
            });
        }
        if let Some((provider, key)) = self.app_state.pending_provider_key.take() {
            self.app_state.provider_key_status = match self.api.as_ref() {
                Some(api) => match api.save_provider_key(&provider, &key) {
                    Ok(()) => { self.app_state.provider_key_draft.clear(); if key.is_empty() { "Stored key removed.".into() } else { "API key encrypted and saved.".into() } },
                    Err(error) => format!("Could not save key: {error}"),
                },
                None => "Secure account service is unavailable.".into(),
            };
        }
        if let Some((project_id, artifact_id, file_name, artifact_content)) = self.app_state.pending_artifact.take() {
            if file_name.is_empty() || artifact_content.trim().is_empty() {
                self.app_state.artifact_status = "Add a filename and some content first.".into();
            } else if let Some(store) = &self.workspace {
                self.app_state.artifact_status = match store.save_artifact(&project_id, &artifact_id, &file_name, "text/plain", artifact_content.as_bytes()) {
                    Ok(_) => { self.app_state.projects = store.projects(false).unwrap_or_default(); self.app_state.artifact_name_draft.clear(); self.app_state.artifact_content_draft.clear(); "Artifact saved to your private workspace.".into() },
                    Err(error) => format!("Could not save artifact: {error}"),
                };
            } else {
                self.app_state.artifact_status = "Sign in to save artifacts to your account workspace.".into();
            }
        }
        if let Some((name, description)) = self.app_state.pending_project_create.take() {
            if name.is_empty() {
                self.app_state.project_status = "Enter a project name first.".into();
            } else if let Some(store) = &self.workspace {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                let project = crate::core::workspace::Project {
                    id: format!("project-{now}"), name, description,
                    created_at_unix_ms: now, updated_at_unix_ms: now, archived: false, artifacts: Vec::new(),
                };
                self.app_state.project_status = match store.save_project(project) {
                    Ok(()) => { self.app_state.projects = store.projects(false).unwrap_or_default(); self.app_state.project_name_draft.clear(); self.app_state.project_description_draft.clear(); "Project saved to your private workspace.".into() },
                    Err(error) => format!("Could not save project: {error}"),
                };
            } else {
                self.app_state.project_status = "Sign in to save projects to your account workspace.".into();
            }
        }
        self.dispatch_new_message();
        while let Ok(event) = self.response_rx.try_recv() {
            if event.account_generation() != self.account_generation {
                continue;
            }
            match event {
                ChatEvent::Delta(_, message_id, delta) => {
                    let stream_id = match self.active_streams.get(&message_id) {
                        Some(stream_id) => *stream_id,
                        None => {
                            let stream_id = self.app_state.conversation.begin_stream();
                            self.active_streams.insert(message_id, stream_id);
                            stream_id
                        }
                    };
                    self.app_state.conversation.append_stream(stream_id, &delta);
                }
                ChatEvent::SignedIn(_, result) => {
                    match result {
                        Ok(email) => {
                            let account_id = self.api.as_ref().and_then(|service| service.account_id().ok()).unwrap_or_default();
                            if account_id.is_empty() {
                                self.reset_user_scoped_state();
                                self.active_user_id.clear();
                                self.app_state.auth_status = "Sign-in succeeded but the verified account identity is unavailable.".into();
                                continue;
                            }
                            if self.active_user_id != account_id {
                                self.reset_user_scoped_state();
                            }
                            self.active_user_id = account_id.clone();
                            self.app_state.account_email = email.clone();
                            self.app_state.email_verified = self.api.as_ref().is_some_and(CenAiApi::email_verified);
                            self.app_state.guardian_consent = self.api.as_ref().is_some_and(CenAiApi::guardian_consent);
                            self.guardian.set_telemetry_consent(self.app_state.guardian_consent);
                            self.app_state.auth_status = "Signed in successfully.".into();
                            match WorkspaceStore::new(self.workspace_root.join("cenai-workspace"), &account_id) {
                                Ok(store) => {
                                    if let Ok(messages) = store.load_conversation("primary") { self.app_state.conversation.restore_messages(messages); }
                                    self.app_state.projects = store.projects(false).unwrap_or_default();
                                    self.last_saved_message_count = self.app_state.conversation.messages().count();
                                    self.workspace = Some(store);
                                }
                                Err(error) => self.app_state.auth_status = format!("Signed in, but workspace could not open: {error}"),
                            }
                        }
                        Err(error) => self.app_state.auth_status = format!("Sign-in failed: {error}"),
                    }
                }
                ChatEvent::SignedOut(_, result) => {
                    match result {
                        Ok(()) => {
                            self.reset_user_scoped_state();
                            self.active_user_id.clear();
                            self.app_state.account_email.clear();
                            self.app_state.email_verified = false;
                            self.app_state.otp_status.clear();
                            self.app_state.auth_status = "Signed out.".into();
                            self.workspace = None;
                        }
                        Err(error) => self.app_state.auth_status = format!("Sign-out failed: {error}"),
                    }
                }
                ChatEvent::PlanFinished(_, message_id, result) => {
                    self.app_state.conversation.sending = false;
                    match result {
                        Ok(plan) => {
                            let mut summary = format!("## Proposed plan: {}\n\n", plan.goal);
                            for step in &plan.steps {
                                summary.push_str(&format!("{}. **{:?}** — {}\n", step.id, step.role, step.title));
                            }
                            summary.push_str("\nNothing has been executed. Review this plan before authorizing any actions.");
                            self.app_state.conversation.push(MessageRole::Assistant, summary);
                        }
                        Err(error) => { self.app_state.conversation.push(MessageRole::Assistant, format!("I could not create a safe task plan: {error}")); }
                    }
                    log::debug!("Central Brain plan completed for message {message_id}");
                }
                ChatEvent::CheckoutReady(_, result) => {
                    match result {
                        Ok(url) => { self.app_state.pending_open_url = Some(url); self.app_state.checkout_status = "Opening Stripe checkout…".into(); }
                        Err(error) => self.app_state.checkout_status = format!("Checkout unavailable: {error}"),
                    }
                }
                ChatEvent::McpCallFinished(_, result) => {
                    match result {
                        Ok(value) => { self.app_state.mcp_call_result = value.to_string(); self.app_state.mcp_status = "Approved MCP action completed.".into(); }
                        Err(error) => self.app_state.mcp_status = format!("MCP action failed: {error}"),
                    }
                }
                ChatEvent::McpConnected(_, result) => {
                    match result {
                        Ok((manifest, client, tools)) => {
                            self.mcp_clients.insert(manifest.server_id.clone(), client);
                            self.app_state.mcp_tools = tools;
                            self.app_state.mcp_status = format!("Connected to {} server(s); discovered {} tools. No tool has been executed.", self.mcp_clients.len(), self.app_state.mcp_tools.len());
                        }
                        Err(error) => self.app_state.mcp_status = format!("MCP discovery failed: {error}"),
                    }
                }
                ChatEvent::OtpSent(_, result) => {
                    self.app_state.otp_status = match result { Ok(message) => message, Err(error) => format!("Could not send code: {error}") };
                }
                ChatEvent::OtpVerified(_, result) => {
                    match result {
                        Ok(message) => { self.app_state.email_verified = true; self.app_state.otp_draft.clear(); self.app_state.otp_status = message; }
                        Err(error) => self.app_state.otp_status = format!("Verification failed: {error}"),
                    }
                }
                ChatEvent::Finished(_, message_id, result) => {
                    self.app_state.conversation.sending = false;
                    match result {
                        Ok(answer) => {
                            if let Some(stream_id) = self.active_streams.remove(&message_id) {
                                self.app_state.conversation.finish_stream(stream_id);
                            } else if !answer.is_empty() {
                                let stream_id = self.app_state.conversation.begin_stream();
                                self.app_state.conversation.append_stream(stream_id, &answer);
                                self.app_state.conversation.finish_stream(stream_id);
                            }
                        }
                        Err(error) => {
                            if let Some(stream_id) = self.active_streams.remove(&message_id) {
                                self.app_state.conversation.finish_stream(stream_id);
                            }
                            self.app_state.conversation.push(MessageRole::Assistant, format!("I could not complete that request: {error}"));
                        }
                    }
                    log::debug!("Completed CenAI request for message {message_id}");
                }
            }
        }
        let message_count = self.app_state.conversation.messages().count();
        if message_count != self.last_saved_message_count {
            if let Some(store) = &self.workspace {
                let messages: Vec<_> = self.app_state.conversation.messages().cloned().collect();
                if let Err(error) = store.save_conversation("primary", &messages) {
                    log::warn!("Local conversation autosave failed: {error}");
                }
            }
            self.last_saved_message_count = message_count;
        }
        self.egui_state.handle_platform_output(window, full_output.platform_output);
        let paint_jobs = self.egui_context.tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        for (id, delta) in &full_output.textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
        }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("CenAI frame encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("CenAI ambient background"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        let user_commands = self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("CenAI interface"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.egui_renderer.render(&mut pass.forget_lifetime(), &paint_jobs, &screen_descriptor);
        }

        self.queue.submit(user_commands.into_iter().chain(std::iter::once(encoder.finish())));
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
        frame.present();
        Ok(())
    }
}

impl RenderBackend for WgpuRenderer {
    fn backend_name(&self) -> &'static str {
        "wgpu + egui"
    }
}

//! WGPU renderer hosting CenAI's native egui screens over the animated GPU background.
use std::{collections::HashMap, sync::{mpsc, Arc}};
use tokio::runtime::Handle;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, event::WindowEvent, window::Window};

use crate::{core::{network::CenAiApi, providers::{ChatMessage, ChatRequest}, workspace::WorkspaceStore}, ui::{model::{AppState, MessageRole}, screen}};
use super::RenderBackend;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShaderUniforms {
    resolution: [f32; 2],
    time_seconds: f32,
    intensity: f32,
}

enum ChatEvent {
    Delta(u64, String),
    Finished(u64, Result<String, String>),
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
    response_tx: mpsc::Sender<ChatEvent>,
    response_rx: mpsc::Receiver<ChatEvent>,
    active_streams: HashMap<u64, u64>,
    workspace: Option<WorkspaceStore>,
    last_saved_message_count: usize,
}

impl WgpuRenderer {
    pub fn new(window: Arc<Window>, api: Option<CenAiApi>, runtime: Handle, workspace: Option<WorkspaceStore>) -> anyhow::Result<Self> {
        pollster::block_on(Self::new_async(window, api, runtime, workspace))
    }

    async fn new_async(window: Arc<Window>, api: Option<CenAiApi>, runtime: Handle, workspace: Option<WorkspaceStore>) -> anyhow::Result<Self> {
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
        }
        let last_saved_message_count = app_state.conversation.messages().count();
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
            response_tx,
            response_rx,
            active_streams: HashMap::new(),
            workspace,
            last_saved_message_count,
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

        let Some(api) = self.api.clone() else {
            let _ = self.response_tx.send(ChatEvent::Finished(message_id, Err("The account service is not initialized.".into())));
            return;
        };
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
                    let _ = delta_sender.send(ChatEvent::Delta(message_id, delta.to_owned()));
                }).await;
                let _ = sender.send(ChatEvent::Finished(message_id, result));
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
                let _ = sender.send(ChatEvent::Finished(message_id, result));
            });
        }
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
        if let Some((provider, key)) = self.app_state.pending_provider_key.take() {
            self.app_state.provider_key_status = match self.api.as_ref() {
                Some(api) => match api.save_provider_key(&provider, &key) {
                    Ok(()) => { self.app_state.provider_key_draft.clear(); if key.is_empty() { "Stored key removed.".into() } else { "API key encrypted and saved.".into() } },
                    Err(error) => format!("Could not save key: {error}"),
                },
                None => "Secure account service is unavailable.".into(),
            };
        }
        self.dispatch_new_message();
        while let Ok(event) = self.response_rx.try_recv() {
            match event {
                ChatEvent::Delta(message_id, delta) => {
                    let stream_id = *self.active_streams.entry(message_id).or_insert_with(|| self.app_state.conversation.begin_stream());
                    self.app_state.conversation.append_stream(stream_id, &delta);
                }
                ChatEvent::Finished(message_id, result) => {
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
                            self.active_streams.remove(&message_id);
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

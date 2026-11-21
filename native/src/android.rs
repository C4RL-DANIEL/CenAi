//! Android NativeActivity entrypoint. All application UI and rendering live in Rust.
use std::sync::Arc;

mod vault;
use vault::AndroidSecretVault;
use crate::core::{guardian::DiagnosticKind, network::{CenAiApi, SessionVault}, workspace::WorkspaceStore};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::android::{activity::AndroidApp, EventLoopBuilderExtAndroid},
    window::{Window, WindowId},
};

use crate::render::wgpu_renderer::WgpuRenderer;

struct CenAiApplication {
    api: Option<CenAiApi>,
    runtime: tokio::runtime::Handle,
    workspace: Option<WorkspaceStore>,
    workspace_root: std::path::PathBuf,
    window: Option<Arc<Window>>,
    renderer: Option<WgpuRenderer>,
    last_size: winit::dpi::PhysicalSize<u32>,
}

impl CenAiApplication {
    fn new(api: Option<CenAiApi>, runtime: tokio::runtime::Handle, workspace: Option<WorkspaceStore>, workspace_root: std::path::PathBuf) -> Self {
        Self { api, runtime, workspace, workspace_root, window: None, renderer: None, last_size: winit::dpi::PhysicalSize::new(0, 0) }
    }
}

impl ApplicationHandler for CenAiApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("CenAI")
            .with_transparent(false);
        let window = Arc::new(event_loop.create_window(attributes).expect("create Android window"));
        self.last_size = window.inner_size();
        match WgpuRenderer::new(window.clone(), self.api.clone(), self.runtime.clone(), self.workspace.take(), self.workspace_root.clone()) {
            Ok(renderer) => {
                log::info!("CenAI renderer initialized: {}", crate::RUNTIME_ID);
                self.renderer = Some(renderer);
            }
            Err(error) => {
                log::error!("CenAI GPU initialization failed: {error:#}");
            }
        }
        self.window = Some(window);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(window) = &self.window {
            if self.renderer.as_mut().is_some_and(|renderer| renderer.handle_window_event(window, &event)) {
                window.request_redraw();
            }
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.last_size = size;
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &mut self.renderer {
                    let window = self.window.as_ref().expect("window is initialized");
                    match renderer.render(window) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            renderer.record_diagnostic(DiagnosticKind::RenderFailure, "gpu_surface", "lost_or_outdated");
                            renderer.resize(self.last_size);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            renderer.record_diagnostic(DiagnosticKind::RenderFailure, "gpu_surface", "out_of_memory");
                            event_loop.exit();
                        },
                        Err(wgpu::SurfaceError::Timeout) => {}
                        Err(_) => {}
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

#[no_mangle]
pub fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    let vault = match AndroidSecretVault::new(app.clone()) {
        Ok(vault) => Arc::new(vault),
        Err(error) => { log::error!("Secure vault initialization failed: {error}"); return; }
    };
    let email = vault.account_email().unwrap_or_default();
    let workspace_root = app.internal_data_path().unwrap_or_default();
    let workspace = if email.is_empty() {
        None
    } else {
        app.internal_data_path().and_then(|root| WorkspaceStore::new(root.join("cenai-workspace"), &email).ok())
    };
    let api = CenAiApi::new(vault).ok();
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => { log::error!("Async runtime initialization failed: {error}"); return; }
    };
    let mut builder = EventLoop::builder();
    builder.with_android_app(app);
    let event_loop = builder.build().expect("create Android event loop");
    event_loop
        .run_app(&mut CenAiApplication::new(api, runtime.handle().clone(), workspace, workspace_root))
        .expect("run CenAI Android application");
}

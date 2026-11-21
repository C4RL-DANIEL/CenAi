//! Android NativeActivity entrypoint. All application UI and rendering live in Rust.
use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::android::{activity::AndroidApp, EventLoopBuilderExtAndroid},
    window::{Window, WindowId},
};

use crate::render::wgpu_renderer::WgpuRenderer;

#[derive(Default)]
struct CenAiApplication {
    window: Option<Arc<Window>>,
    renderer: Option<WgpuRenderer>,
    last_size: winit::dpi::PhysicalSize<u32>,
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
        match WgpuRenderer::new(window.clone()) {
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
                    match renderer.render() {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            renderer.resize(self.last_size);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
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
    let mut builder = EventLoop::builder();
    builder.with_android_app(app);
    let event_loop = builder.build().expect("create Android event loop");
    event_loop
        .run_app(&mut CenAiApplication::default())
        .expect("run CenAI Android application");
}

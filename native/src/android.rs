//! Android NativeActivity entrypoint. All application UI and rendering live in Rust.
use std::{collections::VecDeque, sync::{Arc, Mutex, OnceLock}};
use jni::{objects::{JClass, JObject, JString, JValue}, sys::jint, JNIEnv, JavaVM};

mod vault;
pub(crate) mod audio;
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

#[derive(Debug)]
enum AndroidPlatformEvent {
    FilePicked { request_code: i32, path: String, mime_type: String, display_name: String, error: String },
}

static PLATFORM_EVENTS: OnceLock<Mutex<VecDeque<AndroidPlatformEvent>>> = OnceLock::new();

fn push_platform_event(event: AndroidPlatformEvent) {
    let queue = PLATFORM_EVENTS.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Ok(mut queue) = queue.lock() {
        if queue.len() < 8 { queue.push_back(event); }
    }
}

fn pop_platform_event() -> Option<AndroidPlatformEvent> {
    PLATFORM_EVENTS.get()?.lock().ok()?.pop_front()
}

fn java_string(env: &mut JNIEnv<'_>, value: JString<'_>) -> String {
    env.get_string(&value).map(|value| value.into()).unwrap_or_default()
}

/// AndroidApp owns this JNI global reference. Create a thread-local reference
/// without allowing JObject's Drop to delete the AndroidApp-owned global.
fn activity_local<'local>(env: &mut JNIEnv<'local>, app: &AndroidApp) -> jni::errors::Result<JObject<'local>> {
    let global = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let local = env.new_local_ref(&global);
    std::mem::forget(global);
    local
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_CenAiNativeActivity_nativeOnFilePicked(
    mut env: JNIEnv<'_>, _class: JClass<'_>, request_code: jint,
    path: JString<'_>, mime_type: JString<'_>, display_name: JString<'_>, error: JString<'_>,
) {
    push_platform_event(AndroidPlatformEvent::FilePicked {
        request_code,
        path: java_string(&mut env, path),
        mime_type: java_string(&mut env, mime_type),
        display_name: java_string(&mut env, display_name),
        error: java_string(&mut env, error),
    });
}

struct CenAiApplication {
    api: Option<CenAiApi>,
    runtime: tokio::runtime::Handle,
    workspace: Option<WorkspaceStore>,
    workspace_root: std::path::PathBuf,
    android_app: AndroidApp,
    window: Option<Arc<Window>>,
    renderer: Option<WgpuRenderer>,
    last_size: winit::dpi::PhysicalSize<u32>,
}

impl CenAiApplication {
    fn new(api: Option<CenAiApi>, runtime: tokio::runtime::Handle, workspace: Option<WorkspaceStore>, workspace_root: std::path::PathBuf, android_app: AndroidApp) -> Self {
        Self { api, runtime, workspace, workspace_root, android_app, window: None, renderer: None, last_size: winit::dpi::PhysicalSize::new(0, 0) }
    }

    fn invoke_activity_method(app: AndroidApp, method: &'static str) {
        let callback_app = app.clone();
        app.run_on_java_main_thread(Box::new(move || {
            let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
            let Ok(vm) = vm else { log::error!("Could not attach to Android Java VM"); return; };
            let Ok(mut env) = vm.attach_current_thread() else { log::error!("Could not attach JNI thread"); return; };
            let Ok(activity) = activity_local(&mut env, &callback_app) else { log::error!("Could not access CenAI activity on JNI thread"); return; };
            if let Err(error) = env.call_method(&activity, method, "()V", &[]) {
                log::error!("Could not launch Android picker {method}: {error}");
            }
        }));
    }

    fn open_external_url(app: AndroidApp, url: String) {
        if !url.starts_with("https://checkout.stripe.com/") { log::warn!("Blocked external checkout URL outside the Stripe allowlist"); return; }
        let callback_app = app.clone();
        app.run_on_java_main_thread(Box::new(move || {
            let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
            let Ok(vm) = vm else { log::error!("Could not attach to Android Java VM"); return; };
            let Ok(mut env) = vm.attach_current_thread() else { log::error!("Could not attach JNI thread"); return; };
            let activity = unsafe { JObject::from_raw(callback_app.activity_as_ptr().cast()) };
            let result = (|| -> jni::errors::Result<()> {
                let uri_string = env.new_string(url)?;
                let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&uri_string)])?.l()?;
                let action = env.new_string("android.intent.action.VIEW")?;
                let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[JValue::Object(&action), JValue::Object(&uri)])?;
                env.call_method(&activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
                Ok(())
            })();
            if let Err(error) = result { log::error!("Could not open checkout: {error}"); }
        }));
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
        match WgpuRenderer::new_android(window.clone(), self.api.clone(), self.runtime.clone(), self.workspace.take(), self.workspace_root.clone(), self.android_app.clone()) {
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
                    while let Some(platform_event) = pop_platform_event() {
                        match platform_event {
                            AndroidPlatformEvent::FilePicked { request_code, path, mime_type, display_name, error } => {
                                match request_code {
                                    8101 => renderer.handle_attachment_import(path, mime_type, display_name, error),
                                    8102 => renderer.handle_backup_file_import(path, error),
                                    _ => log::warn!("Ignoring unknown Android file picker request {request_code}"),
                                }
                            }
                        }
                    }
                    if renderer.take_attachment_picker_request() {
                        Self::invoke_activity_method(self.android_app.clone(), "openAttachmentPicker");
                    }
                    if renderer.take_backup_picker_request() {
                        Self::invoke_activity_method(self.android_app.clone(), "openBackupPicker");
                    }
                    if let Some(url) = renderer.take_open_url() {
                        Self::open_external_url(self.android_app.clone(), url);
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
    let account_id = vault.account_id().unwrap_or_default();
    let workspace_root = app.internal_data_path().unwrap_or_default();
    let workspace = if account_id.is_empty() {
        None
    } else {
        app.internal_data_path().and_then(|root| WorkspaceStore::new(root.join("cenai-workspace"), &account_id).ok())
    };
    let api = CenAiApi::new(vault).ok();
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => { log::error!("Async runtime initialization failed: {error}"); return; }
    };
    let mut builder = EventLoop::builder();
    builder.with_android_app(app.clone());
    let event_loop = builder.build().expect("create Android event loop");
    event_loop
        .run_app(&mut CenAiApplication::new(api, runtime.handle().clone(), workspace, workspace_root, app.clone()))
        .expect("run CenAI Android application");
}

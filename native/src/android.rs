//! Android NativeActivity entrypoint. All application UI and rendering live in Rust.
use std::{collections::VecDeque, sync::{Arc, Mutex, OnceLock}};
use jni::{objects::{JClass, JObject, JString, JValue}, sys::{jboolean, jint}, JNIEnv, JavaVM};

mod vault;
pub(crate) mod audio;
use vault::AndroidSecretVault;
use crate::core::{automations::reminder_notification_tag, guardian::DiagnosticKind, network::{CenAiApi, SessionVault}, workspace::WorkspaceStore};

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
    AgentActionResult { name: String, success: bool, message: String },
    OAuthCallback(String),
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

fn export_legacy_secure_store(app: &AndroidApp) -> Result<String, String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
    let activity = activity_local(&mut env, app).map_err(|error| error.to_string())?;
    let value = env.call_method(&activity, "exportLegacySecureStore", "()Ljava/lang/String;", &[])
        .map_err(|error| error.to_string())?.l().map_err(|error| error.to_string())?;
    let value = JString::from(value);
    env.get_string(&value).map(|value| value.into()).map_err(|error| error.to_string())
}

fn export_legacy_conversation_history(app: &AndroidApp) -> Result<String, String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
    let activity = activity_local(&mut env, app).map_err(|error| error.to_string())?;
    let value = env.call_method(&activity, "exportLegacyConversationHistory", "()Ljava/lang/String;", &[])
        .map_err(|error| error.to_string())?.l().map_err(|error| error.to_string())?;
    let value = JString::from(value);
    env.get_string(&value).map(|value| value.into()).map_err(|error| error.to_string())
}

fn clear_legacy_secure_store(app: &AndroidApp) -> Result<(), String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
    let activity = activity_local(&mut env, app).map_err(|error| error.to_string())?;
    env.call_method(&activity, "clearLegacySecureStoreAfterMigration", "()V", &[])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn current_app_version(app: &AndroidApp) -> Result<String, String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
    let activity = activity_local(&mut env, app).map_err(|error| error.to_string())?;
    let value = env.call_method(&activity, "getInstalledVersionName", "()Ljava/lang/String;", &[])
        .map_err(|error| error.to_string())?.l().map_err(|error| error.to_string())?;
    let value = JString::from(value);
    env.get_string(&value).map(|value| value.into()).map_err(|error| error.to_string())
}

/// Dispatch a model-requested Android action only after the native approval dialog
/// has explicitly authorized this one request. Validate the exact argument shape again
/// at the platform boundary before crossing JNI.
pub(crate) fn execute_approved_agent_action(app: AndroidApp, name: String, arguments: serde_json::Value) {
    let object = arguments.as_object();
    let value = match name.as_str() {
        "open_url" if object.is_some_and(|o| o.len() == 1 && o.contains_key("url")) => {
            let Some(url) = arguments.get("url").and_then(serde_json::Value::as_str) else {
                log::warn!("Rejected approved URL action with invalid arguments"); return;
            };
            if url.len() > 4096 || !reqwest::Url::parse(url).ok().is_some_and(|parsed| {
                matches!(parsed.scheme(), "https" | "http") && parsed.host_str().is_some()
                    && parsed.username().is_empty() && parsed.password().is_none()
            }) {
                log::warn!("Rejected approved URL action outside the HTTP(S) allowlist"); return;
            }
            url.to_owned()
        }
        "open_dialer" if object.is_some_and(|o| o.len() == 1 && o.contains_key("number")) => {
            let Some(number) = arguments.get("number").and_then(serde_json::Value::as_str) else {
                log::warn!("Rejected approved dialer action with invalid arguments"); return;
            };
            if !(4..=32).contains(&number.len()) || !number.chars().all(|c|
                c.is_ascii_digit() || "+*#(). -".contains(c)) {
                log::warn!("Rejected approved dialer action with invalid number"); return;
            }
            number.to_owned()
        }
        "share_text" if object.is_some_and(|o| o.len() == 1 && o.contains_key("text")) => {
            let Some(text) = arguments.get("text").and_then(serde_json::Value::as_str) else {
                log::warn!("Rejected approved share action with invalid arguments"); return;
            };
            if text.trim().is_empty() || text.len() > 64 * 1024 {
                log::warn!("Rejected approved share action outside the size limit"); return;
            }
            text.to_owned()
        }
        "open_app_settings" if object.is_some_and(|object| object.is_empty()) => String::new(),
        _ => {
            log::warn!("Rejected unsupported or malformed approved Android action");
            return;
        }
    };

    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else {
            push_platform_event(AndroidPlatformEvent::AgentActionResult {
                name, success: false, message: "Android runtime is unavailable.".into(),
            });
            return;
        };
        let Ok(mut env) = vm.attach_current_thread() else {
            push_platform_event(AndroidPlatformEvent::AgentActionResult {
                name, success: false, message: "Could not attach to the Android runtime.".into(),
            });
            return;
        };
        let Ok(activity) = activity_local(&mut env, &callback_app) else {
            push_platform_event(AndroidPlatformEvent::AgentActionResult {
                name, success: false, message: "CenAI activity is unavailable.".into(),
            });
            return;
        };
        let result = (|| -> jni::errors::Result<()> {
            let action = env.new_string(&name)?;
            let value = env.new_string(&value)?;
            env.call_method(
                &activity,
                "performAgentAction",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
                &[JValue::Object(&action), JValue::Object(&value)],
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            push_platform_event(AndroidPlatformEvent::AgentActionResult {
                name, success: false, message: format!("Android action bridge failed: {error}"),
            });
        }
    }));
}

/// Schedule a persistent OS-managed reminder for a user-owned automation.
/// The worker only posts a notification; the task itself remains approval-gated in Rust.
pub(crate) fn schedule_automation_reminder(app: AndroidApp, owner_scope: String, task_id: String, delay_ms: u64) {
    let Some(notification_tag) = reminder_notification_tag(&owner_scope, &task_id) else {
        log::warn!("Rejected invalid automation reminder identity");
        return;
    };
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else { log::error!("Could not attach to Android VM for automation reminder"); return; };
        let Ok(mut env) = vm.attach_current_thread() else { return; };
        let Ok(activity) = activity_local(&mut env, &callback_app) else { return; };
        let result = (|| -> jni::errors::Result<()> {
            let tag = env.new_string(notification_tag)?;
            env.call_static_method(
                "com/cenai/app/AutomationReminderBridge",
                "schedule",
                "(Landroid/content/Context;Ljava/lang/String;J)V",
                &[
                    JValue::Object(&activity),
                    JValue::Object(&tag),
                    JValue::Long(delay_ms.min(i64::MAX as u64) as jni::sys::jlong),
                ],
            )?;
            env.call_method(&activity, "requestNotificationPermissionIfNeeded", "()V", &[])?;
            Ok(())
        })();
        if let Err(error) = result { log::error!("Could not schedule CenAI automation reminder: {error}"); }
    }));
}

pub(crate) fn cancel_automation_reminder(app: AndroidApp, owner_scope: String, task_id: String) {
    let Some(notification_tag) = reminder_notification_tag(&owner_scope, &task_id) else {
        log::warn!("Rejected invalid automation reminder cancellation identity");
        return;
    };
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else { log::error!("Could not attach to Android VM to cancel automation reminder"); return; };
        let Ok(mut env) = vm.attach_current_thread() else { return; };
        let Ok(activity) = activity_local(&mut env, &callback_app) else { return; };
        let result = (|| -> jni::errors::Result<()> {
            let tag = env.new_string(notification_tag)?;
            env.call_static_method(
                "com/cenai/app/AutomationReminderBridge",
                "cancel",
                "(Landroid/content/Context;Ljava/lang/String;)V",
                &[JValue::Object(&activity), JValue::Object(&tag)],
            )?;
            Ok(())
        })();
        if let Err(error) = result { log::error!("Could not cancel CenAI automation reminder: {error}"); }
    }));
}

/// Release the resident Lightning engine when Android backgrounds the app.
/// The verified model remains in private app storage and is reloaded lazily on the
/// next local request, avoiding a permanently resident ~600 MB model while idle.
pub(crate) fn release_lightning_runtime(app: AndroidApp) {
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else { log::warn!("Could not access Android VM to release Lightning"); return; };
        let Ok(mut env) = vm.attach_current_thread() else { log::warn!("Could not attach JNI thread to release Lightning"); return; };
        if let Err(error) = env.call_static_method(
            "com/cenai/app/LightningRuntimeBridge", "release", "()V", &[],
        ) {
            log::warn!("Could not release the resident Lightning engine: {error}");
        }
    }));
}

/// Invoke the fixed, on-device Lightning runtime. This JNI adapter is called only
/// from a blocking worker; the model policy and fallback decision remain in Rust.
pub(crate) fn lightning_generate(app: AndroidApp, prompt: &str) -> Result<String, String> {
    crate::core::lightning::validate_prompt(prompt).map_err(str::to_owned)?;
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|error| format!("Lightning could not access the Android VM: {error}"))?;
    let mut env = vm.attach_current_thread()
        .map_err(|error| format!("Lightning could not attach to the Android VM: {error}"))?;
    let activity = activity_local(&mut env, &app)
        .map_err(|error| format!("Lightning could not access the Android context: {error}"))?;
    let prompt = env.new_string(prompt)
        .map_err(|error| format!("Lightning prompt conversion failed: {error}"))?;
    let output = env.call_static_method(
        "com/cenai/app/LightningRuntimeBridge",
        "generate",
        "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
        &[JValue::Object(&activity), JValue::Object(&prompt)],
    ).and_then(|value| value.l())
        .map_err(|error| format!("Lightning local inference failed: {error}"))?;
    let output = JString::from(output);
    env.get_string(&output)
        .map(|value| value.into())
        .map_err(|error| format!("Lightning response conversion failed: {error}"))
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

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_CenAiNativeActivity_nativeOnPlatformActionResult(
    mut env: JNIEnv<'_>, _class: JClass<'_>, name: JString<'_>, success: jboolean, message: JString<'_>,
) {
    push_platform_event(AndroidPlatformEvent::AgentActionResult {
        name: java_string(&mut env, name),
        success: success != 0,
        message: java_string(&mut env, message),
    });
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_CenAiNativeActivity_nativeOnOAuthCallback(
    mut env: JNIEnv<'_>, _class: JClass<'_>, callback: JString<'_>,
) {
    let callback = java_string(&mut env, callback);
    if callback.len() <= 8 * 1024 {
        push_platform_event(AndroidPlatformEvent::OAuthCallback(callback));
    }
}

/// Launch Google's validated authorization URL through the narrow Android adapter.
pub(crate) fn open_google_oauth(app: AndroidApp, url: String) {
    let parsed = reqwest::Url::parse(&url);
    if !parsed.ok().is_some_and(|uri| uri.scheme() == "https"
        && uri.host_str() == Some("accounts.google.com")
        && uri.path() == "/o/oauth2/v2/auth") {
        log::warn!("Rejected Google OAuth URL outside the official authorization endpoint");
        return;
    }
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else { return; };
        let Ok(mut env) = vm.attach_current_thread() else { return; };
        let Ok(activity) = activity_local(&mut env, &callback_app) else { return; };
        let Ok(url_string) = env.new_string(url) else { return; };
        if let Err(error) = env.call_method(
            &activity, "openGoogleOAuth", "(Ljava/lang/String;)Z",
            &[JValue::Object(&url_string)],
        ) {
            log::error!("Could not launch Google OAuth: {error}");
        }
    }));
}

/// Ask Android to re-check package identity/signing certificate and show its
/// system installer. The user must still approve the installation in Android.
pub(crate) fn install_verified_update(app: AndroidApp, path: String) {
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else { return; };
        let Ok(mut env) = vm.attach_current_thread() else { return; };
        let Ok(activity) = activity_local(&mut env, &callback_app) else { return; };
        let Ok(path_string) = env.new_string(path) else { return; };
        match env.call_method(
            &activity, "installVerifiedUpdate", "(Ljava/lang/String;)Z",
            &[JValue::Object(&path_string)],
        ) {
            Ok(result) => {
                if !result.z().unwrap_or(false) {
                    log::warn!("Android declined to open the verified update installer; check install-source permission and retry");
                }
            }
            Err(error) => log::error!("Verified update installer bridge failed: {error}"),
        }
    }));
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
            let Ok(activity) = activity_local(&mut env, &callback_app) else { log::error!("Could not access CenAI activity on JNI thread"); return; };
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
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Release model memory in the background; the verified model reloads lazily.
        release_lightning_runtime(self.android_app.clone());
    }

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
                            AndroidPlatformEvent::AgentActionResult { name, success, message } => {
                                renderer.handle_agent_action_result(name, success, message);
                            }
                            AndroidPlatformEvent::OAuthCallback(callback) => {
                                renderer.handle_google_oauth_callback(callback);
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
    match export_legacy_secure_store(&app) {
        Ok(export) if export != "{}" && !export.is_empty() => {
            match vault.migrate_legacy_export(&export) {
                Ok(true) => match clear_legacy_secure_store(&app) {
                    Ok(()) => log::info!("Legacy Kotlin credentials migrated into the Rust account vault."),
                    Err(error) => log::warn!("Legacy credentials migrated, but old-store cleanup was deferred: {error}"),
                },
                Ok(false) => {},
                Err(error) => log::warn!("Legacy credential migration was deferred; original encrypted data was retained: {error}"),
            }
        }
        Ok(_) => {},
        Err(error) => log::warn!("Legacy secure-store export was unavailable; original data was retained: {error}"),
    }
    let workspace_root = app.internal_data_path().unwrap_or_default();
    let api = CenAiApi::new(vault).ok();
    // Always construct the workspace through the authenticated API so its
    // index, conversations and artifacts use the Android Keystore-backed
    // account/path-bound encryption adapter. Never fall back to plaintext.
    let workspace = match (&api, app.internal_data_path()) {
        (Some(api), Some(root)) => match api.workspace_store(root.join("cenai-workspace")) {
            Ok(workspace) => Some(workspace),
            Err(error) => {
                log::error!("Encrypted workspace initialization failed: {error}");
                None
            }
        },
        _ => None,
    };
    if let Some(workspace_store) = workspace.as_ref() {
        match export_legacy_conversation_history(&app) {
            Ok(export) if !export.is_empty() && export != "[]" => {
                match workspace_store.import_legacy_conversations(&export) {
                    Ok(count) if count > 0 => log::info!("Imported {count} legacy conversations into the encrypted Rust workspace."),
                    Ok(_) => log::info!("Legacy conversation history is already present in the Rust workspace."),
                    Err(error) => log::warn!("Legacy history import was deferred; original history was retained: {error}"),
                }
            }
            Ok(_) => log::info!("No legacy conversation history was found."),
            Err(error) => log::warn!("Legacy history export was unavailable; original history was retained: {error}"),
        }
    }
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

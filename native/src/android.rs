//! Android NativeActivity entrypoint. All application UI and rendering live in Rust.
use std::{collections::VecDeque, sync::{Arc, Mutex, OnceLock}};
use jni::{objects::{JClass, JObject, JString, JValue}, sys::{jboolean, jint, jlong}, JNIEnv, JavaVM};

mod vault;
pub(crate) mod audio;
use vault::AndroidSecretVault;
use crate::core::{automations::{reminder_notification_tag, AutomationScheduler}, brain::CentralBrain, guardian::DiagnosticKind, network::CenAiApi, workspace::WorkspaceStore};

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
    AutomationReminder(String),
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

pub(crate) fn migrate_legacy_history(
    app: &AndroidApp,
    workspace: &WorkspaceStore,
    expected_email: &str,
) -> Result<usize, String> {
    if expected_email.trim().is_empty() {
        return Err("The signed-in account email is unavailable; legacy history was not imported".into());
    }
    // Keep the old encrypted account store until history import succeeds. Its
    // explicitly verified email is the only authority for ownerless legacy chats.
    let legacy_secure_export = export_legacy_secure_store(app).ok();
    let legacy_identity = legacy_secure_export.as_deref()
        .and_then(crate::core::legacy_history::verified_legacy_owner_email);
    let export = export_legacy_conversation_history(app)?;
    if export.trim().is_empty() || export.trim() == "[]" {
        return Ok(0);
    }
    // Keep the original encrypted preferences as a rollback source until the
    // account, provider-key and conversation migrations all have device-level
    // verification. Re-import is idempotent and cannot overwrite Rust chats.
    workspace.import_legacy_conversations(&export, expected_email, legacy_identity.as_deref())
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

/// Reconcile recurring automation reminders in Android's durable WorkManager queue.
pub(crate) fn schedule_automation_schedule_refresh_worker(app: AndroidApp) {
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else { log::error!("Could not attach to Android VM to schedule automation refresh"); return; };
        let Ok(mut env) = vm.attach_current_thread() else { return; };
        let Ok(activity) = activity_local(&mut env, &callback_app) else { return; };
        if let Err(error) = env.call_static_method(
            "com/cenai/app/AutomationReminderBridge", "scheduleRefresh", "(Landroid/content/Context;)V",
            &[JValue::Object(&activity)],
        ) { log::warn!("Could not schedule recurring CenAI automation reconciliation: {error}"); }
    }));
}

pub(crate) fn cancel_automation_schedule_refresh_worker(app: AndroidApp) {
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else { return; };
        let Ok(mut env) = vm.attach_current_thread() else { return; };
        let Ok(activity) = activity_local(&mut env, &callback_app) else { return; };
        if let Err(error) = env.call_static_method(
            "com/cenai/app/AutomationReminderBridge", "cancelRefresh", "(Landroid/content/Context;)V",
            &[JValue::Object(&activity)],
        ) { log::warn!("Could not cancel recurring CenAI automation reconciliation: {error}"); }
    }));
}

/// Schedule OS-managed background cognitive work. The Rust worker executes
/// only explicitly authorized research/analysis/writing steps; it has no tools.
pub(crate) fn schedule_background_task_worker(app: AndroidApp) {
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else { log::error!("Could not attach to Android VM to schedule background tasks"); return; };
        let Ok(mut env) = vm.attach_current_thread() else { return; };
        let Ok(activity) = activity_local(&mut env, &callback_app) else { return; };
        if let Err(error) = env.call_static_method(
            "com/cenai/app/BackgroundTaskBridge", "schedule", "(Landroid/content/Context;)V",
            &[JValue::Object(&activity)],
        ) { log::error!("Could not schedule CenAI background cognitive work: {error}"); }
    }));
}

pub(crate) fn cancel_background_task_worker(app: AndroidApp) {
    let callback_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let vm = unsafe { JavaVM::from_raw(callback_app.vm_as_ptr().cast()) };
        let Ok(vm) = vm else { return; };
        let Ok(mut env) = vm.attach_current_thread() else { return; };
        let Ok(activity) = activity_local(&mut env, &callback_app) else { return; };
        if let Err(error) = env.call_static_method(
            "com/cenai/app/BackgroundTaskBridge", "cancel", "(Landroid/content/Context;)V",
            &[JValue::Object(&activity)],
        ) { log::warn!("Could not cancel CenAI background cognitive work: {error}"); }
    }));
}

async fn run_background_cognitive_batch(api: CenAiApi, workspace: WorkspaceStore) -> Result<usize, String> {
    let brain = CentralBrain::new(api);
    let records = workspace.task_plans()?;
    let mut executed = 0usize;
    for record in records {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_millis().min(u64::MAX as u128) as u64;
        let Some(plan) = workspace.recover_interrupted_background_steps(&record.id, now, 15 * 60 * 1_000)? else { continue; };
        if !plan.background_authorized || plan.stopped || plan.is_paused() { continue; }
        for step_id in plan.ready_steps().into_iter().take(2usize.saturating_sub(executed)) {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_millis().min(u64::MAX as u128) as u64;
            let Some(claimed) = workspace.claim_background_step(&record.id, step_id, now)? else { continue; };
            // This cognitive worker has no device, account, tool, or code-execution access.
            let result = brain.execute_step(&claimed, step_id).await;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_millis().min(u64::MAX as u128) as u64;
            if workspace.finish_background_step(&record.id, step_id, result, now)? { executed += 1; }
            if executed >= 2 { return Ok(executed); }
        }
    }
    Ok(executed)
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

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_AutomationScheduleRefreshWorker_nativeRefreshAutomationSchedules(
    mut env: JNIEnv<'_>, _class: JClass<'_>, context: JObject<'_>,
) -> jint {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info));
    let result = (|| -> Result<usize, String> {
        let vault = AndroidSecretVault::new_from_context(&mut env, &context)?;
        let files_dir = vault.files_dir().to_path_buf();
        let api = CenAiApi::new(Arc::new(vault)).map_err(|error| error.to_string())?;
        if !api.has_session() { return Ok(0); }
        let owner_scope = api.account_id()?;
        if owner_scope.is_empty() { return Ok(0); }
        let workspace = api.workspace_store(files_dir.join("cenai-workspace"))?;
        let mut scheduler = AutomationScheduler::from_workspace(&workspace)?;
        let now = chrono::Utc::now();
        let due = scheduler.poll_due_and_persist(now, &workspace)?;
        let automations = workspace.automations()?;
        let mut scheduled = 0usize;
        for automation in automations {
            if automation.owner_scope != owner_scope { continue; }
            let Some(tag) = reminder_notification_tag(&owner_scope, &automation.id) else { continue; };
            if !automation.enabled {
                let tag_value = env.new_string(&tag).map_err(|error| error.to_string())?;
                env.call_static_method(
                    "com/cenai/app/AutomationReminderBridge", "cancel", "(Landroid/content/Context;Ljava/lang/String;)V",
                    &[JValue::Object(&context), JValue::Object(&tag_value)],
                ).map_err(|error| error.to_string())?;
                continue;
            }
            let just_became_due = due.iter().any(|trigger| trigger.automation_id == automation.id && trigger.owner_scope == owner_scope);
            let next = if just_became_due {
                Some(now)
            } else if automation.pending_scheduled_at.is_some() {
                // A prior notification is pending; avoid creating a 15-minute notification loop.
                None
            } else {
                automation.next_run_after(now).map_err(str::to_owned)?
            };
            let Some(next) = next else { continue; };
            let delay = (next - now).num_milliseconds().max(0) as jlong;
            let tag_value = env.new_string(&tag).map_err(|error| error.to_string())?;
            env.call_static_method(
                "com/cenai/app/AutomationReminderBridge", "schedule",
                "(Landroid/content/Context;Ljava/lang/String;J)V",
                &[JValue::Object(&context), JValue::Object(&tag_value), JValue::Long(delay)],
            ).map_err(|error| error.to_string())?;
            scheduled += 1;
        }
        Ok(scheduled)
    })();
    match result {
        Ok(count) => count.min(i32::MAX as usize) as jint,
        Err(error) => { log::warn!("CenAI automation schedule refresh deferred: {error}"); -1 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_BackgroundTaskWorker_nativeRunBackgroundTasks(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    context: JObject<'_>,
) -> jint {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info));
    let result = (|| -> Result<usize, String> {
        let vault = AndroidSecretVault::new_from_context(&mut env, &context)?;
        let files_dir = vault.files_dir().to_path_buf();
        let api = CenAiApi::new(Arc::new(vault)).map_err(|error| error.to_string())?;
        if !api.has_session() { return Ok(0); }
        let account_id = api.account_id()?;
        if account_id.is_empty() { return Ok(0); }
        let workspace = api.workspace_store(files_dir.join("cenai-workspace"))?;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all()
            .build().map_err(|error| format!("Could not start background runtime: {error}"))?;
        runtime.block_on(run_background_cognitive_batch(api, workspace))
    })();
    match result {
        Ok(completed) => completed.min(i32::MAX as usize) as jint,
        Err(error) => { log::warn!("CenAI background cognitive worker deferred: {error}"); -1 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_CenAiNativeActivity_nativeOnAutomationReminder(
    mut env: JNIEnv<'_>, _class: JClass<'_>, notification_tag: JString<'_>,
) {
    let tag = java_string(&mut env, notification_tag);
    if tag.len() == 81 && tag.starts_with("cenai_automation_")
        && tag[17..].bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        push_platform_event(AndroidPlatformEvent::AutomationReminder(tag));
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
                            AndroidPlatformEvent::AutomationReminder(tag) => {
                                renderer.handle_automation_reminder(tag);
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
    let legacy_secure_export = export_legacy_secure_store(&app).ok();
    match legacy_secure_export.as_deref() {
        Some(export) if export != "{}" && !export.is_empty() => {
            match vault.migrate_legacy_export(export) {
                Ok(true) => log::info!("Legacy Kotlin credentials migrated into the Rust account vault; old-store cleanup remains deferred until device-level migration verification."),
                Ok(false) => {},
                Err(error) => log::warn!("Legacy credential migration was deferred; original encrypted data was retained: {error}"),
            }
        }
        Some(_) => {},
        None => log::warn!("Legacy secure-store export was unavailable; original data was retained."),
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
        let expected_email = api.as_ref().and_then(|service| service.session_email().ok()).unwrap_or_default();
        match migrate_legacy_history(&app, workspace_store, &expected_email) {
            Ok(count) if count > 0 => log::info!("Imported {count} legacy conversations into the encrypted Rust workspace."),
            Ok(_) => log::info!("Legacy conversation history is already present or no legacy history was found."),
            Err(error) => log::warn!("Legacy history import was deferred; original history was retained: {error}"),
        }
    }
    if workspace.as_ref().is_some_and(|store| store.task_plans().unwrap_or_default().iter()
        .any(|record| record.plan.background_authorized && !record.plan.stopped)) {
        schedule_background_task_worker(app.clone());
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

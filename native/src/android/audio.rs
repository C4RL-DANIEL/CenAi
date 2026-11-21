//! Android microphone capture using AudioRecord, exposed as bounded PCM16 frames to Rust.
use std::{
    sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
};
use jni::{objects::{GlobalRef, JByteArray, JObject, JValue}, JavaVM};
use winit::platform::android::activity::AndroidApp;
use crate::core::voice::AudioFrame;

const SAMPLE_RATE: i32 = 16_000;
const CHANNEL_IN_MONO: i32 = 16;
const ENCODING_PCM_16BIT: i32 = 2;
const AUDIO_SOURCE_MIC: i32 = 1;
const REQUEST_CODE: i32 = 7_331;

pub struct AndroidAudioCapture {
    app: AndroidApp,
    sender: mpsc::Sender<Result<AudioFrame, String>>,
    receiver: mpsc::Receiver<Result<AudioFrame, String>>,
    stop_flag: Arc<AtomicBool>,
    recorder: Arc<Mutex<Option<GlobalRef>>>,
    worker: Option<JoinHandle<()>>,
    permission_requested: bool,
}

impl AndroidAudioCapture {
    pub fn new(app: AndroidApp) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            app,
            sender,
            receiver,
            stop_flag: Arc::new(AtomicBool::new(false)),
            recorder: Arc::new(Mutex::new(None)),
            worker: None,
            permission_requested: false,
        }
    }

    fn with_env<T>(&self, operation: impl FnOnce(&mut jni::JNIEnv<'_>) -> jni::errors::Result<T>) -> Result<T, String> {
        let vm = unsafe { JavaVM::from_raw(self.app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
        operation(&mut env).map_err(|error| error.to_string())
    }

    pub fn has_permission(&self) -> Result<bool, String> {
        self.with_env(|env| {
            let activity = unsafe { JObject::from_raw(self.app.activity_as_ptr().cast()) };
            let permission = env.new_string("android.permission.RECORD_AUDIO")?;
            let result = env.call_method(
                &activity,
                "checkSelfPermission",
                "(Ljava/lang/String;)I",
                &[JValue::Object(&permission)],
            )?.i()?;
            Ok(result == 0)
        })
    }

    fn request_permission(&mut self) {
        if self.permission_requested { return; }
        self.permission_requested = true;
        let app = self.app.clone();
        app.run_on_java_main_thread(Box::new(move || {
            let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
            let Ok(vm) = vm else { log::error!("Could not attach to Android VM for microphone permission"); return; };
            let Ok(mut env) = vm.attach_current_thread() else { return; };
            let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
            let result = (|| -> jni::errors::Result<()> {
                let permission = env.new_string("android.permission.RECORD_AUDIO")?;
                let permissions = env.new_object_array(1, "java/lang/String", JObject::null())?;
                env.set_object_array_element(&permissions, 0, permission)?;
                let permissions_object = JObject::from(permissions);
                env.call_method(
                    &activity,
                    "requestPermissions",
                    "([Ljava/lang/String;I)V",
                    &[JValue::Object(&permissions_object), JValue::Int(REQUEST_CODE)],
                )?;
                Ok(())
            })();
            if let Err(error) = result { log::error!("Microphone permission request failed: {error}"); }
        }));
    }

    /// Returns false while the Android permission prompt is pending.
    pub fn start(&mut self) -> Result<bool, String> {
        if self.worker.is_some() { return Err("Microphone capture is already active".into()); }
        if !self.has_permission()? {
            self.request_permission();
            return Ok(false);
        }
        self.permission_requested = false;
        self.stop_flag.store(false, Ordering::Release);
        let app = self.app.clone();
        let sender = self.sender.clone();
        let stop_flag = self.stop_flag.clone();
        let recorder_slot = self.recorder.clone();
        self.worker = Some(thread::Builder::new().name("cenai-audio-capture".into()).spawn(move || {
            let result = (|| -> Result<(), String> {
                let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
                let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
                let min_buffer = env.call_static_method(
                    "android/media/AudioRecord",
                    "getMinBufferSize",
                    "(III)I",
                    &[JValue::Int(SAMPLE_RATE), JValue::Int(CHANNEL_IN_MONO), JValue::Int(ENCODING_PCM_16BIT)],
                ).map_err(|error| error.to_string())?.i().map_err(|error| error.to_string())?;
                if min_buffer <= 0 { return Err("Android could not determine a microphone buffer size".into()); }
                let buffer_size = (min_buffer as usize).max(4096).min(64 * 1024) as i32;
                let recorder = env.new_object(
                    "android/media/AudioRecord",
                    "(IIIII)V",
                    &[
                        JValue::Int(AUDIO_SOURCE_MIC),
                        JValue::Int(SAMPLE_RATE),
                        JValue::Int(CHANNEL_IN_MONO),
                        JValue::Int(ENCODING_PCM_16BIT),
                        JValue::Int(buffer_size),
                    ],
                ).map_err(|error| error.to_string())?;
                let state = env.call_method(&recorder, "getState", "()I", &[])
                    .map_err(|error| error.to_string())?.i().map_err(|error| error.to_string())?;
                if state != 1 { return Err("Android AudioRecord failed to initialize".into()); }
                let global = env.new_global_ref(&recorder).map_err(|error| error.to_string())?;
                *recorder_slot.lock().map_err(|_| "Microphone state lock was poisoned")? = Some(global);
                env.call_method(&recorder, "startRecording", "()V", &[]).map_err(|error| error.to_string())?;
                let buffer = env.new_byte_array(buffer_size).map_err(|error| error.to_string())?;
                let buffer_object = JObject::from(buffer);
                while !stop_flag.load(Ordering::Acquire) {
                    let count = match env.call_method(
                        &recorder,
                        "read",
                        "([BII)I",
                        &[JValue::Object(&buffer_object), JValue::Int(0), JValue::Int(buffer_size)],
                    ) {
                        Ok(value) => value.i().map_err(|error| error.to_string())?,
                        Err(error) => {
                            if stop_flag.load(Ordering::Acquire) { break; }
                            return Err(error.to_string());
                        }
                    };
                    if count > 0 {
                        let bytes = env.convert_byte_array(JByteArray::from(buffer_object)).map_err(|error| error.to_string())?;
                        let frame = AudioFrame::from_pcm16_le(SAMPLE_RATE as u32, 1, &bytes[..count as usize])
                            .map_err(str::to_owned)?;
                        if sender.send(Ok(frame)).is_err() { break; }
                    } else if count < 0 {
                        return Err(format!("Android microphone read failed with code {count}"));
                    }
                }
                let _ = env.call_method(&recorder, "stop", "()V", &[]);
                let _ = env.call_method(&recorder, "release", "()V", &[]);
                Ok(())
            })();
            if let Err(error) = result {
                let _ = sender.send(Err(error));
            }
            if let Ok(mut slot) = recorder_slot.lock() { *slot = None; }
        }).map_err(|error| error.to_string())?);
        Ok(true)
    }

    pub fn try_recv(&self) -> Option<Result<AudioFrame, String>> {
        self.receiver.try_recv().ok()
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        let recorder = self.recorder.lock().ok().and_then(|slot| slot.as_ref().cloned());
        if let Some(recorder) = recorder {
            let _ = self.with_env(|env| {
                env.call_method(recorder.as_obj(), "stop", "()V", &[])?;
                Ok(())
            });
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for AndroidAudioCapture {
    fn drop(&mut self) { self.stop(); }
}

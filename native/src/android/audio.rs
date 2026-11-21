//! Android microphone capture using AudioRecord, exposed as bounded PCM16 frames to Rust.
use std::{
    sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
};
use jni::{objects::{GlobalRef, JObject, JValue}, JavaVM};
use winit::platform::android::activity::AndroidApp;
use crate::core::voice::AudioFrame;

const SAMPLE_RATE: i32 = 16_000;
const CHANNEL_IN_MONO: i32 = 16;
const ENCODING_PCM_16BIT: i32 = 2;
const AUDIO_SOURCE_MIC: i32 = 1;
const REQUEST_CODE: i32 = 7_331;

pub struct AndroidAudioCapture {
    app: AndroidApp,
    sender: mpsc::SyncSender<Result<AudioFrame, String>>,
    receiver: mpsc::Receiver<Result<AudioFrame, String>>,
    stop_flag: Arc<AtomicBool>,
    recorder: Arc<Mutex<Option<GlobalRef>>>,
    worker: Option<JoinHandle<()>>,
    permission_requested: bool,
}

impl AndroidAudioCapture {
    pub fn new(app: AndroidApp) -> Self {
        let (sender, receiver) = mpsc::sync_channel(8);
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
                let buffer_object = buffer.as_ref();
                while !stop_flag.load(Ordering::Acquire) {
                    let count = match env.call_method(
                        &recorder,
                        "read",
                        "([BII)I",
                        &[JValue::Object(buffer_object), JValue::Int(0), JValue::Int(buffer_size)],
                    ) {
                        Ok(value) => value.i().map_err(|error| error.to_string())?,
                        Err(error) => {
                            if stop_flag.load(Ordering::Acquire) {
                                let _ = env.exception_clear();
                                break;
                            }
                            return Err(error.to_string());
                        }
                    };
                    if count > 0 {
                        let mut signed_bytes = vec![0i8; count as usize];
                        env.get_byte_array_region(&buffer, 0, &mut signed_bytes).map_err(|error| error.to_string())?;
                        let bytes = signed_bytes.into_iter().map(|byte| byte as u8).collect::<Vec<_>>();
                        let frame = AudioFrame::from_pcm16_le(SAMPLE_RATE as u32, 1, &bytes)
                            .map_err(str::to_owned)?;
                        if sender.try_send(Ok(frame)).is_err() { /* Drop stale frames rather than growing an unbounded audio queue. */ }
                    } else if count < 0 {
                        return Err(format!("Android microphone read failed with code {count}"));
                    }
                }
                let _ = env.call_method(&recorder, "stop", "()V", &[]);
                let _ = env.call_method(&recorder, "release", "()V", &[]);
                Ok(())
            })();
            if let Err(error) = result {
                let _ = sender.try_send(Err(error));
            }
            if let Ok(mut slot) = recorder_slot.lock() { *slot = None; }
        }).map_err(|error| error.to_string())?);
        Ok(true)
    }

    /// Cancels a pending Android permission prompt attempt so the user can retry later.
    pub fn cancel_permission_request(&mut self) {
        if self.worker.is_none() {
            self.permission_requested = false;
        }
    }

    pub fn try_recv(&self) -> Option<Result<AudioFrame, String>> {
        self.receiver.try_recv().ok()
    }

    pub fn stop(&mut self) {
        self.permission_requested = false;
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

fn parse_pcm_wav(bytes: &[u8]) -> Result<(u32, u16, &[u8]), String> {
    if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Speech audio is not a valid WAV file".into());
    }
    let mut offset = 12usize;
    let mut format = None;
    let mut sample_rate = None;
    let mut channels = None;
    let mut bits = None;
    let mut data = None;
    while offset.saturating_add(8) <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().map_err(|_| "Invalid WAV chunk")?) as usize;
        let start = offset + 8;
        let end = start.checked_add(size).ok_or("WAV chunk length overflow")?;
        if end > bytes.len() { return Err("WAV chunk exceeds audio buffer".into()); }
        match id {
            b"fmt " if size >= 16 => {
                format = Some(u16::from_le_bytes([bytes[start], bytes[start + 1]]));
                channels = Some(u16::from_le_bytes([bytes[start + 2], bytes[start + 3]]));
                sample_rate = Some(u32::from_le_bytes(bytes[start + 4..start + 8].try_into().map_err(|_| "Invalid WAV sample rate")?));
                bits = Some(u16::from_le_bytes([bytes[start + 14], bytes[start + 15]]));
            }
            b"data" => data = Some(&bytes[start..end]),
            _ => {}
        }
        offset = end + (size & 1);
    }
    if format != Some(1) || bits != Some(16) || channels != Some(1) {
        return Err("Only mono 16-bit PCM speech playback is supported".into());
    }
    let rate = sample_rate.ok_or("WAV sample rate is missing")?;
    if !matches!(rate, 16_000 | 24_000 | 48_000) {
        return Err("Speech sample rate is unsupported".into());
    }
    let pcm = data.ok_or("WAV audio data is missing")?;
    if pcm.is_empty() || pcm.len() % 2 != 0 {
        return Err("WAV PCM data is empty or misaligned".into());
    }
    Ok((rate, 1, pcm))
}

/// Plays validated PCM WAV data through Android AudioTrack without blocking the render loop.
pub struct AndroidAudioPlayback {
    app: AndroidApp,
    stop_flag: Arc<AtomicBool>,
    track: Arc<Mutex<Option<GlobalRef>>>,
    worker: Option<JoinHandle<()>>,
}

impl AndroidAudioPlayback {
    pub fn new(app: AndroidApp) -> Self {
        Self { app, stop_flag: Arc::new(AtomicBool::new(false)), track: Arc::new(Mutex::new(None)), worker: None }
    }

    pub fn play_wav(&mut self, wav: Vec<u8>) -> Result<(), String> {
        self.stop();
        let (sample_rate, _channels, pcm) = parse_pcm_wav(&wav)?;
        let pcm = pcm.to_vec();
        self.stop_flag.store(false, Ordering::Release);
        let app = self.app.clone();
        let stop_flag = self.stop_flag.clone();
        let track_slot = self.track.clone();
        self.worker = Some(thread::Builder::new().name("cenai-audio-playback".into()).spawn(move || {
            let result = (|| -> Result<(), String> {
                let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
                let mut env = vm.attach_current_thread().map_err(|error| error.to_string())?;
                let min_buffer = env.call_static_method(
                    "android/media/AudioTrack",
                    "getMinBufferSize",
                    "(III)I",
                    &[JValue::Int(sample_rate as i32), JValue::Int(4), JValue::Int(2)],
                ).map_err(|error| error.to_string())?.i().map_err(|error| error.to_string())?;
                if min_buffer <= 0 { return Err("Android could not determine an audio output buffer size".into()); }
                let buffer_size = (min_buffer as usize).max(8192).min(128 * 1024) as i32;
                let track = env.new_object(
                    "android/media/AudioTrack",
                    "(IIIIII)V",
                    &[
                        JValue::Int(3), JValue::Int(sample_rate as i32), JValue::Int(4),
                        JValue::Int(2), JValue::Int(buffer_size), JValue::Int(1),
                    ],
                ).map_err(|error| error.to_string())?;
                let state = env.call_method(&track, "getState", "()I", &[])
                    .map_err(|error| error.to_string())?.i().map_err(|error| error.to_string())?;
                if state != 1 { return Err("Android AudioTrack failed to initialize".into()); }
                let global = env.new_global_ref(&track).map_err(|error| error.to_string())?;
                *track_slot.lock().map_err(|_| "Audio playback state lock was poisoned")? = Some(global);
                env.call_method(&track, "play", "()V", &[]).map_err(|error| error.to_string())?;
                for chunk in pcm.chunks(16 * 1024) {
                    if stop_flag.load(Ordering::Acquire) { break; }
                    let bytes = env.byte_array_from_slice(chunk).map_err(|error| error.to_string())?;
                    let bytes_object = JObject::from(bytes);
                    let written = env.call_method(
                        &track, "write", "([BII)I",
                        &[JValue::Object(&bytes_object), JValue::Int(0), JValue::Int(chunk.len() as i32)],
                    ).map_err(|error| error.to_string())?.i().map_err(|error| error.to_string())?;
                    if written < 0 { return Err(format!("Android audio output failed with code {written}")); }
                }
                let _ = env.call_method(&track, "stop", "()V", &[]);
                let _ = env.call_method(&track, "release", "()V", &[]);
                Ok(())
            })();
            if let Err(error) = result { log::warn!("CenAI speech playback failed: {error}"); }
            if let Ok(mut slot) = track_slot.lock() { *slot = None; }
        }).map_err(|error| error.to_string())?);
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        self.worker.as_ref().is_some_and(|worker| !worker.is_finished())
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        let track = self.track.lock().ok().and_then(|slot| slot.as_ref().cloned());
        if let Some(track) = track {
            let vm = unsafe { JavaVM::from_raw(self.app.vm_as_ptr().cast()) };
            if let Ok(vm) = vm {
                if let Ok(mut env) = vm.attach_current_thread() {
                    let _ = env.call_method(track.as_obj(), "stop", "()V", &[]);
                }
            }
        }
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

impl Drop for AndroidAudioPlayback {
    fn drop(&mut self) { self.stop(); }
}

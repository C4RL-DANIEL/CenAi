use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;

#[no_mangle]
pub extern "system" fn Java_com_cenai_app_NativeCore_normalizePrompt(
    mut env: JNIEnv, _class: JClass, input: JString
) -> jstring {
    let value: String = match env.get_string(&input) {
        Ok(v) => v.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    match env.new_string(value.trim()) {
        Ok(v) => v.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

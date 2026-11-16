//! The phone's half of the protocol, as a native library for the Android app.
//!
//! Android only hands SMS, notifications, and calls to JVM code, so Kotlin
//! gathers the data. Everything that goes on the wire passes through here:
//! the same framing and the same message definitions the desktop uses, so the
//! two ends cannot drift apart. A message Kotlin builds wrongly is refused
//! here, on the phone, instead of confusing the desktop.

pub mod net;

use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::sys::{jbyteArray, jint, jlong, jobjectArray, jstring};
use jni::JNIEnv;
use omalink_proto::{frame, number_key, Codec, DecodeError, Message, PROTOCOL_VERSION, SERVICE_UUID};

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_serviceUuid<'l>(env: JNIEnv<'l>, _class: JClass<'l>) -> jstring {
    env.new_string(SERVICE_UUID).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_protocolVersion<'l>(_env: JNIEnv<'l>, _class: JClass<'l>) -> jint {
    PROTOCOL_VERSION as jint
}

/// A decoder for one connection. Freed with `freeCodec`.
#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_newCodec<'l>(_env: JNIEnv<'l>, _class: JClass<'l>) -> jlong {
    Box::into_raw(Box::new(Codec::new())) as jlong
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_freeCodec<'l>(_env: JNIEnv<'l>, _class: JClass<'l>, handle: jlong) {
    if handle != 0 {
        // Safety: the handle came from newCodec and Kotlin frees it once.
        drop(unsafe { Box::from_raw(handle as *mut Codec) });
    }
}

/// Takes bytes as they arrive and returns the whole messages they complete,
/// as JSON. Returns null when the stream is corrupt and must be closed.
#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_feed<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    bytes: JByteArray<'l>,
    length: jint,
) -> jobjectArray {
    if handle == 0 {
        return std::ptr::null_mut();
    }
    // Safety: the handle came from newCodec, and one thread reads a connection.
    let codec = unsafe { &mut *(handle as *mut Codec) };
    let Ok(data) = env.convert_byte_array(&bytes) else { return std::ptr::null_mut() };
    codec.feed(&data[..(length.max(0) as usize).min(data.len())]);

    let mut messages = Vec::new();
    loop {
        match codec.next() {
            Ok(Some(message)) => messages.push(serde_json::to_string(&message).unwrap_or_default()),
            Ok(None) => break,
            // A newer desktop may say things this build doesn't know.
            Err(DecodeError::Unknown(_)) => continue,
            Err(DecodeError::TooLarge(_)) => return std::ptr::null_mut(),
        }
    }

    let Ok(array) = env.new_object_array(messages.len() as jint, "java/lang/String", JObject::null()) else {
        return std::ptr::null_mut();
    };
    for (i, message) in messages.iter().enumerate() {
        let Ok(string) = env.new_string(message) else { return std::ptr::null_mut() };
        if env.set_object_array_element(&array, i as jint, &string).is_err() {
            return std::ptr::null_mut();
        }
        // A big sync can produce more strings than the local reference table holds.
        let _ = env.delete_local_ref(string);
    }
    array.into_raw()
}

/// Checks that the JSON is a message the desktop will understand and frames
/// it for the wire. Returns null when it is not.
#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_frame<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, json: JString<'l>) -> jbyteArray {
    let Ok(text) = env.get_string(&json) else { return std::ptr::null_mut() };
    let text: String = text.into();
    let Ok(message) = serde_json::from_str::<Message>(&text) else { return std::ptr::null_mut() };
    let body = serde_json::to_vec(&message).unwrap_or_default();
    env.byte_array_from_slice(&frame(&body)).map(|a| a.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Why a message was refused by `frame`, for the log.
#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_explain<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, json: JString<'l>) -> jstring {
    let text: String = env.get_string(&json).map(Into::into).unwrap_or_default();
    let reason = match serde_json::from_str::<Message>(&text) {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    };
    env.new_string(reason).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_numberKey<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, number: JString<'l>) -> jstring {
    let text: String = env.get_string(&number).map(Into::into).unwrap_or_default();
    env.new_string(number_key(&text)).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

// The network link. One `Phone` per process, started by the link service.

static PHONE: std::sync::OnceLock<net::Phone> = std::sync::OnceLock::new();

fn phone() -> Option<&'static net::Phone> {
    PHONE.get()
}

fn string(env: &mut JNIEnv, value: &JString) -> String {
    env.get_string(value).map(Into::into).unwrap_or_default()
}

fn jstring_or_null(env: &mut JNIEnv, value: Option<String>) -> jstring {
    match value {
        Some(text) => env.new_string(text).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// Starts networking with its key and trusted computers in `dir`. Returns
/// this phone's address, or null if networking couldn't start.
#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netStart<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    context: JObject<'l>,
    dir: JString<'l>,
    name: JString<'l>,
) -> jstring {
    if let Some(phone) = phone() {
        return jstring_or_null(&mut env, Some(phone.id()));
    }
    // Lets iroh read the phone's DNS servers; without it, public ones are used.
    #[cfg(target_os = "android")]
    if let (Ok(vm), Ok(global)) = (env.get_java_vm(), env.new_global_ref(&context)) {
        let context = global.as_obj().as_raw() as *mut std::ffi::c_void;
        // The process keeps both for as long as it lives.
        std::mem::forget(global);
        unsafe { omalink_net::iroh::dns::install_android_jni_context(vm.get_java_vm_pointer() as *mut std::ffi::c_void, context) };
    }
    #[cfg(not(target_os = "android"))]
    let _ = context;
    let (dir, name) = (string(&mut env, &dir), string(&mut env, &name));
    let id = match net::Phone::start(std::path::Path::new(&dir), &name) {
        Ok(phone) => {
            let id = phone.id();
            let _ = PHONE.set(phone);
            Some(id)
        }
        Err(_) => None,
    };
    jstring_or_null(&mut env, id)
}

/// The next event as JSON, or null after `timeout_ms`.
#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netNext<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, timeout_ms: jint) -> jstring {
    let event = phone().and_then(|p| p.next_event(std::time::Duration::from_millis(timeout_ms.max(0) as u64)));
    jstring_or_null(&mut env, event)
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netConnected<'l>(_env: JNIEnv<'l>, _class: JClass<'l>) -> jni::sys::jboolean {
    phone().is_some_and(|p| p.connected()) as jni::sys::jboolean
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netSend<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, json: JString<'l>) -> jni::sys::jboolean {
    let json = string(&mut env, &json);
    phone().is_some_and(|p| p.send(&json)) as jni::sys::jboolean
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netPair<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, id: JString<'l>, name: JString<'l>) {
    let (id, name) = (string(&mut env, &id), string(&mut env, &name));
    if let Some(p) = phone() {
        p.pair(&id, &name);
    }
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netChoose<'l>(_env: JNIEnv<'l>, _class: JClass<'l>, index: jint) {
    if let Some(p) = phone() {
        p.choose(index.max(0) as usize);
    }
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netDecline<'l>(_env: JNIEnv<'l>, _class: JClass<'l>) {
    if let Some(p) = phone() {
        p.decline();
    }
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netPeers<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>) -> jstring {
    let peers = phone().map(|p| p.peers()).unwrap_or_else(|| "[]".into());
    jstring_or_null(&mut env, Some(peers))
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netForget<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, id: JString<'l>) {
    let id = string(&mut env, &id);
    if let Some(p) = phone() {
        p.forget(&id);
    }
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netNetworkChanged<'l>(_env: JNIEnv<'l>, _class: JClass<'l>) {
    if let Some(p) = phone() {
        p.network_changed();
    }
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netInvited<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, id: JString<'l>, name: JString<'l>) {
    let (id, name) = (string(&mut env, &id), string(&mut env, &name));
    if let Some(p) = phone() {
        p.invited(&id, &name);
    }
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netOpenFile<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, id: jlong, name: JString<'l>, size: jlong) -> jlong {
    let name = string(&mut env, &name);
    phone().map(|p| p.open_file(id as u64, &name, size)).unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netOpenMirror<'l>(_env: JNIEnv<'l>, _class: JClass<'l>, width: jint, height: jint) -> jlong {
    phone().map(|p| p.open_mirror(width.max(0) as u32, height.max(0) as u32)).unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netWrite<'l>(env: JNIEnv<'l>, _class: JClass<'l>, handle: jlong, bytes: JByteArray<'l>, length: jint) -> jni::sys::jboolean {
    let Ok(data) = env.convert_byte_array(&bytes) else { return 0 };
    let length = (length.max(0) as usize).min(data.len());
    phone().is_some_and(|p| p.write(handle, &data[..length])) as jni::sys::jboolean
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netWriteFrame<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    bytes: JByteArray<'l>,
    length: jint,
    pts_us: jlong,
    flags: jint,
) -> jni::sys::jboolean {
    let Ok(data) = env.convert_byte_array(&bytes) else { return 0 };
    let length = (length.max(0) as usize).min(data.len());
    phone().is_some_and(|p| p.write_frame(handle, &data[..length], pts_us.max(0) as u64, flags as u8)) as jni::sys::jboolean
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netFinish<'l>(_env: JNIEnv<'l>, _class: JClass<'l>, handle: jlong) -> jni::sys::jboolean {
    phone().is_some_and(|p| p.finish(handle)) as jni::sys::jboolean
}

/// Reads into `buffer`: the byte count, 0 at the end, -1 on error.
#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netRead<'l>(env: JNIEnv<'l>, _class: JClass<'l>, handle: jlong, buffer: JByteArray<'l>) -> jint {
    let Some(p) = phone() else { return -1 };
    let Ok(capacity) = env.get_array_length(&buffer) else { return -1 };
    let mut chunk = vec![0u8; capacity.max(0) as usize];
    let n = p.read(handle, &mut chunk);
    if n > 0 {
        // Safety: i8 and u8 have the same layout.
        let signed = unsafe { std::slice::from_raw_parts(chunk.as_ptr() as *const i8, n as usize) };
        if env.set_byte_array_region(&buffer, 0, signed).is_err() {
            return -1;
        }
    }
    n
}

#[no_mangle]
pub extern "system" fn Java_io_omalink_Core_netClose<'l>(_env: JNIEnv<'l>, _class: JClass<'l>, handle: jlong) {
    if let Some(p) = phone() {
        p.close(handle);
    }
}

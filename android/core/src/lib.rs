//! The phone's half of the protocol, as a native library for the Android app.
//!
//! Android only hands SMS, notifications, and calls to JVM code, so Kotlin
//! gathers the data. Everything that goes on the wire passes through here:
//! the same framing and the same message definitions the desktop uses, so the
//! two ends cannot drift apart. A message Kotlin builds wrongly is refused
//! here, on the phone, instead of confusing the desktop.

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

//! The JNI entry points of `expo.modules.spotatuicore.SpotatuiNative`.

use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::{jint, jlong, jstring};
use jni::{JNIEnv, JavaVM};
use spotatui::{MediaCommand, MediaState};
use std::sync::{Once, OnceLock};

static CONTEXT: Once = Once::new();
/// The JavaVM and `SpotatuiMedia`: a Rust thread cannot find an app class itself.
static MEDIA: OnceLock<(JavaVM, GlobalRef)> = OnceLock::new();

/// The media button codes of `SpotatuiNative.command`.
const PLAY: jint = 0;
const PAUSE: jint = 1;
const NEXT: jint = 2;
const PREVIOUS: jint = 3;
const SEEK: jint = 4;

/// Gives cpal the JavaVM and the Application context; only the first call does anything.
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_spotatuicore_SpotatuiNative_setContext(
  mut env: JNIEnv,
  _class: JClass,
  context: JObject,
) {
  let mut result = Ok(());
  CONTEXT.call_once(|| result = set_context(&mut env, &context));
  if let Err(e) = result {
    throw(&mut env, &format!("could not set the Android context: {e}"));
  }
}

fn set_context(env: &mut JNIEnv, context: &JObject) -> jni::errors::Result<()> {
  let app = env
    .call_method(
      context,
      "getApplicationContext",
      "()Landroid/content/Context;",
      &[],
    )?
    .l()?;
  // A global ref: cpal reads the context later from the player thread.
  let app = env.new_global_ref(app)?;
  let vm = env.get_java_vm()?;
  // SAFETY: a live JavaVM and a global ref that is never released, set once under `CONTEXT`.
  unsafe {
    ndk_context::initialize_android_context(
      vm.get_java_vm_pointer().cast(),
      app.as_obj().as_raw().cast(),
    );
  }
  std::mem::forget(app);
  let media = env.find_class("expo/modules/spotatuicore/SpotatuiMedia")?;
  let media = env.new_global_ref(media)?;
  let _ = MEDIA.set((env.get_java_vm()?, media));
  Ok(())
}

/// Shows the core's media state in `SpotatuiMedia`, or clears it.
pub(crate) fn show_media(state: Option<&MediaState>) {
  let Some((vm, media)) = MEDIA.get() else {
    return;
  };
  let Ok(mut env) = vm.attach_current_thread_permanently() else {
    return log::warn!("media controls: could not attach the media thread");
  };
  let class: &JClass = media.as_obj().into();
  // This thread never returns to Java, so its local refs are freed by a frame of their own.
  let result = env.with_local_frame(8, |env| -> jni::errors::Result<()> {
    match state {
      None => env.call_static_method(class, "clear", "()V", &[])?,
      Some(state) => {
        let title = env.new_string(&state.title)?;
        let artists = env.new_string(state.artists.join(", "))?;
        let album = env.new_string(&state.album)?;
        let artwork = match &state.artwork_url {
          Some(url) => JObject::from(env.new_string(url)?),
          None => JObject::null(),
        };
        env.call_static_method(
          class,
          "update",
          "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;JJZ)V",
          &[
            JValue::Object(&title),
            JValue::Object(&artists),
            JValue::Object(&album),
            JValue::Object(&artwork),
            JValue::Long(state.duration_ms as jlong),
            JValue::Long(state.position_ms as jlong),
            JValue::Bool(state.playing.into()),
          ],
        )?
      }
    };
    Ok(())
  });
  if let Err(e) = result {
    if env.exception_check().unwrap_or(false) {
      let _ = env.exception_describe();
      let _ = env.exception_clear();
    }
    log::warn!("media controls: {e}");
  }
}

/// A press on the system media controls, from `SpotatuiNative.command`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_spotatuicore_SpotatuiNative_command(
  _env: JNIEnv,
  _class: JClass,
  code: jint,
  position_ms: jlong,
) {
  let command = match code {
    PLAY => MediaCommand::Play,
    PAUSE => MediaCommand::Pause,
    NEXT => MediaCommand::Next,
    PREVIOUS => MediaCommand::Previous,
    SEEK => MediaCommand::SeekTo(position_ms.max(0) as u64),
    _ => return,
  };
  crate::send_media_command(command);
}

/// Starts the core and returns its launch URL; throws when the core cannot start.
#[unsafe(no_mangle)]
pub extern "system" fn Java_expo_modules_spotatuicore_SpotatuiNative_start(
  mut env: JNIEnv,
  _class: JClass,
  files: JString,
  cache: JString,
) -> jstring {
  let dirs = env.get_string(&files).and_then(|files| {
    Ok(crate::Dirs {
      files: files.into(),
      cache: env.get_string(&cache)?.into(),
    })
  });
  let url = dirs
    .map_err(anyhow::Error::from)
    .and_then(|dirs| crate::start(&dirs))
    .and_then(|url| Ok(env.new_string(url)?));
  match url {
    Ok(url) => url.into_raw(),
    Err(e) => {
      throw(&mut env, &format!("{e:#}"));
      std::ptr::null_mut()
    }
  }
}

fn throw(env: &mut JNIEnv, message: &str) {
  let _ = env.throw_new("java/lang/IllegalStateException", message);
}

//! The JNI entry points of `expo.modules.spotatuicore.SpotatuiNative`.

use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString};
use jni::sys::jstring;
use std::sync::Once;

static CONTEXT: Once = Once::new();

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
  Ok(())
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

package expo.modules.spotatuicore

import android.content.Context

/** The JNI surface of libspotatui_mobile.so; the symbols are in mobile/core/src/android.rs. */
object SpotatuiNative {
  init {
    System.loadLibrary("spotatui_mobile")
  }

  @JvmStatic external fun setContext(context: Context)

  @JvmStatic external fun start(files: String, cache: String): String
}

package expo.modules.spotatuicore

import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.functions.Coroutine
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** The page storage, kept as long as the core: a JavaScript reload must not lose the session token. */
private object SessionStorage {
  val items = ConcurrentHashMap<String, String>()
}

class SpotatuiCoreModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("SpotatuiCore")

    // The start blocks until the core serves, so it leaves the queue all modules share.
    AsyncFunction("start") Coroutine { ->
      val context = appContext.reactContext ?: throw Exceptions.ReactContextLost()
      withContext(Dispatchers.IO) {
        SpotatuiNative.setContext(context)
        SpotatuiNative.start(context.filesDir.absolutePath, context.cacheDir.absolutePath)
      }
    }

    Function("getItem") { key: String -> SessionStorage.items[key] }

    Function("setItem") { key: String, value: String -> SessionStorage.items[key] = value }
  }
}

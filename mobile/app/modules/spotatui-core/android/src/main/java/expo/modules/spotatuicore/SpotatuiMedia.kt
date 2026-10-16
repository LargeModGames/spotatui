package expo.modules.spotatuicore

import android.content.ComponentName
import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture

/** The core's media state, called over JNI from its media thread (mobile/core/src/android.rs). */
object SpotatuiMedia {
  private val main = Handler(Looper.getMainLooper())

  /** Set before the core starts. */
  @Volatile var context: Context? = null

  // A connected controller creates the service, which owns the session.
  private var controller: ListenableFuture<MediaController>? = null

  @JvmStatic
  fun update(
    title: String,
    artists: String,
    album: String,
    artworkUrl: String?,
    durationMs: Long,
    positionMs: Long,
    playing: Boolean,
  ) {
    val track = CoreTrack(title, artists, album, artworkUrl, durationMs, positionMs, playing)
    main.post {
      connect()
      CorePlayer.show(track)
    }
  }

  @JvmStatic
  fun clear() {
    main.post { CorePlayer.show(null) }
  }

  private fun connect() {
    val context = context ?: return
    if (controller != null) return
    val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
    val future = MediaController.Builder(context, token).buildAsync()
    controller = future
    // A failed connection is forgotten, so the next update tries again.
    future.addListener({
      if (runCatching { future.get() }.isFailure && controller === future) controller = null
    }, main::post)
  }
}

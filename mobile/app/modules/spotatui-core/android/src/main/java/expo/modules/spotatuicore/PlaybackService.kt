package expo.modules.spotatuicore

import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService

/** The media session's service: the notification, the lock screen and headset buttons. */
class PlaybackService : MediaSessionService() {
  private var session: MediaSession? = null

  override fun onCreate() {
    super.onCreate()
    session = MediaSession.Builder(this, CorePlayer).build()
  }

  override fun onGetSession(controllerInfo: MediaSession.ControllerInfo) = session

  override fun onDestroy() {
    session?.release()
    session = null
    super.onDestroy()
  }
}

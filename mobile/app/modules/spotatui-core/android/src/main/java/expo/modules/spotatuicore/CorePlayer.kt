package expo.modules.spotatuicore

import android.net.Uri
import android.os.Looper
import androidx.annotation.OptIn
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.UnstableApi
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture

/** What the core reports it plays in this process. */
data class CoreTrack(
  val title: String,
  val artists: String,
  val album: String,
  val artworkUrl: String?,
  val durationMs: Long,
  val positionMs: Long,
  val playing: Boolean,
)

/**
 * The core's playback as a media3 player: the session shows it, and every press goes to the
 * core, which owns the queue. Main thread only.
 */
@OptIn(UnstableApi::class)
object CorePlayer : SimpleBasePlayer(Looper.getMainLooper()) {
  private val commands =
    Player.Commands.Builder()
      .addAll(
        Player.COMMAND_PLAY_PAUSE,
        Player.COMMAND_SEEK_TO_NEXT,
        Player.COMMAND_SEEK_TO_PREVIOUS,
        Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM,
        Player.COMMAND_GET_CURRENT_MEDIA_ITEM,
        Player.COMMAND_GET_METADATA,
        Player.COMMAND_GET_TIMELINE,
      )
      .build()

  private var track: CoreTrack? = null

  fun show(track: CoreTrack?) {
    this.track = track
    invalidateState()
  }

  override fun getState(): State {
    val builder = State.Builder().setAvailableCommands(commands)
    val track = track ?: return builder.setPlaybackState(Player.STATE_IDLE).build()
    val metadata =
      MediaMetadata.Builder()
        .setTitle(track.title)
        .setArtist(track.artists)
        .setAlbumTitle(track.album)
        .setArtworkUri(track.artworkUrl?.let(Uri::parse))
        .build()
    // A new uid per track tells the session that the item changed.
    val uid = "${track.title}\u0000${track.artists}\u0000${track.album}"
    val item =
      MediaItemData.Builder(uid)
        .setMediaItem(MediaItem.Builder().setMediaId(uid).setMediaMetadata(metadata).build())
        .setDurationUs(track.durationMs * 1000)
        .build()
    return builder
      .setPlaylist(listOf(item))
      .setPlaybackState(Player.STATE_READY)
      .setPlayWhenReady(track.playing, Player.PLAY_WHEN_READY_CHANGE_REASON_REMOTE)
      .setContentPositionMs(
        PositionSupplier.getExtrapolating(track.positionMs, if (track.playing) 1f else 0f)
      )
      .build()
  }

  override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
    // Shown at once; the core's next report confirms it.
    track = track?.copy(playing = playWhenReady)
    SpotatuiNative.command(if (playWhenReady) PLAY else PAUSE, 0)
    return Futures.immediateVoidFuture()
  }

  override fun handleSeek(mediaItemIndex: Int, positionMs: Long, seekCommand: Int): ListenableFuture<*> {
    // With one item, next and previous still arrive here as their commands.
    when (seekCommand) {
      Player.COMMAND_SEEK_TO_NEXT, Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM ->
        SpotatuiNative.command(NEXT, 0)
      Player.COMMAND_SEEK_TO_PREVIOUS, Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM ->
        SpotatuiNative.command(PREVIOUS, 0)
      Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM -> SpotatuiNative.command(SEEK, positionMs)
    }
    return Futures.immediateVoidFuture()
  }

  // The codes of SpotatuiNative.command, matched in mobile/core/src/android.rs.
  private const val PLAY = 0
  private const val PAUSE = 1
  private const val NEXT = 2
  private const val PREVIOUS = 3
  private const val SEEK = 4
}

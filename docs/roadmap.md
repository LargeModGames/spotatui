# Roadmap

The goal is to eventually implement almost every Spotify feature.

## High Priority

Both items previously listed here (adding songs to a playlist, and scrolling
through result pages) are now implemented.

## Spotify API Coverage

This table shows what is possible with the Spotify API, what is implemented, and whether it's essential.

| API Method | Implemented | Description | Essential |
| --- | :---: | --- | :---: |
| **Tracks** |
| track | ✅ | Get a single track by ID | No |
| tracks | ✅ | Get multiple tracks by IDs | No |
| **Artists** |
| artist | ❌ | Get a single artist by ID | Yes |
| artists | ❌ | Get multiple artists by IDs | No |
| artist_albums | ✅ | Get an artist's albums | Yes |
| artist_top_tracks | ✅ | Get an artist's top 10 tracks | Yes |
| artist_related_artists | ✅ | Get similar artists | Yes |
| **Albums** |
| album | ✅ | Get a single album by ID | Yes |
| albums | ❌ | Get multiple albums by IDs | No |
| album_track | ✅ | Get an album's tracks | Yes |
| **Search** |
| search_album | ✅ | Search albums | Yes |
| search_artist | ✅ | Search artists | Yes |
| search_track | ✅ | Search tracks | Yes |
| search_playlist | ✅ | Search playlists | Yes |
| **Playlists** |
| playlist | ✅ | Get playlist details | Yes |
| current_user_playlists | ✅ | Get user's playlists | Yes |
| user_playlists | ❌ | Get another user's playlists | No |
| user_playlist_tracks | ✅ | Get playlist tracks | Yes |
| user_playlist_create | ✅ | Create a playlist | Yes |
| user_playlist_change_detail | ❌ | Change playlist name/visibility | Yes |
| user_playlist_unfollow | ✅ | Unfollow (delete) playlist | Yes |
| user_playlist_add_track | ✅ | Add tracks to playlist | Yes |
| user_playlist_follow_playlist | ✅ | Follow a playlist | Yes |
| **Library** |
| current_user_saved_albums | ✅ | Get saved albums | Yes |
| current_user_saved_tracks | ✅ | Get liked songs | Yes |
| current_user_followed_artists | ✅ | Get followed artists | Yes |
| current_user_saved_tracks_add | ✅ | Like a track | Yes |
| current_user_saved_tracks_delete | ✅ | Unlike a track | Yes |
| current_user_saved_albums_add | ✅ | Save an album | Yes |
| current_user_saved_albums_delete | ✅ | Remove saved album | Yes |
| user_follow_artists | ✅ | Follow artists | Yes |
| user_unfollow_artists | ✅ | Unfollow artists | Yes |
| current_user_recently_played | ✅ | Get recently played | Yes |
| current_user_top_artists | ✅ | Get top artists | Yes |
| current_user_top_tracks | ✅ | Get top tracks | Yes |
| **Playback** |
| device | ✅ | Get available devices | Yes |
| current_playback | ✅ | Get current playback state | Yes |
| transfer_playback | ✅ | Transfer to another device | Yes |
| start_playback | ✅ | Start/resume playback | Yes |
| pause_playback | ✅ | Pause playback | Yes |
| next_track | ✅ | Skip to next | Yes |
| previous_track | ✅ | Skip to previous | Yes |
| seek_track | ✅ | Seek position | Yes |
| repeat | ✅ | Set repeat mode | Yes |
| volume | ✅ | Set volume | Yes |
| shuffle | ✅ | Toggle shuffle | Yes |
| **Other** |
| recommendations | ✅ | Get recommendations | Yes |
| audio_analysis | ❌ | Get audio analysis (visualization now uses local FFT, not this endpoint) | Yes |
| featured_playlists | ❌ | Get featured playlists | Yes |
| new_releases | ❌ | Get new releases | Yes |
| categories | ❌ | Get categories | Yes |

## Want to Help?

Pick an unimplemented feature and [contribute](https://github.com/LargeModGames/spotatui/blob/main/CONTRIBUTING.md)!

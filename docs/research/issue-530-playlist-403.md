# Issue #530: Spotify playlist-content 403s and librespot spclient

**Research date:** 2026-09-24  
**Scope:** repository state at commit [`91fd6b3`](https://github.com/nohint404/spotatui/tree/91fd6b36f5b0f6081cf0aebd8e1471f71fcee532); no credentials or live account were used.

## Findings

### Development Mode and the Web API

Spotify announced the current Development Mode tightening on **February 6, 2026**. New Development Mode client IDs receive the reduced endpoint set from **February 11, 2026**, including Premium-owner, one-client-ID, and five-authorized-user limits. The migration guide originally said existing integrations would migrate on **March 9, 2026**. However, Spotify's announcement now contains a March 9 update explicitly saying that **endpoint-access changes for existing integrations were postponed**, with no replacement date; the Premium, user-cap, and one-client-ID changes remained planned ([announcement](https://developer.spotify.com/blog/2026-02-06-update-on-developer-access-and-platform-security), [migration timeline](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide#timeline)). Extended Quota Mode apps are documented as unaffected by the migration ([migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide#who-is-affected)).

For playlist reads, Spotify's February changelog/migration guide replaces:

- `GET /v1/playlists/{id}/tracks` → `GET /v1/playlists/{id}/items`
- response `tracks` → `items`, and item `track` → `item`

The old `/tracks` operation is listed as **removed**, not merely renamed ([changelog](https://developer.spotify.com/documentation/web-api/references/changes/february-2026), [migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide#playlist-endpoint-renames)). The current `GET /playlists/{playlist_id}/items` reference explicitly says it returns **403 Forbidden** when the caller is neither the playlist owner nor a collaborator ([reference](https://developer.spotify.com/documentation/web-api/reference/get-playlists-items)).

The important access distinction is therefore:

- **Owned playlist:** contents are eligible for the current user.
- **Playlist where the user is a collaborator:** contents are also eligible.
- **External playlist:** merely public, followed, or discoverable is not enough; Spotify says only metadata is returned for non-owned/non-collaborated playlists and the `items` field is absent. The reference separately specifies 403 for the items endpoint in that case ([migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide#playlist), [playlist concepts](https://developer.spotify.com/documentation/web-api/concepts/playlists)).

Thus, 403 from the legacy `/tracks` path is expected to be treated as a removed-endpoint failure. A 403 from `/items` for an external playlist is also documented behavior. A 403 from `/items` for an owned/collaborative playlist is not explained by the published contract and needs account/client-mode reproduction; the official pages do not promise that an internal client can bypass it.

### Exact librespot dependency in this repository

`Cargo.toml` renames the maintained fork's crates back to `librespot_*` imports and pins them with `=` at **0.8.3**; `Cargo.lock` confirms all seven fork crates (`core`, `connect`, `oauth`, `metadata`, `protocol`, `playback`, `audio`) at **0.8.3** with registry checksums ([Cargo.toml](https://github.com/nohint404/spotatui/blob/91fd6b36f5b0f6081cf0aebd8e1471f71fcee532/Cargo.toml#L94-L117), [Cargo.lock](https://github.com/nohint404/spotatui/blob/91fd6b36f5b0f6081cf0aebd8e1471f71fcee532/Cargo.lock#L5844-L6015)). The exact `spotatui` fork publishing commit is [`3145b11`](https://github.com/LargeModGames/spotatui-librespot/commit/3145b1139c0574111714b10f8c458378376a219c); crates.io identifies `spotatui-librespot-core` 0.8.3 as this repository's fork ([crate metadata](https://crates.io/api/v1/crates/spotatui-librespot-core/0.8.3)).

The fork's own note says it is upstream librespot v0.8.0 plus spotatui patches; its 0.8.3-specific patch concerns direct `Player::load` request IDs, not playlist access ([`SPOTATUI_FORK.md`](https://github.com/LargeModGames/spotatui-librespot/blob/3145b1139c0574111714b10f8c458378376a219c/SPOTATUI_FORK.md)). The upstream CDN fallback that the fork carries is [librespot PR #1722](https://github.com/librespot-org/librespot/pull/1722), unrelated to playlist retrieval.

### Does this fork expose playlist contents through `Session`?

**Yes, as a public internal spclient API.** In the exact fork source:

1. `Session::spclient()` is public and returns `&SpClient` ([`session.rs`](https://github.com/LargeModGames/spotatui-librespot/blob/3145b1139c0574111714b10f8c458378376a219c/core/src/session.rs#L324-L326)).
2. `SpClient::get_playlist(&SpotifyId)` is public and sends `GET /playlist/v2/playlist/{base62-id}` through the authenticated spclient client ([`spclient.rs`](https://github.com/LargeModGames/spotatui-librespot/blob/3145b1139c0574111714b10f8c458378376a219c/core/src/spclient.rs#L669-L672)).
3. The fork's public metadata implementation calls exactly that method for a playlist and parses the protobuf `SelectedListContent`, including its `contents` field ([`metadata/src/playlist/list.rs`](https://github.com/LargeModGames/spotatui-librespot/blob/3145b1139c0574111714b10f8c458378376a219c/metadata/src/playlist/list.rs#L88-L124)).

So an implementation can retrieve the fork's parsed playlist metadata/content by going through `Session::spclient().get_playlist(...)`, or use the existing metadata `Playlist` request path. This is a **supported API of this fork's Rust surface**, but it is not a Spotify Web API endpoint and is not an officially documented/public Spotify developer API. It should not be described as a guaranteed Development Mode workaround.

## Unresolved uncertainty

- Spotify's public Development Mode documentation describes Web API endpoint/response restrictions, not the internal `playlist/v2/playlist` spclient service. It does not establish whether that service is allowed, rate-stable, or subject to the same owner/collaborator rule.
- The fork source proves the request and parser exist; it does **not** prove that a current Development Mode session will receive contents for an external playlist, or even that every owned playlist will succeed after Spotify's 2026 policy changes.
- A safe next diagnostic is a credential-free code-path test plus a real, authorized test account: compare the Web API `/items` result and the spclient result for one owned/collaborative playlist and one external playlist, recording only status/shape (never tokens or raw credentials). Until that is done, the likely conclusion is: Web API 403s require the `/items` migration and correct playlist relationship; librespot offers a technically available but undocumented internal fallback whose policy and current server behavior remain unverified.

## Sources

All claims above are linked to the original Spotify, repository, crates.io, or upstream librespot pages at the point of citation. No secrets, tokens, or private account data are included.

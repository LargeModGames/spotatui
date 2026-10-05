// Constant JXA program, executed by osascript. All inputs are argv values.
// Never activate, reveal, select, or construct/evaluate caller-supplied code.
function run(argv) {
    const music = Application('com.apple.Music');
    const op = argv[0];
    const stopped = {running: false, playing: false, track: null, position: 0, volume: 0};
    if (!music.running()) {
        return JSON.stringify(op === 'snapshot' || op === 'pause' ? stopped : {not_running: true});
    }
    // Music cannot play a track its cloud status rules out, nor a file track
    // whose file is gone: it answers with a dialog that blocks every later
    // start until someone dismisses it. Such rows are shown but not played.
    // For a file track the file decides: one removed from the cloud library
    // still has its local copy.
    const UNPLAYABLE = ['no longer available', 'error', 'removed'];
    // The ids of a list's unplayable tracks, in a handful of bulk reads.
    function unplayableIds(list) {
        const bad = {};
        try {
            const ids = list.tracks.persistentID(), cloud = list.tracks.cloudStatus();
            for (let i = 0; i < ids.length; ++i) if (UNPLAYABLE.indexOf(cloud[i]) >= 0) bad[ids[i]] = true;
            const fileIds = list.fileTracks.persistentID(), locations = list.fileTracks.location();
            for (let i = 0; i < fileIds.length; ++i) {
                if (locations[i]) delete bad[fileIds[i]];
                else bad[fileIds[i]] = true;
            }
        } catch (e) {}
        return bad;
    }
    function trackInfo(track) {
        return {id: track.persistentID(), name: track.name(), artist: track.artist(),
                album: track.album(), duration: track.duration()};
    }
    // The per-track reader for lists without bulk reads (search results).
    function trackInfoIn(list) {
        const bad = unplayableIds(list);
        return function(track) {
            const row = trackInfo(track);
            row.playable = !bad[row.id];
            return row;
        };
    }
    // At volume 0 Music answers a start with a "volume is at its lowest"
    // dialog and plays nothing until it is dismissed, so the floor is the
    // lowest level that reads back above 0. Music on macOS 27.2 stores 1 as
    // 0, so a level is read back and raised (at most to 5) until it is not.
    // Music drops a second change made within about 0.1s of the first, so
    // each retry waits 0.2s.
    function setVolume(level) {
        let v = Math.max(1, level);
        music.soundVolume = v;
        while (music.soundVolume() === 0 && v < 5) {
            delay(0.2);
            music.soundVolume = ++v;
        }
    }
    function ensureAudible() {
        if (music.soundVolume() === 0) setVolume(1);
    }
    // Whether Music really started: up to 3s for the state to read playing.
    let started = null;
    function awaitPlaying() {
        for (let i = 0; i < 15; ++i) {
            if (music.playerState() === 'playing') return true;
            delay(0.2);
        }
        return false;
    }
    // Right after play/next/previous Music can report "playing" before its
    // current track resolves (-1728 while a stream buffers). That is not a
    // failure: report no track yet and let the next poll fill it in.
    function currentTrackInfo() {
        try {
            return trackInfo(music.currentTrack);
        } catch (e) {
            return null;
        }
    }
    function snapshot() {
        const state = music.playerState();
        const track = state === 'stopped' ? null : currentTrackInfo();
        return {running: true, playing: state === 'playing', track: track,
                position: track ? music.playerPosition() : 0, volume: music.soundVolume(),
                shuffle: music.shuffleEnabled()};
    }
    // Give a just-started track up to ~2s to resolve, well inside the 8s limit.
    function awaitCurrentTrack() {
        for (let i = 0; i < 20 && currentTrackInfo() === null; ++i) delay(0.1);
    }
    // The sidebar lists the user's playlists and the Apple Music playlists
    // added to the library, which Music scripts as subscription playlists.
    function playlist(id) {
        if (id === 'library') return music.libraryPlaylists[0];
        let found = music.userPlaylists.whose({persistentID: id})();
        if (!found.length) found = music.subscriptionPlaylists.whose({persistentID: id})();
        if (!found.length) throw new Error('Playlist no longer exists in Music');
        return found[0];
    }
    // In Music's own order, without the library and anything not browsable.
    function browsablePlaylists() {
        return music.playlists().filter(function(p) {
            const cls = p.class();
            return cls === 'userPlaylist' || cls === 'subscriptionPlaylist';
        });
    }
    function page(items, offset, convert) {
        const total = items.length;
        const start = Math.min(Number(offset), total);
        const next = Math.min(start + 100, total);
        const rows = [];
        // Music's search can return entries that no longer resolve (every
        // property fails with -1728); skip them rather than fail the page.
        for (let i = start; i < next; ++i) {
            try { rows.push(convert(items[i])); } catch (e) {}
        }
        return {items: rows, offset: start, total: total, next: next};
    }
    // One Apple Event per property for the whole list, instead of five per
    // track: a 1,600-track library reads in about 0.2s this way, against
    // about 4s for every 100 tracks one by one. Falls back to the per-track
    // walk (which skips unreadable entries) if a bulk read fails or the list
    // changes between the reads.
    function trackPage(list, offset) {
        const tracks = list.tracks;
        try {
            const ids = tracks.persistentID(), names = tracks.name(), artists = tracks.artist(),
                  albums = tracks.album(), durations = tracks.duration();
            const total = ids.length;
            if ([names, artists, albums, durations].some(function(l) { return l.length !== total; })) {
                throw new Error('Music list changed while reading');
            }
            const bad = unplayableIds(list);
            const start = Math.min(Number(offset), total);
            const next = Math.min(start + 100, total);
            const rows = [];
            for (let i = start; i < next; ++i) {
                // A missing tag comes back as null in a bulk read.
                rows.push({id: ids[i], name: names[i] || '', artist: artists[i] || '',
                           album: albums[i] || '', duration: durations[i] || 0,
                           playable: !bad[ids[i]]});
            }
            return {items: rows, offset: start, total: total, next: next};
        } catch (e) {
            return page(tracks(), offset, trackInfoIn(list));
        }
    }
    switch (op) {
    case 'playlists':
        return JSON.stringify(page(browsablePlaylists(), argv[1], function(p) {
            return {id: p.persistentID(), name: p.name()};
        }));
    case 'tracks':
        return JSON.stringify(trackPage(playlist(argv[1]), argv[2]));
    case 'search':
        return JSON.stringify(page(music.search(music.libraryPlaylists[0], {for: argv[1], only: 'all'}) || [], argv[2],
                                   trackInfoIn(music.libraryPlaylists[0])));
    case 'play': {
        const context = argv[1] === 'track' ? music.libraryPlaylists[0] : playlist(argv[2]);
        const id = argv[1] === 'track' ? argv[2] : argv[3];
        let selected;
        if (id) {
            const found = context.tracks.whose({persistentID: id})();
            if (!found.length) throw new Error('Track no longer exists in Music');
            selected = found[0];
        } else {
            if (Number(argv[4]) >= context.tracks.length) throw new Error('Playlist is empty or offset is out of range');
            selected = context.tracks[Number(argv[4])];
        }
        // Music queues nothing behind a track started this way, in a
        // playlist or not: spotatui starts the following track itself.
        ensureAudible();
        music.play(selected);
        started = awaitPlaying();
        awaitCurrentTrack();
        break;
    }
    case 'resume': ensureAudible(); music.play(); started = awaitPlaying(); break;
    case 'pause': music.pause(); break;
    case 'next': music.nextTrack(); awaitCurrentTrack(); break;
    case 'previous': music.previousTrack(); awaitCurrentTrack(); break;
    case 'seek': music.playerPosition = Math.min(Number(argv[1]) / 1000, music.currentTrack.duration()); break;
    case 'volume': setVolume(Number(argv[1])); break;
    case 'snapshot': break;
    default: throw new Error('Unknown Music operation');
    }
    const result = snapshot();
    if (started !== null) result.started = started;
    return JSON.stringify(result);
}

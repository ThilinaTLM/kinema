# Player enhancements

**Status:** Draft · **Area:** playback

## Problem

The embedded player covers the basics. It lacks several features users expect
from a modern player.

## Current state

See [playback](../architecture/playback.md). Seek previews show text only. The
shortcuts are fixed and come from one catalog. The pointer is not hidden over
video. Subtitle clearance moves `sub-pos`, which styled ASS subtitles may ignore.

## Options

1. **Seek-bar thumbnails.** A separate low-resolution decode path, or a
   pre-generated sprite. It must not stall the main decoder, and it must work
   for partially downloaded media (show nothing when the bytes are missing).
2. **Configurable bindings.** Rebind keys, detect conflicts and reset to
   defaults, built on `player_shortcuts.rs`. Importing mpv's `input.conf` is not
   planned. First qualify the default bindings on Windows and macOS and on
   non-US layouts. Ctrl+arrow conflicts with macOS system shortcuts, and symbol
   keys are logical, not physical.
3. **Hide the pointer** over the video while the chrome is hidden.
4. **Remember added subtitles** (a local file or a download) per title and
   reload them on resume. Local files and online search are implemented (see
   [playback](../architecture/playback.md#external-subtitles)); this would make
   them persisted state, likely a SQLite change (High risk).
5. **Minimize and occlusion suppression.** Stop rendering while the window is
   hidden. This must not imply that the download is suspended (see
   [media storage options](media-storage-options.md)).
6. **4K and hardware decoding qualification** on real hardware. This includes
   checking that decoding is sustained and that frames don't drop.
7. **ASS subtitle clearance** that works with styled subtitles.

## Open questions

- Which candidates matter most for the first release?

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QByteArray>
#include <QList>
#include <QString>

namespace kinema::core::tracks {

/**
 * One mpv `track-list` entry, flattened into a value type the UI can
 * consume without any libmpv include. Populated by `parseTrackList`
 * from the JSON representation mpv produces for the `track-list`
 * property.
 *
 * Only the fields Kinema actually uses are surfaced; the parser
 * silently drops everything else. Unknown / missing fields leave the
 * corresponding member at its default value rather than throwing.
 */
struct Entry {
    /// mpv track id, 1-based per type. -1 indicates an unparsable
    /// entry the caller should ignore.
    int id = 0;
    /// "audio" | "sub" | "video". Other types are passed through
    /// verbatim; callers filter on this.
    QString type;
    QString title;
    /// ISO 639-2 / ISO 639-1 language code, as mpv reports it.
    QString lang;
    QString codec;
    bool selected = false;
    bool isDefault = false;
    bool forced = false;
    bool external = false;
    /// Audio channel count (`demux-channel-count`); zero for
    /// non-audio tracks or when mpv didn't supply it.
    int channelCount = 0;
    /// Video dimensions (`demux-w` / `demux-h`); zero for non-video.
    int width = 0;
    int height = 0;
};

using TrackList = QList<Entry>;

/**
 * Parse the JSON representation of mpv's `track-list` property.
 * libmpv returns this either via `mpv_get_property_string(track-list)`
 * or by serialising an `MPV_FORMAT_NODE` to JSON with
 * `mpv_node_to_json`; both encodings are accepted here.
 *
 * Malformed JSON, non-array roots, and non-object elements are
 * tolerated — `parseTrackList` returns an empty list on any fatal
 * parse error and silently drops per-entry failures, so the UI can
 * keep running when mpv produces an unexpected shape.
 */
TrackList parseTrackList(const QByteArray& json);

/**
 * Human-readable label for a track, suitable as a menu item.
 *
 * Examples:
 *   - `"English — AC-3 5.1"`
 *   - `"French (forced)"`
 *   - `"Track 3"` (when no language / title is known)
 *
 * Never throws; callers can pass any Entry.
 */
QString formatLabel(const Entry& e);

/**
 * Serialise a track list into the compact JSON shape the
 * `kinema-overlays` Lua script consumes via its `set-tracks`
 * message. Only audio and subtitle tracks are emitted; video
 * tracks are dropped because the pickers never offer them.
 *
 * Shape (stable — versioned by the IPC protocol):
 *
 *     {
 *       "audio":    [{ id, lang, title, codec, channels, selected }, ...],
 *       "subtitle": [{ id, lang, title, codec, forced, selected }, ...]
 *     }
 *
 * Fields that aren't known are omitted rather than emitted as
 * empty strings, so the Lua side can cheaply test presence.
 */
QByteArray toIpcJson(const TrackList& list);

} // namespace kinema::core::tracks

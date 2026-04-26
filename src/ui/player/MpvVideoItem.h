// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "core/MpvChapterList.h"
#include "core/MpvTrackList.h"

#include <MpvAbstractItem>

#include <QString>
#include <QStringList>
#include <QUrl>
#include <QVariant>
#include <QtQmlIntegration/qqmlintegration.h>

#include <deque>
#include <optional>

namespace kinema::config {
class PlayerSettings;
}

namespace kinema::ui::player {

/**
 * QtQuick item that renders an in-process libmpv instance. Subclasses
 * `MpvAbstractItem` from the KDE-maintained `mpvqt` library, which
 * handles the offscreen FBO + worker thread that own
 * `mpv_render_context`.
 *
 * MpvVideoItem is the one place in the codebase that knows about
 * libmpv property names. Everything else (chrome, controllers,
 * persistence) consumes this item's signals / `Q_PROPERTY`s.
 *
 * Threading: the underlying `MpvController` runs on its own QThread;
 * its `propertyChanged` signal is delivered to this object's slot
 * via `Qt::QueuedConnection`, so all observable state lives on the
 * GUI thread.
 *
 * Lifecycle: PlayerSettings is injected at construction so the
 * widget can translate Kinema-level preferences (hardware decoding,
 * preferred audio / subtitle language, remembered volume) into mpv
 * options before the first `loadFile`. Tests may construct the item
 * without a settings object — in that case all options revert to
 * mpv's defaults, which is fine for parser-level tests that never
 * actually start playback.
 */
class MpvVideoItem : public MpvAbstractItem
{
    Q_OBJECT
    QML_NAMED_ELEMENT(MpvVideoItem)

    // ---- Bindable transport state ------------------------------------
    //
    // QML chrome binds against these directly. Setters that mutate
    // mpv go through invokable slots (loadFile, setPaused, etc.) so
    // QML never writes the property — Qt Quick respects that
    // pattern via the read-only declaration.
    Q_PROPERTY(bool paused READ isPaused NOTIFY pausedChanged)
    Q_PROPERTY(double position READ position NOTIFY positionChanged)
    Q_PROPERTY(double duration READ duration NOTIFY durationChanged)
    Q_PROPERTY(double volume READ volume NOTIFY volumeChanged)
    Q_PROPERTY(bool muted READ isMuted NOTIFY muteChanged)
    Q_PROPERTY(double speed READ speed NOTIFY speedChanged)
    Q_PROPERTY(bool buffering READ isBuffering NOTIFY bufferingChanged)
    Q_PROPERTY(int bufferingPercent READ bufferingPercent
        NOTIFY bufferingChanged)
    Q_PROPERTY(double cacheAhead READ cacheAhead
        NOTIFY cacheAheadChanged)
    Q_PROPERTY(int audioTrackId READ audioTrackId
        NOTIFY audioTrackChanged)
    Q_PROPERTY(int subtitleTrackId READ subtitleTrackId
        NOTIFY subtitleTrackChanged)

public:
    explicit MpvVideoItem(QQuickItem* parent = nullptr);
    ~MpvVideoItem() override;

    /// Inject the user's player preferences. Applied lazily on the
    /// first call so the worker thread has finished `mpv_initialize`
    /// before we start writing options. Calling more than once is a
    /// no-op except for the remembered volume.
    void applySettings(const config::PlayerSettings& settings);

    // ---- Imperative slots used by PlayerWindow / PlayerViewModel -----

    Q_INVOKABLE void loadFile(const QUrl& url,
        std::optional<double> startSeconds = std::nullopt);
    Q_INVOKABLE void stop();
    Q_INVOKABLE void setPaused(bool paused);
    Q_INVOKABLE void cyclePause();
    Q_INVOKABLE void seekAbsolute(double seconds);
    Q_INVOKABLE void seekRelative(double seconds);
    Q_INVOKABLE void setVolumePercent(double v);
    Q_INVOKABLE void setMuted(bool m);
    Q_INVOKABLE void setSpeed(double s);
    Q_INVOKABLE void setAudioTrack(int id);
    Q_INVOKABLE void setSubtitleTrack(int id);
    Q_INVOKABLE void setMediaTitle(const QString& title);

    /// Sideload a subtitle file via mpv's `sub-add` command. `select`
    /// true makes the new track active; `auto` lets mpv decide based
    /// on its own slang/forced rules. `lang` is the ISO 639-2 code
    /// ("eng"); `title` is the display label that shows up in
    /// `track-list`.
    Q_INVOKABLE void addSubtitleFile(const QString& path,
        const QString& title, const QString& lang, bool select);

    // ---- Cached property accessors -----------------------------------

    bool isPaused() const noexcept { return m_paused; }
    double position() const noexcept { return m_position; }
    double duration() const noexcept { return m_duration; }
    double volume() const noexcept { return m_volume; }
    bool isMuted() const noexcept { return m_muted; }
    double speed() const noexcept { return m_speed; }
    bool isBuffering() const noexcept { return m_bufferingWaiting; }
    int bufferingPercent() const noexcept { return m_bufferingPercent; }
    double cacheAhead() const noexcept { return m_cacheAhead; }
    int audioTrackId() const noexcept { return m_audioTrackId; }
    int subtitleTrackId() const noexcept { return m_subtitleTrackId; }

    /// The most recent track-list parsed from mpv. Owned by this
    /// item; `PlayerViewModel` mirrors it into the QML-visible
    /// audio / subtitle list models.
    const core::tracks::TrackList& tracks() const noexcept
    {
        return m_tracks;
    }
    /// The most recent chapter list.
    const core::chapters::ChapterList& chapters() const noexcept
    {
        return m_chapters;
    }

    /// Snapshot of decoder / cache statistics. Used by the chip
    /// row and by `HistoryController` for codec-aware track memory.
    struct VideoStats {
        int width = 0;
        int height = 0;
        QString videoCodec;
        QString audioCodec;
        double fps = 0.0;
        double videoBitrate = 0.0;
        double audioBitrate = 0.0;
        double cacheSeconds = 0.0;
        int audioChannels = 0;
        QString hdrPrimaries;
        QString hdrGamma;
    };
    VideoStats currentStats() const noexcept { return m_stats; }

    /// Most recent log lines from mpv's `MPV_LOG_LEVEL_INFO` stream.
    /// Used by `PlaybackController` to classify end-file errors.
    QStringList recentLogLines() const;

Q_SIGNALS:
    void fileLoaded();
    void endOfFile(const QString& reason);
    void positionChanged(double seconds);
    void durationChanged(double seconds);
    void pausedChanged(bool paused);
    void volumeChanged(double percent);
    void muteChanged(bool muted);
    void speedChanged(double factor);
    void bufferingChanged(bool waiting, int percent);
    void cacheAheadChanged(double seconds);
    void trackListChanged(const core::tracks::TrackList& tracks);
    void chaptersChanged(const core::chapters::ChapterList& chapters);
    void audioTrackChanged(int id);
    void subtitleTrackChanged(int id);
    void videoStatsChanged(const VideoStats& stats);
    void mpvError(const QString& message);

private Q_SLOTS:
    void onMpvFileLoaded();
    void onMpvEndFile(const QString& reason);
    void onMpvPropertyChanged(const QString& name, const QVariant& value);

private:
    void initObservers();
    void parseTrackListNode(const QVariant& nodeValue);
    void parseChapterListNode(const QVariant& nodeValue);
    void appendLogLine(const QString& line);

    bool m_observersInstalled = false;

    // Cached state. Defaults match "no sample observed yet"; chrome
    // bindings should treat -1 / 0 sentinels as "unknown" until the
    // matching signal fires.
    bool m_paused = false;
    double m_position = -1.0;
    double m_duration = -1.0;
    double m_volume = -1.0;
    bool m_muted = false;
    double m_speed = 1.0;
    bool m_bufferingWaiting = false;
    int m_bufferingPercent = 0;
    double m_cacheAhead = -1.0;
    int m_audioTrackId = -1;
    int m_subtitleTrackId = -1;

    core::tracks::TrackList m_tracks;
    core::chapters::ChapterList m_chapters;
    VideoStats m_stats;

    // Optional remembered volume, applied to mpv on the first
    // loadFile() so the very first frames decode at the saved level.
    std::optional<double> m_pendingVolume;

    // Bounded ring of recent log lines for end-file classification.
    std::deque<QString> m_logTail;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV

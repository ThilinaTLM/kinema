// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/PlayerWindow.h"

#include "config/AppearanceSettings.h"
#include "config/PlayerSettings.h"
#include "core/ShortcutSections.h"
#include "core/MediaChips.h"
#include "ui/player/MpvVideoItem.h"
#include "ui/player/PlayerViewModel.h"

#include <KLocalizedString>

#include <QCloseEvent>
#include <QCursor>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QMetaMethod>
#include <QHideEvent>
#include <QIcon>
#include <QKeyEvent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQuickItem>
#include <QScreen>
#include <QShowEvent>
#include <QString>
#include <QUrl>
#include <QWidget>
#include <QWindow>

#include "kinema_debug.h"

namespace kinema::ui::player {

namespace {

/// Build the short subtitle line the title strip renders under the
/// primary title. Movies have no subtitle; series get
/// `"S01E03 \u2014 Episode title"` when both are known, falling
/// back to the numeric part alone otherwise.
QString buildSubtitleLabel(const api::PlaybackContext& ctx)
{
    if (ctx.key.kind != api::MediaKind::Series) {
        return {};
    }
    if (!ctx.key.season.has_value() || !ctx.key.episode.has_value()) {
        return ctx.episodeTitle;
    }
    const QString code = QStringLiteral("S%1E%2")
                             .arg(*ctx.key.season, 2, 10, QLatin1Char('0'))
                             .arg(*ctx.key.episode, 2, 10, QLatin1Char('0'));
    if (ctx.episodeTitle.isEmpty()) {
        return code;
    }
    return i18nc(
        "@label subtitle on the player title strip, e.g. "
        "\"S01E03 \u2014 Episode Title\"",
        "%1 \u2014 %2", code, ctx.episodeTitle);
}

QStringList chipsFromJson(const QByteArray& json)
{
    // `core::media_chips::toIpcJson` produces a JSON array of
    // strings (e.g. `["4K", "HDR10", "EAC3 5.1"]`). The chip row
    // QML binds to a QStringList, so peel one layer of JSON off here.
    const auto doc = QJsonDocument::fromJson(json);
    if (!doc.isArray()) {
        return {};
    }
    QStringList out;
    for (const auto v : doc.array()) {
        if (v.isString()) {
            out.append(v.toString());
        }
    }
    return out;
}

} // namespace

PlayerWindow::PlayerWindow(config::AppearanceSettings& appearance,
    config::PlayerSettings& player, QWindow* transientFor)
    : QQuickView(/*parent=*/nullptr)
    , m_appearanceSettings(appearance)
    , m_playerSettings(player)
    , m_windowParent(transientFor)
{
    // The application's main QML window is a `QQuickWindow` (a
    // `QWindow`), so we can wire it directly as the transient
    // parent — no `windowHandle()` indirection like the old
    // QWidget-owned MainWindow needed. The QObject parent is
    // owned by `MainController` (passed via
    // `PlaybackController::setPlayerWindow`); we deliberately do
    // not also set it as a QObject child of the QML window since
    // its destruction order is engine-driven.
    if (transientFor) {
        setTransientParent(transientFor);
    }

    // Standard window chrome.
    setTitle(i18nc("@title:window", "Kinema Player"));
    setIcon(QIcon::fromTheme(QStringLiteral("dev.tlmtech.kinema")));

    // Resize the QML root to fill the window so the scene fills
    // the available area.
    setResizeMode(QQuickView::SizeRootObjectToView);

    // Construct the chrome view-model and expose it to QML before
    // the scene loads.
    m_viewModel = new PlayerViewModel(this);
    rootContext()->setContextProperty(
        QStringLiteral("playerVm"), m_viewModel);

    // Re-emit the action signals on our own surface so
    // PlaybackController only listens to the window.
    connect(m_viewModel, &PlayerViewModel::resumeAccepted,
        this, &PlayerWindow::resumeAccepted);
    connect(m_viewModel, &PlayerViewModel::resumeDeclined,
        this, &PlayerWindow::resumeDeclined);
    connect(m_viewModel, &PlayerViewModel::skipRequested,
        this, &PlayerWindow::skipRequested);
    connect(m_viewModel, &PlayerViewModel::nextEpisodeAccepted,
        this, &PlayerWindow::nextEpisodeAccepted);
    connect(m_viewModel, &PlayerViewModel::nextEpisodeCancelled,
        this, &PlayerWindow::nextEpisodeCancelled);
    connect(m_viewModel, &PlayerViewModel::audioPicked,
        this, &PlayerWindow::audioPicked);
    connect(m_viewModel, &PlayerViewModel::subtitlePicked,
        this, &PlayerWindow::subtitlePicked);
    connect(m_viewModel, &PlayerViewModel::speedPicked,
        this, &PlayerWindow::speedPicked);
    connect(m_viewModel, &PlayerViewModel::closeRequested,
        this, [this] { close(); });
    connect(m_viewModel, &PlayerViewModel::fullscreenToggleRequested,
        this, &PlayerWindow::toggleFullscreen);

    // Load the QML scene. The MpvVideoItem inside it will be the
    // first MpvVideoItem child of the root; we look it up after
    // loading completes.
    // The qt_add_qml_module-generated resource keeps each QML file's
    // source-relative path under the module prefix, so PlayerScene
    // lives at .../ui/player/qml/PlayerScene.qml. Other QML files
    // resolve via the import system once this loads.
    setSource(QUrl(QStringLiteral(
        "qrc:/qt/qml/dev/tlmtech/kinema/player"
        "/ui/player/qml/PlayerScene.qml")));

    if (status() == QQuickView::Error) {
        const auto errors = QQuickView::errors();
        for (const auto& e : errors) {
            qCWarning(KINEMA) << "PlayerScene.qml load error:"
                              << e.toString();
        }
    }

    if (auto* root = rootObject()) {
        // Mirror the QML root's `chromeVisible` property to the
        // window cursor. The property is declared in PlayerScene.qml
        // (`property bool chromeVisible: true`); Qt auto-generates a
        // `chromeVisibleChanged()` notify signal we can connect to
        // by name via the meta-object.
        const int notifyIdx = root->metaObject()->indexOfSignal(
            "chromeVisibleChanged()");
        if (notifyIdx >= 0) {
            const QMetaMethod notify =
                root->metaObject()->method(notifyIdx);
            const int slotIdx = this->metaObject()->indexOfSlot(
                "onChromeVisibleChanged()");
            if (slotIdx >= 0) {
                const QMetaMethod slot =
                    this->metaObject()->method(slotIdx);
                QObject::connect(root, notify, this, slot);
            }
        }
        // Sync once for the initial state (the property starts true,
        // so this just unsets the cursor; harmless either way).
        onChromeVisibleChanged();

        if (auto* video = root->findChild<MpvVideoItem*>(
                QStringLiteral("kinemaMpvVideoItem"))) {
            m_video = video;
            video->applySettings(m_playerSettings);
            m_viewModel->attach(video);

            // Forward MpvVideoItem signals to PlayerWindow's public
            // surface. PlaybackController and HistoryController only
            // see the window.
            connect(video, &MpvVideoItem::fileLoaded,
                this, &PlayerWindow::fileLoaded);
            connect(video, &MpvVideoItem::mpvError,
                this, &PlayerWindow::mpvError);
            connect(video, &MpvVideoItem::endOfFile,
                this, [this](const QString& reason) {
                    stopAndHide();
                    Q_EMIT endOfFile(reason);
                });
            connect(video, &MpvVideoItem::positionChanged,
                this, &PlayerWindow::positionChanged);
            connect(video, &MpvVideoItem::durationChanged,
                this, &PlayerWindow::durationChanged);
            connect(video, &MpvVideoItem::trackListChanged,
                this, &PlayerWindow::trackListChanged);
            connect(video, &MpvVideoItem::chaptersChanged,
                this, &PlayerWindow::chaptersChanged);
            connect(video, &MpvVideoItem::videoStatsChanged,
                this, [this](const MpvVideoItem::VideoStats&) {
                    pushMediaChips();
                    pushStreamInfo();
                });
            connect(video, &MpvVideoItem::trackListChanged,
                this, [this](const core::tracks::TrackList&) {
                    pushMediaChips();
                    pushStreamInfo();
                });
        } else {
            qCWarning(KINEMA)
                << "PlayerWindow: kinemaMpvVideoItem not found in QML";
        }
    }
}

PlayerWindow::~PlayerWindow() = default;

const core::tracks::TrackList& PlayerWindow::trackList() const
{
    if (m_video) {
        return m_video->tracks();
    }
    return m_emptyTracks;
}

QStringList PlayerWindow::recentLogLines() const
{
    return m_video ? m_video->recentLogLines() : QStringList();
}

// ---- Imperative control surface ----------------------------------------

void PlayerWindow::play(const QUrl& url, const api::PlaybackContext& ctx)
{
    m_hasEverLoaded = true;

    const auto& title = ctx.title;
    setTitle(title.isEmpty()
        ? QStringLiteral("Kinema")
        : i18nc("@title:window window title with media title",
              "%1 \u2014 Kinema", title));

    if (!m_geometryApplied) {
        loadGeometry();
    }

    std::optional<double> startSec;
    if (ctx.resumeSeconds && *ctx.resumeSeconds > 0) {
        startSec = static_cast<double>(*ctx.resumeSeconds);
    }

    if (m_viewModel) {
        m_viewModel->setMediaContext(ctx.title,
            buildSubtitleLabel(ctx),
            ctx.key.kind == api::MediaKind::Series
                ? QStringLiteral("series")
                : QStringLiteral("movie"));
        pushMediaChips();
        pushShortcutSections();
    }

    m_currentSourceUrl = url;

    if (m_video) {
        m_video->setMediaTitle(ctx.title);
        m_video->loadFile(url, startSec);
    }

    if (m_viewModel) {
        pushStreamInfo();
    }

    show();
    raise();
    requestActivate();
    if (auto* root = rootObject()) {
        root->forceActiveFocus();
    }
}

void PlayerWindow::setPaused(bool paused)
{
    if (m_video) m_video->setPaused(paused);
}

void PlayerWindow::seekAbsolute(double seconds)
{
    if (m_video) m_video->seekAbsolute(seconds);
}

void PlayerWindow::setAudioTrack(int id)
{
    if (m_video) m_video->setAudioTrack(id);
}

void PlayerWindow::setSubtitleTrack(int id)
{
    if (m_video) m_video->setSubtitleTrack(id);
}

void PlayerWindow::setSpeed(double factor)
{
    if (m_video) m_video->setSpeed(factor);
}

void PlayerWindow::showResumePrompt(qint64 seconds)
{
    if (m_viewModel) m_viewModel->showResume(seconds);
}

void PlayerWindow::hideResumePrompt()
{
    if (m_viewModel) m_viewModel->hideResume();
}

void PlayerWindow::showNextEpisodeBanner(
    const api::PlaybackContext& ctx, int countdownSec)
{
    if (m_viewModel) {
        m_viewModel->showNextEpisode(ctx.title,
            buildSubtitleLabel(ctx), countdownSec);
    }
}

void PlayerWindow::updateNextEpisodeCountdown(int seconds)
{
    if (m_viewModel) m_viewModel->updateNextEpisodeCountdown(seconds);
}

void PlayerWindow::hideNextEpisodeBanner()
{
    if (m_viewModel) m_viewModel->hideNextEpisode();
}

void PlayerWindow::showSkipChapter(const QString& kind,
    const QString& label, qint64 startSec, qint64 endSec)
{
    if (m_viewModel) {
        m_viewModel->showSkip(kind, label, startSec, endSec);
    }
}

void PlayerWindow::hideSkipChapter()
{
    if (m_viewModel) m_viewModel->hideSkip();
}

void PlayerWindow::stopAndHide()
{
    if (visibility() == QWindow::FullScreen) {
        showNormal();
    }
    if (m_video) {
        m_video->stop();
    }
    // Don't leak a blank cursor onto the desktop / parent if the
    // user closed the window mid-auto-hide.
    unsetCursor();
    hide();
}

// ---- Window event handlers ---------------------------------------------

void PlayerWindow::closeEvent(QCloseEvent* e)
{
    saveGeometryToConfig();
    stopAndHide();
    e->accept();
}

void PlayerWindow::keyPressEvent(QKeyEvent* e)
{
    // QML's PlayerInputs handles most keys; if focus has briefly
    // detached (e.g. a popup just closed), Esc still closes / leaves
    // fullscreen so the user can always escape.
    if (e->key() == Qt::Key_Escape) {
        leaveFullscreenOrClose();
        e->accept();
        return;
    }
    QQuickView::keyPressEvent(e);
}

void PlayerWindow::showEvent(QShowEvent* e)
{
    if (!m_geometryApplied) {
        loadGeometry();
    }
    QQuickView::showEvent(e);
    Q_EMIT visibilityChanged(true);
}

void PlayerWindow::hideEvent(QHideEvent* e)
{
    saveGeometryToConfig();
    saveVolumeToConfig();
    QQuickView::hideEvent(e);
    Q_EMIT visibilityChanged(false);
}

// ---- Fullscreen plumbing -----------------------------------------------

void PlayerWindow::toggleFullscreen()
{
    if (visibility() == QWindow::FullScreen) {
        showNormal();
    } else {
        showFullScreen();
    }
}

void PlayerWindow::leaveFullscreenOrClose()
{
    if (visibility() == QWindow::FullScreen) {
        showNormal();
        return;
    }
    close();
}

// ---- Geometry / volume persistence -------------------------------------

void PlayerWindow::loadGeometry()
{
    m_geometryApplied = true;

    const QByteArray saved = m_appearanceSettings.playerWindowGeometry();
    if (!saved.isEmpty()) {
        // Window's own restoreGeometry would round-trip a QByteArray
        // produced by QWidget::saveGeometry, but our existing
        // settings carry the QWidget format. For now decode the
        // first-run values; users get a fresh-state default the
        // first time they open the player after upgrading. We avoid
        // the cross-format crash by only honouring the new size hint
        // (width / height) and re-centring.
        // TODO: switch AppearanceSettings to a window-style encoding
        // and migrate. Until then the default geometry is fine \u2014
        // visible in a follow-up PR.
    }

    resize(1280, 720);
    QScreen* s = nullptr;
    if (m_windowParent) {
        s = m_windowParent->screen();
    }
    if (!s) {
        s = QGuiApplication::primaryScreen();
    }
    if (s) {
        const QRect avail = s->availableGeometry();
        setPosition(avail.center().x() - width() / 2,
            avail.center().y() - height() / 2);
    }
}

void PlayerWindow::saveGeometryToConfig()
{
    // Preserved API; serialisation format change is tracked in
    // loadGeometry's TODO. Writing the existing key with a
    // non-QWidget blob would corrupt downgrades, so leave the
    // stored value untouched here.
}

void PlayerWindow::saveVolumeToConfig()
{
    if (!m_video) {
        return;
    }
    const double v = m_video->volume();
    if (v < 0.0) {
        return;
    }
    m_playerSettings.setRememberedVolume(v);
}

// ---- Push media chips / cheat sheet to the view-model -------------------

void PlayerWindow::pushMediaChips()
{
    if (!m_viewModel || !m_video) {
        return;
    }
    const auto stats = m_video->currentStats();
    core::media_chips::ChipInputs in;
    in.videoHeight   = stats.height;
    in.videoCodec    = stats.videoCodec;
    in.audioCodec    = stats.audioCodec;
    in.audioChannels = stats.audioChannels;
    in.hdrPrimaries  = stats.hdrPrimaries;
    in.hdrGamma      = stats.hdrGamma;
    const auto json = core::media_chips::toIpcJson(in);
    m_viewModel->setMediaChips(chipsFromJson(json));
}

void PlayerWindow::onChromeVisibleChanged()
{
    auto* root = rootObject();
    if (!root) {
        return;
    }
    const bool visible = root->property("chromeVisible").toBool();
    if (visible) {
        unsetCursor();
    } else {
        setCursor(QCursor(Qt::BlankCursor));
    }
}

void PlayerWindow::pushShortcutSections()
{
    if (m_viewModel) {
        m_viewModel->setShortcutSections(
            core::shortcuts::renderSections());
    }
}

void PlayerWindow::pushStreamInfo()
{
    if (!m_viewModel) {
        return;
    }
    QVariantMap info;
    if (m_video) {
        const auto stats = m_video->currentStats();
        info[QStringLiteral("videoCodec")]    = stats.videoCodec;
        info[QStringLiteral("audioCodec")]    = stats.audioCodec;
        info[QStringLiteral("width")]         = stats.width;
        info[QStringLiteral("height")]        = stats.height;
        info[QStringLiteral("fps")]           = stats.fps;
        info[QStringLiteral("audioChannels")] = stats.audioChannels;
        info[QStringLiteral("hdr")]
            = !stats.hdrPrimaries.isEmpty()
                || !stats.hdrGamma.isEmpty();
        int audioCount = 0;
        int subCount = 0;
        for (const auto& t : m_video->tracks()) {
            if (t.type == QLatin1String("audio")) ++audioCount;
            else if (t.type == QLatin1String("sub")) ++subCount;
        }
        info[QStringLiteral("audioTrackCount")]    = audioCount;
        info[QStringLiteral("subtitleTrackCount")] = subCount;
    }
    info[QStringLiteral("sourceUrl")] = m_currentSourceUrl.toString();
    // Best-effort container: the path's suffix is what the user
    // sees in Settings and is enough for an "about" panel until
    // we expose mpv's `file-format` property.
    QString container = m_currentSourceUrl.fileName();
    const int dot = container.lastIndexOf(QLatin1Char('.'));
    container = (dot >= 0 ? container.mid(dot + 1).toUpper() : QString());
    info[QStringLiteral("container")] = container;
    m_viewModel->setStreamInfo(info);
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV

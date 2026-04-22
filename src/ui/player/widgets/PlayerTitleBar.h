// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "api/PlaybackContext.h"
#include "core/MpvTrackList.h"

#include <QWidget>

class QLabel;
class QToolButton;

namespace kinema::ui::player::widgets {

/**
 * Top-strip overlay above the video. Mirrors the auto-show / auto-hide
 * cadence of TransportBar; PlayerOverlay owns both timers and toggles
 * visibility in sync.
 *
 * Displays a back-arrow (close) on the left, the media title + an
 * optional episode subtitle centred, and a settings gear + close
 * button on the right. The gear opens a QMenu composed via
 * `widgets::menus::populateAudioMenu` / `populateSubtitleMenu` /
 * `populateSpeedMenu` plus the stats / cheat-sheet toggles.
 *
 * Ownership model: the bar does not touch mpv directly. It emits
 * `audioTrackRequested`, `subtitleTrackRequested`, `speedRequested`,
 * `statsToggleRequested`, `cheatSheetToggleRequested`, and
 * `closeRequested`; PlayerOverlay routes them into the right places.
 */
class PlayerTitleBar : public QWidget
{
    Q_OBJECT
public:
    explicit PlayerTitleBar(QWidget* parent = nullptr);

    /// Push the current PlaybackContext. Updates the two label lines.
    void setContext(const api::PlaybackContext& ctx);
    /// Push the latest track list + selected speed so the settings
    /// menu (built on demand when the gear is clicked) reflects
    /// current mpv state.
    void setTrackList(const core::tracks::TrackList& tracks);
    void setSpeed(double factor);

Q_SIGNALS:
    void closeRequested();
    void audioTrackRequested(int id);
    void subtitleTrackRequested(int id);
    void speedRequested(double factor);
    void statsToggleRequested();
    void cheatSheetToggleRequested();

protected:
    void paintEvent(QPaintEvent* e) override;

private Q_SLOTS:
    void onSettingsButtonClicked();

private:
    QToolButton* m_backButton {nullptr};
    QLabel* m_titleLabel {nullptr};
    QLabel* m_subtitleLabel {nullptr};
    QToolButton* m_settingsButton {nullptr};
    QToolButton* m_closeButton {nullptr};

    core::tracks::TrackList m_tracks;
    double m_speed {1.0};
};

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV

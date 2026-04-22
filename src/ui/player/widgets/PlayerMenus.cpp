// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/player/widgets/PlayerMenus.h"

#include <KLocalizedString>

#include <QAction>
#include <QMenu>

#include <cmath>

namespace kinema::ui::player::widgets::menus {

namespace {

QAction* addCheckable(QMenu* menu, const QString& label, bool checked)
{
    auto* a = menu->addAction(label);
    a->setCheckable(true);
    a->setChecked(checked);
    return a;
}

} // namespace

void populateAudioMenu(QMenu* menu, const core::tracks::TrackList& tracks,
    std::function<void(int)> onSelect)
{
    if (!menu) {
        return;
    }
    menu->clear();

    bool anyAudio = false;
    for (const auto& t : tracks) {
        if (t.type != QLatin1String("audio")) {
            continue;
        }
        anyAudio = true;
        auto* a = addCheckable(menu,
            core::tracks::formatLabel(t), t.selected);
        const int id = t.id;
        QObject::connect(a, &QAction::triggered, menu,
            [id, onSelect]() {
                if (onSelect) {
                    onSelect(id);
                }
            });
    }

    if (!anyAudio) {
        auto* empty = menu->addAction(i18nc(
            "@item:inmenu shown when the file has no audio tracks",
            "No audio tracks"));
        empty->setEnabled(false);
    }
}

void populateSubtitleMenu(QMenu* menu,
    const core::tracks::TrackList& tracks,
    std::function<void(int)> onSelect)
{
    if (!menu) {
        return;
    }
    menu->clear();

    // Any subtitle selected?
    bool anySelected = false;
    for (const auto& t : tracks) {
        if (t.type == QLatin1String("sub") && t.selected) {
            anySelected = true;
            break;
        }
    }

    auto* none = addCheckable(menu,
        i18nc("@item:inmenu disables subtitles", "None"),
        !anySelected);
    QObject::connect(none, &QAction::triggered, menu, [onSelect]() {
        if (onSelect) {
            onSelect(-1);
        }
    });

    bool anySub = false;
    for (const auto& t : tracks) {
        if (t.type != QLatin1String("sub")) {
            continue;
        }
        if (!anySub) {
            menu->addSeparator();
            anySub = true;
        }
        auto* a = addCheckable(menu,
            core::tracks::formatLabel(t), t.selected);
        const int id = t.id;
        QObject::connect(a, &QAction::triggered, menu,
            [id, onSelect]() {
                if (onSelect) {
                    onSelect(id);
                }
            });
    }
}

void populateSpeedMenu(QMenu* menu, double current,
    std::function<void(double)> onSelect)
{
    if (!menu) {
        return;
    }
    menu->clear();

    // Fixed ladder. Users wanting arbitrary speeds still have the
    // `[` / `]` mpv bindings.
    static constexpr double kSpeeds[] = {
        0.5, 0.75, 1.0, 1.25, 1.5, 2.0,
    };

    for (double s : kSpeeds) {
        const QString label = qFuzzyCompare(s, 1.0)
            ? i18nc("@item:inmenu normal playback speed", "Normal (1.0\u00d7)")
            : i18nc("@item:inmenu playback speed multiplier, "
                    "e.g. \"1.25\u00d7\"",
                  "%1\u00d7",
                  QString::number(s, 'g', 3));
        auto* a = addCheckable(menu, label,
            std::abs(s - current) < 0.01);
        QObject::connect(a, &QAction::triggered, menu, [s, onSelect]() {
            if (onSelect) {
                onSelect(s);
            }
        });
    }
}

} // namespace kinema::ui::player::widgets::menus

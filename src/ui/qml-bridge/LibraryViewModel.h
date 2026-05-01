// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"
#include "ui/qml-bridge/LibraryListModel.h"

#include <QList>
#include <QObject>
#include <QString>

namespace kinema::controllers {
class LibraryController;
}

namespace kinema::ui::qml {

class LibraryViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(LibraryListModel* model READ model CONSTANT)
    Q_PROPERTY(Section section READ section WRITE setSection NOTIFY sectionChanged)
    Q_PROPERTY(bool empty READ empty NOTIFY modelChanged)
    Q_PROPERTY(int continueCount READ continueCount NOTIFY countsChanged)
    Q_PROPERTY(int toWatchCount READ toWatchCount NOTIFY countsChanged)
    Q_PROPERTY(int watchedCount READ watchedCount NOTIFY countsChanged)
    Q_PROPERTY(int upcomingCount READ upcomingCount NOTIFY countsChanged)

public:
    enum class Section {
        Continue = 0,
        ToWatch,
        Watched,
        Upcoming,
    };
    Q_ENUM(Section)

    explicit LibraryViewModel(controllers::LibraryController* library,
        QObject* parent = nullptr);
    ~LibraryViewModel() override;

    LibraryListModel* model() const noexcept { return m_model; }
    Section section() const noexcept { return m_section; }
    bool empty() const noexcept { return m_model->rowCount() == 0; }
    int continueCount() const noexcept { return m_continueRows.size(); }
    int toWatchCount() const noexcept { return m_toWatchRows.size(); }
    int watchedCount() const noexcept { return m_watchedRows.size(); }
    int upcomingCount() const noexcept { return m_upcomingRows.size(); }

public Q_SLOTS:
    void refresh();
    void activate(int row);
    void resume(int row);
    void setSection(Section section);
    void removeFromLibrary(int row, bool hardDelete);
    void toggleWatched(int row);

Q_SIGNALS:
    void sectionChanged();
    void modelChanged();
    void countsChanged();
    void openMovieRequested(const QString& imdbId, const QString& title);
    void openSeriesRequested(const QString& imdbId, const QString& title);
    void openSeriesEpisodeRequested(const QString& imdbId,
        const QString& title, int season, int episode);
    void resumeRequested(const api::HistoryEntry& entry);
    void confirmRemoveRequested(int row, const QString& title);
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    void rebuildRows();
    void publishSection();
    QList<LibraryListRow>& rowsFor(Section section);
    const QList<LibraryListRow>& rowsFor(Section section) const;

    controllers::LibraryController* m_library {};
    LibraryListModel* m_model {};
    Section m_section = Section::ToWatch;
    bool m_sectionInitialized = false;
    bool m_userSelectedSection = false;

    QList<LibraryListRow> m_continueRows;
    QList<LibraryListRow> m_toWatchRows;
    QList<LibraryListRow> m_watchedRows;
    QList<LibraryListRow> m_upcomingRows;
};

} // namespace kinema::ui::qml

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"
#include "ui/qml-bridge/streams/StreamsListModel.h"

#include <QObject>
#include <QString>

namespace kinema::config {
class FilterSettings;
}

namespace kinema::ui::qml {

/**
 * Cohesive state and rendering policy for a detail page's stream list.
 * Network loading and stream actions remain in the owning page view-model.
 */
class StreamListState : public QObject
{
    Q_OBJECT
public:
    StreamListState(config::FilterSettings& filters,
                    domain::MediaKind mediaKind,
                    QObject* parent = nullptr);

    StreamsListModel* model() const noexcept { return m_model; }
    const QList<domain::Stream>& rawStreams() const noexcept { return m_rawStreams; }
    int rawStreamsCount() const noexcept { return m_rawStreams.size(); }

    int sortMode() const noexcept { return static_cast<int>(m_sortMode); }
    void setSortMode(int mode);
    bool sortDescending() const noexcept { return m_sortDescending; }
    void setSortDescending(bool descending);

    QString resolutionFilter() const { return m_uiFilters.resolution; }
    void setResolutionFilter(const QString& resolution);
    bool hdrOnly() const noexcept { return m_uiFilters.hdrOnly; }
    void setHdrOnly(bool enabled);
    bool dolbyVisionOnly() const noexcept { return m_uiFilters.dolbyVisionOnly; }
    void setDolbyVisionOnly(bool enabled);
    bool multiAudioOnly() const noexcept { return m_uiFilters.multiAudioOnly; }
    void setMultiAudioOnly(bool enabled);
    bool anyUiFilterActive() const noexcept;

    void clearUiFilters();
    void setRawStreams(QList<domain::Stream> streams);
    void clearRawStreams();
    void rebuildVisibleStreams();

Q_SIGNALS:
    void sortChanged();
    void rawStreamsCountChanged();
    void uiFiltersChanged();

private:
    config::FilterSettings& m_filters;
    StreamsListModel* m_model;
    QList<domain::Stream> m_rawStreams;
    StreamsListModel::SortMode m_sortMode = StreamsListModel::SortMode::Smart;
    bool m_sortDescending = true;

    struct UiFilterState
    {
        QString resolution;
        bool hdrOnly = false;
        bool dolbyVisionOnly = false;
        bool multiAudioOnly = false;
    } m_uiFilters;
};

} // namespace kinema::ui::qml

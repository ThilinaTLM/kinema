// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/streams/StreamListState.h"

#include "config/FilterSettings.h"
#include "core/util/StreamFilter.h"
#include "ui/qml-bridge/streams/StreamSorting.h"

#include <KLocalizedString>

namespace kinema::ui::qml {

StreamListState::StreamListState(config::FilterSettings& filters,
                                 domain::MediaKind mediaKind,
                                 QObject* parent)
    : QObject(parent), m_filters(filters), m_model(new StreamsListModel(this))
{
    m_model->setMediaKind(mediaKind);
    connect(&m_filters,
            &config::FilterSettings::keywordBlocklistChanged,
            this,
            [this](const QStringList&) { rebuildVisibleStreams(); });
    connect(&m_filters,
            &config::FilterSettings::exclusionsChanged,
            this,
            &StreamListState::rebuildVisibleStreams);
}

void StreamListState::setSortMode(int mode)
{
    const auto value = static_cast<StreamsListModel::SortMode>(mode);
    if (m_sortMode == value)
        return;
    m_sortMode = value;
    Q_EMIT sortChanged();
    rebuildVisibleStreams();
}

void StreamListState::setSortDescending(bool descending)
{
    if (m_sortDescending == descending)
        return;
    m_sortDescending = descending;
    Q_EMIT sortChanged();
    rebuildVisibleStreams();
}

void StreamListState::setResolutionFilter(const QString& resolution)
{
    if (m_uiFilters.resolution == resolution)
        return;
    m_uiFilters.resolution = resolution;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void StreamListState::setHdrOnly(bool enabled)
{
    if (m_uiFilters.hdrOnly == enabled)
        return;
    m_uiFilters.hdrOnly = enabled;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void StreamListState::setDolbyVisionOnly(bool enabled)
{
    if (m_uiFilters.dolbyVisionOnly == enabled)
        return;
    m_uiFilters.dolbyVisionOnly = enabled;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void StreamListState::setMultiAudioOnly(bool enabled)
{
    if (m_uiFilters.multiAudioOnly == enabled)
        return;
    m_uiFilters.multiAudioOnly = enabled;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

bool StreamListState::anyUiFilterActive() const noexcept
{
    return !m_uiFilters.resolution.isEmpty() || m_uiFilters.hdrOnly || m_uiFilters.dolbyVisionOnly
           || m_uiFilters.multiAudioOnly;
}

void StreamListState::clearUiFilters()
{
    if (!anyUiFilterActive())
        return;
    m_uiFilters = {};
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void StreamListState::setRawStreams(QList<domain::Stream> streams)
{
    m_rawStreams = std::move(streams);
    Q_EMIT rawStreamsCountChanged();
    rebuildVisibleStreams();
}

void StreamListState::clearRawStreams()
{
    if (m_rawStreams.isEmpty())
        return;
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();
}

void StreamListState::rebuildVisibleStreams()
{
    // Preserve Loading/Error/Unreleased/Idle placeholders while no result
    // rows are available.
    if (m_rawStreams.isEmpty())
        return;

    core::stream_filter::ClientFilters filters;
    filters.keywordBlocklist = m_filters.keywordBlocklist();
    filters.excludedResolutions = m_filters.excludedResolutions();
    filters.excludedCategories = m_filters.excludedCategories();
    auto visible = core::stream_filter::apply(m_rawStreams, filters);
    visible = stream_sorting::applyUiFilters(std::move(visible),
                                             {m_uiFilters.resolution,
                                              m_uiFilters.hdrOnly,
                                              m_uiFilters.dolbyVisionOnly,
                                              m_uiFilters.multiAudioOnly});
    stream_sorting::sortInPlace(visible, m_sortMode, m_sortDescending);

    QString explanation;
    if (visible.isEmpty()) {
        explanation =
            i18nc("@info streams empty", "Loosen the exclusions or keyword blocklist in Settings.");
    }
    m_model->setItems(std::move(visible), explanation);
}

} // namespace kinema::ui::qml

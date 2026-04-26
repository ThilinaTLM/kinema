// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/SubtitlesDialog.h"

#include "config/SubtitleSettings.h"
#include "controllers/SubtitleController.h"
#include "ui/StateWidget.h"
#include "ui/widgets/LanguagePickerButton.h"
#include "ui/widgets/SubtitleResultsModel.h"

#include <KCollapsibleGroupBox>
#include <KLocalizedString>

#include <QButtonGroup>
#include <QDialogButtonBox>
#include <QFileDialog>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QIcon>
#include <QItemSelectionModel>
#include <QLabel>
#include <QLineEdit>
#include <QPalette>
#include <QPushButton>
#include <QRadioButton>
#include <QSortFilterProxyModel>
#include <QStackedWidget>
#include <QTreeView>
#include <QVBoxLayout>

namespace kinema::ui::widgets {

namespace {

QString formatTitle(const api::PlaybackContext& ctx)
{
    QString base = ctx.title.isEmpty()
        ? i18nc("@title placeholder when media has no display title",
              "Selected media")
        : ctx.title;
    if (ctx.key.kind == api::MediaKind::Series
        && ctx.key.season.has_value()
        && ctx.key.episode.has_value()) {
        const QString tag = QStringLiteral("S%1E%2")
            .arg(*ctx.key.season, 2, 10, QLatin1Char('0'))
            .arg(*ctx.key.episode, 2, 10, QLatin1Char('0'));
        if (!ctx.episodeTitle.isEmpty()) {
            return i18nc("@info dialog header for an episode. "
                         "%1 series, %2 season+episode tag, %3 episode title",
                "%1 — %2 · %3",
                ctx.seriesTitle.isEmpty() ? base : ctx.seriesTitle,
                tag, ctx.episodeTitle);
        }
        return i18nc("@info dialog header for an episode without a title",
            "%1 — %2",
            ctx.seriesTitle.isEmpty() ? base : ctx.seriesTitle, tag);
    }
    return base;
}

QRadioButton* makeModeRadio(const QString& label, const QString& value,
    QButtonGroup* group, int id, QWidget* parent)
{
    auto* rb = new QRadioButton(label, parent);
    rb->setProperty("modeValue", value);
    group->addButton(rb, id);
    return rb;
}

} // namespace

SubtitlesDialog::SubtitlesDialog(controllers::SubtitleController* controller,
    config::SubtitleSettings& settings,
    QWidget* parent)
    : QDialog(parent)
    , m_controller(controller)
    , m_settings(settings)
{
    setWindowTitle(i18nc("@title:window", "Subtitles"));
    setModal(true);
    setMinimumSize(720, 560);
    buildUi();
}

void SubtitlesDialog::buildUi()
{
    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(16, 12, 16, 12);
    root->setSpacing(10);

    // ---- Filter chrome ---------------------------------------------------
    // Layout: one always-visible row with the most-used filters,
    // plus a KCollapsibleGroupBox for the niche per-search overrides.
    // The release-name filter is the dialog's primary input so it
    // stretches; the language picker and action buttons sit beside
    // it as compact tool buttons.
    auto* filterRow = new QHBoxLayout;
    filterRow->setContentsMargins(0, 0, 0, 0);
    filterRow->setSpacing(8);

    m_releaseEdit = new QLineEdit(this);
    m_releaseEdit->setPlaceholderText(i18nc("@info:placeholder",
        "Filter releases — e.g. 1080p, BluRay, REMUX"));
    m_releaseEdit->setClearButtonEnabled(true);
    filterRow->addWidget(m_releaseEdit, 1);

    m_languagePicker = new LanguagePickerButton(this);
    filterRow->addWidget(m_languagePicker);

    m_resetButton = new QPushButton(
        i18nc("@action:button reset subtitle filters", "Reset"), this);
    m_searchButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("edit-find")),
        i18nc("@action:button", "Search"), this);
    filterRow->addWidget(m_resetButton);
    filterRow->addWidget(m_searchButton);

    root->addLayout(filterRow);

    // ---- Advanced filters (collapsible) ---------------------------------
    // Hearing-impaired and foreign-parts-only are niche per-search
    // overrides; defaults already live in SubtitleSettings. Hide
    // them behind a disclosure so the dialog isn't dominated by
    // controls most users never touch.
    auto modeRow = [&](QButtonGroup* group, QWidget* parent) {
        auto* host = new QWidget(parent);
        auto* row = new QHBoxLayout(host);
        row->setContentsMargins(0, 0, 0, 0);
        row->setSpacing(8);
        row->addWidget(makeModeRadio(i18nc("@option:radio subtitle filter mode",
                                        "Off"),
            QStringLiteral("off"), group, 0, host));
        row->addWidget(makeModeRadio(i18nc("@option:radio subtitle filter mode",
                                        "Include"),
            QStringLiteral("include"), group, 1, host));
        row->addWidget(makeModeRadio(i18nc("@option:radio subtitle filter mode",
                                        "Only"),
            QStringLiteral("only"), group, 2, host));
        row->addStretch(1);
        return host;
    };

    m_advancedBox = new KCollapsibleGroupBox(this);
    m_advancedBox->setTitle(i18nc("@title:group subtitle dialog "
                                  "secondary filters",
        "Advanced filters"));

    auto* adv = new QFormLayout;
    adv->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);
    adv->setContentsMargins(0, 4, 0, 0);

    m_hiGroup = new QButtonGroup(this);
    adv->addRow(i18nc("@label radio group", "Hearing impaired:"),
        modeRow(m_hiGroup, m_advancedBox));

    m_fpoGroup = new QButtonGroup(this);
    adv->addRow(i18nc("@label radio group", "Foreign parts only:"),
        modeRow(m_fpoGroup, m_advancedBox));

    m_advancedBox->setLayout(adv);
    root->addWidget(m_advancedBox);

    // ---- Results stack ----------------------------------------------------
    m_resultsStack = new QStackedWidget(this);
    m_state = new ui::StateWidget(m_resultsStack);
    m_model = new SubtitleResultsModel(this);
    m_proxy = new QSortFilterProxyModel(this);
    m_proxy->setSourceModel(m_model);
    m_proxy->setSortRole(Qt::EditRole);
    m_proxy->setDynamicSortFilter(false);

    m_view = new QTreeView(m_resultsStack);
    m_view->setModel(m_proxy);
    m_view->setRootIsDecorated(false);
    m_view->setAllColumnsShowFocus(true);
    m_view->setUniformRowHeights(true);
    m_view->setAlternatingRowColors(true);
    m_view->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_view->setSortingEnabled(true);
    m_view->sortByColumn(SubtitleResultsModel::DownloadsColumn,
        Qt::DescendingOrder);
    auto* header = m_view->header();
    header->setSectionResizeMode(SubtitleResultsModel::ReleaseColumn,
        QHeaderView::Stretch);
    header->setSectionResizeMode(SubtitleResultsModel::LangColumn,
        QHeaderView::ResizeToContents);
    header->setSectionResizeMode(SubtitleResultsModel::FormatColumn,
        QHeaderView::ResizeToContents);
    header->setSectionResizeMode(SubtitleResultsModel::DownloadsColumn,
        QHeaderView::ResizeToContents);
    header->setSectionResizeMode(SubtitleResultsModel::RatingColumn,
        QHeaderView::ResizeToContents);
    header->setSectionResizeMode(SubtitleResultsModel::FlagsColumn,
        QHeaderView::ResizeToContents);

    m_resultsStack->addWidget(m_state);
    m_resultsStack->addWidget(m_view);

    root->addWidget(m_resultsStack, 1);

    // ---- Footer -----------------------------------------------------------
    m_statusLabel = new QLabel(this);
    {
        QPalette p = m_statusLabel->palette();
        p.setColor(QPalette::WindowText,
            palette().color(QPalette::Disabled, QPalette::WindowText));
        m_statusLabel->setPalette(p);
    }

    m_buttonBox = new QDialogButtonBox(this);
    m_openFileButton = m_buttonBox->addButton(
        i18nc("@action:button", "Open file…"),
        QDialogButtonBox::ActionRole);
    m_openFileButton->setIcon(
        QIcon::fromTheme(QStringLiteral("document-open")));

    m_primaryButton = m_buttonBox->addButton(
        i18nc("@action:button subtitle row primary action default label",
            "Download"),
        QDialogButtonBox::AcceptRole);
    m_primaryButton->setIcon(
        QIcon::fromTheme(QStringLiteral("download")));
    m_primaryButton->setDefault(true);
    m_primaryButton->setEnabled(false);

    m_buttonBox->addButton(QDialogButtonBox::Close);

    auto* footer = new QHBoxLayout;
    footer->addWidget(m_statusLabel, 1);
    footer->addWidget(m_buttonBox);
    root->addLayout(footer);

    // ---- Wires ------------------------------------------------------------
    connect(m_searchButton, &QPushButton::clicked, this,
        &SubtitlesDialog::runSearch);
    connect(m_releaseEdit, &QLineEdit::returnPressed, this,
        &SubtitlesDialog::runSearch);
    connect(m_resetButton, &QPushButton::clicked, this, [this] {
        m_languagePicker->setLanguages(m_settings.preferredLanguages());
        setHi(m_settings.hearingImpaired());
        setFpo(m_settings.foreignPartsOnly());
        m_releaseEdit->clear();
        syncAdvancedExpansion();
    });
    connect(m_openFileButton, &QPushButton::clicked, this,
        &SubtitlesDialog::onLocalFile);

    // QDialogButtonBox accept = primary action. We override the
    // standard QDialog::accept() flow so the dialog stays open
    // after a download — the user typically wants to inspect status
    // or pick a different language.
    connect(m_buttonBox, &QDialogButtonBox::accepted, this,
        &SubtitlesDialog::runPrimaryAction);
    connect(m_buttonBox, &QDialogButtonBox::rejected, this,
        &QDialog::reject);

    connect(m_view, &QAbstractItemView::activated, this,
        [this](const QModelIndex&) {
            if (m_primaryButton && m_primaryButton->isEnabled()) {
                m_primaryButton->animateClick();
            }
        });

    connect(m_state, &ui::StateWidget::retryRequested,
        this, &SubtitlesDialog::runSearch);
}

void SubtitlesDialog::setMedia(const api::PlaybackContext& ctx)
{
    m_context = ctx;
    setWindowTitle(i18nc("@title:window subtitles dialog with media name",
        "Subtitles — %1", formatTitle(ctx)));
    // Reset result set so the previous media doesn't show up briefly
    // when the dialog is reopened.
    m_model->setHits({});
    m_state->showIdle(QString {});
    m_resultsStack->setCurrentWidget(m_state);
    m_languagePicker->setLanguages(m_settings.preferredLanguages());
    setHi(m_settings.hearingImpaired());
    setFpo(m_settings.foreignPartsOnly());
    m_releaseEdit->clear();
    syncAdvancedExpansion();
    refreshGate();
    refreshStatusLine();
    updatePrimaryAction();
}

void SubtitlesDialog::setAttachOnDownload(bool on)
{
    m_attachOnDownload = on;
}

void SubtitlesDialog::showEvent(QShowEvent* event)
{
    QDialog::showEvent(event);
    wireController();
    refreshGate();
    refreshState();
    refreshStatusLine();
    updatePrimaryAction();
    // Auto-search the moment we open against a configured account.
    if (m_controller && m_controller->downloadEnabled()
        && m_context.key.isValid() && m_model->count() == 0) {
        runSearch();
    }
}

void SubtitlesDialog::hideEvent(QHideEvent* event)
{
    unwireController();
    QDialog::hideEvent(event);
}

void SubtitlesDialog::wireController()
{
    if (!m_controller) {
        return;
    }
    connect(m_controller, &controllers::SubtitleController::hitsChanged,
        this, &SubtitlesDialog::refreshState, Qt::UniqueConnection);
    connect(m_controller, &controllers::SubtitleController::cacheChanged,
        this, &SubtitlesDialog::refreshState, Qt::UniqueConnection);
    connect(m_controller, &controllers::SubtitleController::activeSubtitleChanged,
        this, &SubtitlesDialog::refreshState, Qt::UniqueConnection);
    connect(m_controller, &controllers::SubtitleController::searchingChanged,
        this, &SubtitlesDialog::refreshState, Qt::UniqueConnection);
    connect(m_controller, &controllers::SubtitleController::errorChanged,
        this, &SubtitlesDialog::refreshState, Qt::UniqueConnection);
    connect(m_controller, &controllers::SubtitleController::downloadEnabledChanged,
        this, &SubtitlesDialog::refreshGate, Qt::UniqueConnection);
    connect(m_controller, &controllers::SubtitleController::quotaChanged,
        this, &SubtitlesDialog::refreshStatusLine, Qt::UniqueConnection);
    // Note: Qt::UniqueConnection is incompatible with lambda slots; rely on
    // unwireController()'s blanket disconnect() to keep these from stacking.
    connect(m_controller, &controllers::SubtitleController::downloadFinished,
        this, [this](const QString& fileId, const QString& localPath,
                  const QString& language, const QString& languageName) {
            Q_EMIT downloadCompleted(m_context.key, fileId, localPath,
                language, languageName);
            refreshStatusLine();
            updatePrimaryAction();
        });
    connect(m_controller, &controllers::SubtitleController::downloadFailed,
        this, [this](const QString&, const QString&) {
            refreshStatusLine();
            updatePrimaryAction();
        });
}

void SubtitlesDialog::unwireController()
{
    if (!m_controller) {
        return;
    }
    disconnect(m_controller, nullptr, this, nullptr);
}

void SubtitlesDialog::runSearch()
{
    if (!m_controller || !m_context.key.isValid()) {
        return;
    }
    m_controller->runQuery(m_context.key, m_languagePicker->languages(),
        currentHi(), currentFpo(), m_releaseEdit->text().trimmed());
}

void SubtitlesDialog::runPrimaryAction()
{
    if (!m_controller) {
        return;
    }
    const auto src = currentSourceIndex();
    if (!src.isValid()) {
        return;
    }
    const auto fileId = src.data(SubtitleResultsModel::FileIdRole).toString();
    if (fileId.isEmpty()) {
        return;
    }
    m_controller->download(fileId, m_context.key);
}

void SubtitlesDialog::onLocalFile()
{
    const auto path = QFileDialog::getOpenFileName(this,
        i18nc("@title:window", "Open subtitle file"),
        QString {},
        i18nc("@info file dialog filter",
            "Subtitles (*.srt *.vtt *.ass *.ssa *.sub)"));
    if (path.isEmpty()) {
        return;
    }
    Q_EMIT localFileChosen(m_context.key, path);
}

void SubtitlesDialog::refreshState()
{
    if (!m_controller) {
        m_state->showIdle(i18nc("@info", "Subtitle controller unavailable."));
        m_resultsStack->setCurrentWidget(m_state);
        updatePrimaryAction();
        return;
    }

    if (!m_controller->downloadEnabled()) {
        m_state->showIdle(
            i18nc("@info", "OpenSubtitles isn't configured yet."),
            i18nc("@info",
                "Add an API key and credentials in Settings to search "
                "and download subtitles."));
        m_resultsStack->setCurrentWidget(m_state);
        updatePrimaryAction();
        return;
    }

    if (m_controller->isSearching()) {
        m_state->showLoading(
            i18nc("@info:status", "Searching OpenSubtitles…"));
        m_resultsStack->setCurrentWidget(m_state);
        updatePrimaryAction();
        return;
    }

    const auto err = m_controller->lastError();
    if (!err.isEmpty()) {
        m_state->showError(err, /*retryable=*/true);
        m_resultsStack->setCurrentWidget(m_state);
        updatePrimaryAction();
        return;
    }

    m_model->setHits(m_controller->hits());
    m_model->setCachedFileIds(m_controller->cachedFileIds());
    // The controller's `activeLocalPaths()` is keyed by local path;
    // the model wants fileIds. The player path bumps activeFileIds
    // via `setActiveFileIds` directly when needed (future work).
    m_model->setActiveFileIds({});

    if (m_controller->hits().isEmpty()) {
        m_state->showIdle(
            i18nc("@info", "No subtitles found"),
            i18nc("@info",
                "Try widening the languages or removing the release filter."));
        m_resultsStack->setCurrentWidget(m_state);
    } else {
        m_resultsStack->setCurrentWidget(m_view);
        // Re-attach selection handler each time the model resets so
        // the primary button reflects the new selection model.
        if (auto* sm = m_view->selectionModel()) {
            connect(sm, &QItemSelectionModel::currentChanged, this,
                [this](const QModelIndex&, const QModelIndex&) {
                    updatePrimaryAction();
                }, Qt::UniqueConnection);
        }
    }
    refreshStatusLine();
    updatePrimaryAction();
}

void SubtitlesDialog::refreshGate()
{
    const bool enabled = m_controller && m_controller->downloadEnabled()
        && m_context.key.isValid();
    m_searchButton->setEnabled(enabled);
    m_resetButton->setEnabled(enabled);
    m_languagePicker->setEnabled(enabled);
    m_releaseEdit->setEnabled(enabled);
    if (m_hiGroup) {
        for (auto* b : m_hiGroup->buttons()) b->setEnabled(enabled);
    }
    if (m_fpoGroup) {
        for (auto* b : m_fpoGroup->buttons()) b->setEnabled(enabled);
    }
    if (m_advancedBox) {
        m_advancedBox->setEnabled(enabled);
    }

    // Add / remove the "Open settings…" button on the button box
    // depending on whether the controller is configured. Lazy: we
    // only ever construct it once, then toggle visibility.
    const bool needsSettings = m_controller && !m_controller->downloadEnabled();
    if (needsSettings && !m_settingsButton) {
        m_settingsButton = m_buttonBox->addButton(
            i18nc("@action:button open settings dialog from subtitles",
                "Open settings…"),
            QDialogButtonBox::HelpRole);
        m_settingsButton->setIcon(
            QIcon::fromTheme(QStringLiteral("configure")));
        connect(m_settingsButton, &QPushButton::clicked, this,
            [this] { Q_EMIT settingsRequested(); });
    }
    if (m_settingsButton) {
        m_settingsButton->setVisible(needsSettings);
    }
}

void SubtitlesDialog::refreshStatusLine()
{
    if (!m_controller) {
        m_statusLabel->clear();
        return;
    }
    QStringList parts;
    if (m_controller->isSearching()) {
        parts << i18nc("@info:status", "Searching…");
    } else {
        const int n = m_controller->hits().size();
        parts << i18ncp("@info:status result count",
            "%1 result", "%1 results", n);
    }
    const int remaining = m_controller->dailyDownloadsRemaining();
    if (remaining >= 0) {
        parts << i18nc("@info:status OpenSubtitles daily download quota",
            "Daily quota: %1 remaining", remaining);
    }
    m_statusLabel->setText(parts.join(QStringLiteral("  ·  ")));
}

void SubtitlesDialog::updatePrimaryAction()
{
    if (!m_primaryButton) {
        return;
    }
    const auto src = currentSourceIndex();
    const bool gate = m_controller && m_controller->downloadEnabled()
        && m_context.key.isValid();
    if (!src.isValid() || !gate) {
        m_primaryButton->setText(
            i18nc("@action:button subtitle row primary action default label",
                "Download"));
        m_primaryButton->setIcon(
            QIcon::fromTheme(QStringLiteral("download")));
        m_primaryButton->setEnabled(false);
        return;
    }

    const bool active = src.data(SubtitleResultsModel::ActiveRole).toBool();
    const bool cached = src.data(SubtitleResultsModel::CachedRole).toBool();
    if (active) {
        m_primaryButton->setText(
            i18nc("@action:button subtitle row action when already loaded "
                  "in the player",
                "Re-attach"));
        m_primaryButton->setIcon(
            QIcon::fromTheme(QStringLiteral("view-refresh")));
    } else if (cached) {
        m_primaryButton->setText(
            i18nc("@action:button subtitle row action for cached entry",
                "Use"));
        m_primaryButton->setIcon(
            QIcon::fromTheme(QStringLiteral("dialog-ok-apply")));
    } else {
        m_primaryButton->setText(
            i18nc("@action:button subtitle row action for fresh download",
                "Download"));
        m_primaryButton->setIcon(
            QIcon::fromTheme(QStringLiteral("download")));
    }
    m_primaryButton->setEnabled(true);
}

QModelIndex SubtitlesDialog::currentSourceIndex() const
{
    if (!m_view || !m_proxy) {
        return {};
    }
    const auto current = m_view->currentIndex();
    if (!current.isValid()) {
        return {};
    }
    const auto src = m_proxy->mapToSource(current);
    if (!src.isValid()) {
        return {};
    }
    return m_model->index(src.row(), SubtitleResultsModel::ReleaseColumn);
}

QString SubtitlesDialog::currentHi() const
{
    auto* checked = m_hiGroup ? m_hiGroup->checkedButton() : nullptr;
    return checked ? checked->property("modeValue").toString()
                   : QStringLiteral("off");
}

QString SubtitlesDialog::currentFpo() const
{
    auto* checked = m_fpoGroup ? m_fpoGroup->checkedButton() : nullptr;
    return checked ? checked->property("modeValue").toString()
                   : QStringLiteral("off");
}

void SubtitlesDialog::syncAdvancedExpansion()
{
    if (!m_advancedBox) {
        return;
    }
    const bool nonDefault = currentHi() != QStringLiteral("off")
        || currentFpo() != QStringLiteral("off");
    if (nonDefault && !m_advancedBox->isExpanded()) {
        m_advancedBox->setExpanded(true);
    }
}

void SubtitlesDialog::setHi(const QString& mode)
{
    if (!m_hiGroup) return;
    for (auto* b : m_hiGroup->buttons()) {
        if (b->property("modeValue").toString() == mode) {
            b->setChecked(true);
            return;
        }
    }
    if (auto* b = m_hiGroup->button(0)) b->setChecked(true);
}

void SubtitlesDialog::setFpo(const QString& mode)
{
    if (!m_fpoGroup) return;
    for (auto* b : m_fpoGroup->buttons()) {
        if (b->property("modeValue").toString() == mode) {
            b->setChecked(true);
            return;
        }
    }
    if (auto* b = m_fpoGroup->button(0)) b->setChecked(true);
}

} // namespace kinema::ui::widgets

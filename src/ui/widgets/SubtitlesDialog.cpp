// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/SubtitlesDialog.h"

#include "config/SubtitleSettings.h"
#include "controllers/SubtitleController.h"
#include "ui/StateWidget.h"
#include "ui/widgets/LanguageChipBar.h"
#include "ui/widgets/SubtitleResultDelegate.h"
#include "ui/widgets/SubtitleResultsModel.h"

#include <KLocalizedString>

#include <QButtonGroup>
#include <QCursor>
#include <QDialogButtonBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QFrame>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QRadioButton>
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

    // ---- Title strip ------------------------------------------------------
    auto* header = new QLabel(this);
    header->setText(i18nc("@info dialog header label", "Subtitles for"));
    auto headerFont = header->font();
    headerFont.setPointSizeF(headerFont.pointSizeF() * 0.95);
    auto headerColour = palette().color(QPalette::Disabled, QPalette::WindowText);
    {
        QPalette p = header->palette();
        p.setColor(QPalette::WindowText, headerColour);
        header->setPalette(p);
    }
    root->addWidget(header);

    m_titleLabel = new QLabel(this);
    auto titleFont = m_titleLabel->font();
    titleFont.setPointSizeF(titleFont.pointSizeF() + 3.0);
    titleFont.setBold(true);
    m_titleLabel->setFont(titleFont);
    m_titleLabel->setWordWrap(true);
    root->addWidget(m_titleLabel);

    // ---- Filter card ------------------------------------------------------
    auto* filterFrame = new QFrame(this);
    filterFrame->setObjectName(QStringLiteral("FilterCard"));
    filterFrame->setFrameShape(QFrame::StyledPanel);
    filterFrame->setStyleSheet(QStringLiteral(
        "QFrame#FilterCard { border-radius: 8px;"
        " background: palette(alternate-base); padding: 12px; }"));
    auto* filterLayout = new QVBoxLayout(filterFrame);
    filterLayout->setContentsMargins(12, 10, 12, 10);
    filterLayout->setSpacing(8);

    {
        auto* row = new QHBoxLayout;
        row->addWidget(new QLabel(i18nc("@label", "Languages:"), filterFrame));
        m_chipBar = new LanguageChipBar(filterFrame);
        row->addWidget(m_chipBar, 1);
        filterLayout->addLayout(row);
    }

    auto modeRow = [&](const QString& label, QButtonGroup* group) {
        auto* row = new QHBoxLayout;
        row->addWidget(new QLabel(label, filterFrame));
        row->addSpacing(8);
        row->addWidget(makeModeRadio(i18nc("@option:radio subtitle filter mode",
                                        "Off"),
            QStringLiteral("off"), group, 0, filterFrame));
        row->addWidget(makeModeRadio(i18nc("@option:radio subtitle filter mode",
                                        "Include"),
            QStringLiteral("include"), group, 1, filterFrame));
        row->addWidget(makeModeRadio(i18nc("@option:radio subtitle filter mode",
                                        "Only"),
            QStringLiteral("only"), group, 2, filterFrame));
        row->addStretch(1);
        filterLayout->addLayout(row);
    };

    m_hiGroup = new QButtonGroup(this);
    modeRow(i18nc("@label radio group", "Hearing impaired:"), m_hiGroup);

    m_fpoGroup = new QButtonGroup(this);
    modeRow(i18nc("@label radio group", "Foreign parts only:"), m_fpoGroup);

    {
        auto* row = new QHBoxLayout;
        row->addWidget(new QLabel(i18nc("@label", "Release filter:"),
            filterFrame));
        m_releaseEdit = new QLineEdit(filterFrame);
        m_releaseEdit->setPlaceholderText(i18nc("@info:placeholder",
            "e.g. 1080p, BluRay, REMUX"));
        row->addWidget(m_releaseEdit, 1);
        filterLayout->addLayout(row);
    }

    {
        auto* row = new QHBoxLayout;
        row->addStretch(1);
        m_resetButton = new QPushButton(
            i18nc("@action:button reset subtitle filters", "Reset"),
            filterFrame);
        m_searchButton = new QPushButton(
            QIcon::fromTheme(QStringLiteral("edit-find")),
            i18nc("@action:button", "Search"), filterFrame);
        m_searchButton->setDefault(true);
        row->addWidget(m_resetButton);
        row->addWidget(m_searchButton);
        filterLayout->addLayout(row);
    }

    root->addWidget(filterFrame);

    // ---- Results stack ----------------------------------------------------
    m_resultsStack = new QStackedWidget(this);
    m_state = new ui::StateWidget(m_resultsStack);
    m_model = new SubtitleResultsModel(this);
    m_delegate = new SubtitleResultDelegate(this);

    m_view = new QTreeView(m_resultsStack);
    m_view->setModel(m_model);
    m_view->setItemDelegate(m_delegate);
    m_view->setHeaderHidden(true);
    m_view->setRootIsDecorated(false);
    m_view->setAllColumnsShowFocus(true);
    m_view->setUniformRowHeights(true);
    m_view->setMouseTracking(true);
    m_view->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_view->setSelectionMode(QAbstractItemView::SingleSelection);
    m_view->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_view->header()->setSectionResizeMode(QHeaderView::Stretch);

    m_resultsStack->addWidget(m_state);
    m_resultsStack->addWidget(m_view);

    root->addWidget(m_resultsStack, 1);

    // ---- Footer -----------------------------------------------------------
    auto* footer = new QHBoxLayout;
    m_statusLabel = new QLabel(this);
    {
        QPalette p = m_statusLabel->palette();
        p.setColor(QPalette::WindowText,
            palette().color(QPalette::Disabled, QPalette::WindowText));
        m_statusLabel->setPalette(p);
    }
    footer->addWidget(m_statusLabel, 1);

    m_openFileButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("document-open")),
        i18nc("@action:button", "Open file…"), this);
    m_closeButton = new QPushButton(
        i18nc("@action:button", "Close"), this);
    footer->addWidget(m_openFileButton);
    footer->addWidget(m_closeButton);
    root->addLayout(footer);

    // ---- Wires ------------------------------------------------------------
    connect(m_searchButton, &QPushButton::clicked, this,
        &SubtitlesDialog::runSearch);
    connect(m_releaseEdit, &QLineEdit::returnPressed, this,
        &SubtitlesDialog::runSearch);
    connect(m_resetButton, &QPushButton::clicked, this, [this] {
        m_chipBar->setLanguages(m_settings.preferredLanguages());
        setHi(m_settings.hearingImpaired());
        setFpo(m_settings.foreignPartsOnly());
        m_releaseEdit->clear();
    });
    connect(m_openFileButton, &QPushButton::clicked, this,
        &SubtitlesDialog::onLocalFile);
    connect(m_closeButton, &QPushButton::clicked, this, &QDialog::accept);

    connect(m_view, &QAbstractItemView::activated, this,
        &SubtitlesDialog::onRowActivated);
    connect(m_view, &QAbstractItemView::clicked, this,
        &SubtitlesDialog::onViewClicked);

    connect(m_state, &ui::StateWidget::retryRequested,
        this, &SubtitlesDialog::runSearch);
}

void SubtitlesDialog::setMedia(const api::PlaybackContext& ctx)
{
    m_context = ctx;
    m_titleLabel->setText(formatTitle(ctx));
    // Reset result set so the previous media doesn't show up briefly
    // when the dialog is reopened.
    m_model->setHits({});
    m_state->showIdle(QString {});
    m_resultsStack->setCurrentWidget(m_state);
    m_chipBar->setLanguages(m_settings.preferredLanguages());
    setHi(m_settings.hearingImpaired());
    setFpo(m_settings.foreignPartsOnly());
    m_releaseEdit->clear();
    refreshGate();
    refreshStatusLine();
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
    connect(m_controller, &controllers::SubtitleController::downloadFinished,
        this, [this](const QString& fileId, const QString& localPath,
                  const QString& language, const QString& languageName) {
            Q_EMIT downloadCompleted(m_context.key, fileId, localPath,
                language, languageName);
            refreshStatusLine();
        }, Qt::UniqueConnection);
    connect(m_controller, &controllers::SubtitleController::downloadFailed,
        this, [this](const QString&, const QString&) { refreshStatusLine(); },
        Qt::UniqueConnection);
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
    m_controller->runQuery(m_context.key, m_chipBar->languages(),
        currentHi(), currentFpo(), m_releaseEdit->text().trimmed());
}

void SubtitlesDialog::onRowActivated(const QModelIndex& index)
{
    if (!index.isValid() || !m_controller) {
        return;
    }
    const auto fileId = index.data(SubtitleResultsModel::FileIdRole).toString();
    if (!fileId.isEmpty()) {
        m_controller->download(fileId, m_context.key);
    }
}

void SubtitlesDialog::onViewClicked(const QModelIndex& index)
{
    if (!index.isValid()) {
        return;
    }
    const QRect rect = m_view->visualRect(index);
    QStyleOptionViewItem opt;
    opt.rect = rect;
    opt.font = m_view->font();
    opt.widget = m_view;
    const QRect actionRect = m_delegate->actionRectFor(opt, index);
    const QPoint cursor = m_view->viewport()->mapFromGlobal(QCursor::pos());
    if (actionRect.contains(cursor)) {
        onRowActivated(index);
    }
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
        return;
    }

    if (!m_controller->downloadEnabled()) {
        m_state->showIdle(
            i18nc("@info", "OpenSubtitles isn't configured yet."),
            i18nc("@info",
                "Add an API key and credentials in Settings to search "
                "and download subtitles."));
        m_resultsStack->setCurrentWidget(m_state);
        return;
    }

    if (m_controller->isSearching()) {
        m_state->showLoading(
            i18nc("@info:status", "Searching OpenSubtitles…"));
        m_resultsStack->setCurrentWidget(m_state);
        return;
    }

    const auto err = m_controller->lastError();
    if (!err.isEmpty()) {
        m_state->showError(err, /*retryable=*/true);
        m_resultsStack->setCurrentWidget(m_state);
        return;
    }

    m_model->setHits(m_controller->hits());
    // Translate active local-paths to active fileIds for the model
    // by matching against the cache view of the controller.
    QSet<QString> activeFileIds;
    const auto cached = m_controller->cachedFileIds();
    const auto activePaths = m_controller->activeLocalPaths();
    if (!activePaths.isEmpty()) {
        for (const auto& hit : m_controller->hits()) {
            // We don't have a direct fileId→localPath in the
            // controller's public API; the row's "active" badge is
            // driven by `cached + activePaths` overlap which the
            // model already approximates via cachedFileIds. Keep
            // empty here; the player path bumps activeFileIds via
            // `setActiveFileIds` directly when needed (future work).
            Q_UNUSED(hit);
        }
    }
    m_model->setCachedFileIds(cached);
    m_model->setActiveFileIds(activeFileIds);

    if (m_controller->hits().isEmpty()) {
        m_state->showIdle(
            i18nc("@info", "No subtitles found"),
            i18nc("@info",
                "Try widening the languages or removing the release filter."));
        m_resultsStack->setCurrentWidget(m_state);
    } else {
        m_resultsStack->setCurrentWidget(m_view);
    }
    refreshStatusLine();
}

void SubtitlesDialog::refreshGate()
{
    const bool enabled = m_controller && m_controller->downloadEnabled()
        && m_context.key.isValid();
    m_searchButton->setEnabled(enabled);
    m_resetButton->setEnabled(enabled);
    m_chipBar->setEnabled(enabled);
    m_releaseEdit->setEnabled(enabled);
    if (m_hiGroup) {
        for (auto* b : m_hiGroup->buttons()) b->setEnabled(enabled);
    }
    if (m_fpoGroup) {
        for (auto* b : m_fpoGroup->buttons()) b->setEnabled(enabled);
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

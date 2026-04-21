// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QWidget>

class QAction;
class QLineEdit;
class QToolButton;

namespace kinema::ui {

/**
 * Top-of-window search input. Emits `queryRequested(text, kind)` when the
 * user presses Enter or clicks the search button. The kind toggle is wired
 * for movies in M1; the series option is present but disabled with a
 * tooltip announcing M2.
 */
class SearchBar : public QWidget
{
    Q_OBJECT
public:
    explicit SearchBar(QWidget* parent = nullptr);

    void setQuery(const QString& text);
    QString query() const;

    api::MediaKind mediaKind() const { return m_kind; }
    void setMediaKind(api::MediaKind kind);

Q_SIGNALS:
    void queryRequested(const QString& text, api::MediaKind kind);
    void mediaKindChanged(api::MediaKind kind);

private:
    void emitRequest();
    void applyKindToButton();

    QLineEdit* m_edit;
    QToolButton* m_kindToggle;
    QAction* m_movieAction {};
    QAction* m_seriesAction {};
    api::MediaKind m_kind = api::MediaKind::Movie;
};

} // namespace kinema::ui

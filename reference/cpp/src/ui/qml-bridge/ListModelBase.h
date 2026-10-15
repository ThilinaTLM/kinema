// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QAbstractListModel>
#include <QList>
#include <QModelIndex>

namespace kinema::ui::qml {

/**
 * Non-`Q_OBJECT` base for the stateless list models that expose a
 * `QList<Row>` through `QAbstractListModel`. Owns the parent-aware
 * `rowCount()`, the bounds-checked `at()`, and the reset-based
 * replacement used by every row model. Concrete subclasses keep
 * their own `Q_OBJECT`, `data()` (including the row-specific role
 * switch), `roleNames()`, and `countChanged` signal.
 *
 * Because this is a template it cannot carry `Q_OBJECT` or signals
 * (`Q_SIGNALS`); concrete models declare `countChanged` themselves
 * and emit it after calling `replaceRows()`.
 */
template <typename Row>
class ListModelBase : public QAbstractListModel
{
public:
    explicit ListModelBase(QObject* parent = nullptr)
        : QAbstractListModel(parent)
    {
    }

    int rowCount(const QModelIndex& parent = {}) const override
    {
        if (parent.isValid()) {
            return 0;
        }
        return static_cast<int>(m_rows.size());
    }

    /// Bounds-checked row lookup; returns null when `row` is invalid.
    const Row* at(int row) const
    {
        if (row < 0 || row >= m_rows.size()) {
            return nullptr;
        }
        return &m_rows.at(row);
    }

protected:
    /// Reset the model to `rows` (full reset + empty-message
    /// recompute). Derived `setRows()` calls this and then emits its
    /// own `countChanged()`.
    void replaceRows(QList<Row> rows)
    {
        beginResetModel();
        m_rows = std::move(rows);
        endResetModel();
    }

    QList<Row> m_rows;
};

} // namespace kinema::ui::qml

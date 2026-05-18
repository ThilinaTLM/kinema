// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "app/ServiceContainer.h"
#include "config/AppSettings.h"
#include "ui/qml-bridge/ShellViewModel.h"

#include <QClipboard>
#include <QGuiApplication>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTest>
#include <QUrl>

#include <memory>

using kinema::app::ServiceContainer;
using kinema::config::AppSettings;
using kinema::ui::qml::ShellViewModel;

class TestShellViewModelClipboard : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void initTestCase();

    /// Happy path: text lands on the clipboard and the passive
    /// notification fires with the default ("Copied to clipboard")
    /// toast.
    void copyToClipboardDefaultToast();

    /// Caller-supplied confirm message overrides the default toast.
    void copyToClipboardCustomToast();

    /// Empty text is a no-op: no clipboard write, no toast. Guards
    /// against menu items accidentally clearing the clipboard when
    /// a row has no value to copy.
    void copyToClipboardEmptyIsNoop();

    /// Malformed IMDb ids (anything not matching `^tt\\d+$`) are
    /// silently rejected — no URL is opened, no toast fires.
    void openImdbTitleRejectsGarbage();

    /// TMDB id ≤ 0 is silently rejected.
    void openTmdbTitleRejectsZero();
};

void TestShellViewModelClipboard::initTestCase()
{
    QStandardPaths::setTestModeEnabled(true);
}

namespace {

struct ShellFixture {
    std::unique_ptr<AppSettings> settings
        = std::make_unique<AppSettings>();
    std::unique_ptr<ServiceContainer> services;
    std::unique_ptr<ShellViewModel> shell;

    ShellFixture()
    {
        services = std::make_unique<ServiceContainer>(*settings);
        shell = std::make_unique<ShellViewModel>(*services);
    }
};

} // namespace

void TestShellViewModelClipboard::copyToClipboardDefaultToast()
{
    ShellFixture fx;
    QSignalSpy spy(fx.shell.get(), &ShellViewModel::passiveMessage);
    QGuiApplication::clipboard()->clear();

    fx.shell->copyToClipboard(QStringLiteral("hello"));

    QCOMPARE(QGuiApplication::clipboard()->text(),
        QStringLiteral("hello"));
    QCOMPARE(spy.count(), 1);
    const auto args = spy.takeFirst();
    QVERIFY(!args.at(0).toString().isEmpty());
    QCOMPARE(args.at(1).toInt(), 3000);
}

void TestShellViewModelClipboard::copyToClipboardCustomToast()
{
    ShellFixture fx;
    QSignalSpy spy(fx.shell.get(), &ShellViewModel::passiveMessage);

    fx.shell->copyToClipboard(QStringLiteral("Inception"),
        QStringLiteral("Title copied"));

    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.takeFirst().at(0).toString(),
        QStringLiteral("Title copied"));
}

void TestShellViewModelClipboard::copyToClipboardEmptyIsNoop()
{
    ShellFixture fx;
    QSignalSpy spy(fx.shell.get(), &ShellViewModel::passiveMessage);
    QGuiApplication::clipboard()->setText(QStringLiteral("sentinel"));

    fx.shell->copyToClipboard(QString {});

    QCOMPARE(QGuiApplication::clipboard()->text(),
        QStringLiteral("sentinel"));
    QCOMPARE(spy.count(), 0);
}

void TestShellViewModelClipboard::openImdbTitleRejectsGarbage()
{
    ShellFixture fx;
    QSignalSpy spy(fx.shell.get(), &ShellViewModel::passiveMessage);

    // Each of these should be rejected at the regex gate and never
    // reach KIO::OpenUrlJob, so the test stays hermetic on CI.
    fx.shell->openImdbTitle(QString {});
    fx.shell->openImdbTitle(QStringLiteral("not-an-id"));
    fx.shell->openImdbTitle(QStringLiteral("123456"));
    fx.shell->openImdbTitle(QStringLiteral("tt"));

    QCOMPARE(spy.count(), 0);
}

void TestShellViewModelClipboard::openTmdbTitleRejectsZero()
{
    ShellFixture fx;
    QSignalSpy spy(fx.shell.get(), &ShellViewModel::passiveMessage);

    fx.shell->openTmdbTitle(0, 0);
    fx.shell->openTmdbTitle(-1, 0);

    QCOMPARE(spy.count(), 0);
}

QTEST_MAIN(TestShellViewModelClipboard)
#include "tst_shell_view_model_clipboard.moc"

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/io/Logging.h"

#include "kinema_log_app.h"

#include <QByteArray>
#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QLocale>
#include <QMutex>
#include <QMutexLocker>
#include <QOperatingSystemVersion>
#include <QStandardPaths>
#include <QString>
#include <QSysInfo>
#include <QThread>
#include <QtGlobal>
#include <QtMessageHandler>

#include <cstdio>
#include <unistd.h>

namespace kinema::core {

namespace {

// Rotation thresholds: 5 MB per file, keep up to 5 archives. Rotation
// runs once at install time; mid-session rotation would force the
// handler path to take an extra file-stat per write for marginal
// benefit on a desktop log volume.
constexpr qint64 kMaxBytes = 5LL * 1024 * 1024;
constexpr int kMaxArchives = 5;

QMutex& fileMutex()
{
    static QMutex m;
    return m;
}

// Heap-allocated and deliberately leaked. Reentrant logging from
// other modules' static destructors must not be able to chase a
// freed `QFile*` through our handler.
QFile*& logFile()
{
    static QFile* f = nullptr;
    return f;
}

QtMessageHandler& previousHandler()
{
    static QtMessageHandler h = nullptr;
    return h;
}

bool& stderrIsTty()
{
    static bool tty = ::isatty(STDERR_FILENO) != 0;
    return tty;
}

QString severityLabel(QtMsgType type)
{
    switch (type) {
    case QtDebugMsg:    return QStringLiteral("DEBUG");
    case QtInfoMsg:     return QStringLiteral("INFO ");
    case QtWarningMsg:  return QStringLiteral("WARN ");
    case QtCriticalMsg: return QStringLiteral("ERROR");
    case QtFatalMsg:    return QStringLiteral("FATAL");
    }
    return QStringLiteral("?    ");
}

// ANSI colour codes per severity, applied to the severity token
// when stderr is a TTY. File output never carries colour codes.
const char* severityColor(QtMsgType type)
{
    switch (type) {
    case QtDebugMsg:    return "\033[2;37m";   // dim grey
    case QtInfoMsg:     return "\033[1;34m";   // bold blue
    case QtWarningMsg:  return "\033[1;33m";   // bold yellow
    case QtCriticalMsg: return "\033[1;31m";   // bold red
    case QtFatalMsg:    return "\033[1;41;97m"; // white on red
    }
    return "";
}

QString threadTag()
{
    QThread* current = QThread::currentThread();
    QThread* main = QCoreApplication::instance()
        ? QCoreApplication::instance()->thread()
        : nullptr;
    if (current == main) {
        return {};
    }
    QString name = current ? current->objectName() : QString();
    if (name.isEmpty()) {
        // Stable but compact id: low 16 bits of the thread pointer.
        const auto v = reinterpret_cast<quintptr>(current);
        name = QStringLiteral("0x%1").arg(v & 0xFFFF, 4, 16, QLatin1Char('0'));
    }
    return QStringLiteral(" [thrd=") + name + QStringLiteral("]");
}

QString locationTag(QtMsgType type, const QMessageLogContext& ctx)
{
    if (type != QtWarningMsg && type != QtCriticalMsg && type != QtFatalMsg) {
        return {};
    }
    if (ctx.file == nullptr || ctx.line <= 0) {
        return {};
    }
    QString file = QString::fromUtf8(ctx.file);
    // Strip everything before the project's `src/` segment so paths
    // are stable across build hosts.
    const int idx = file.indexOf(QStringLiteral("/src/"));
    if (idx >= 0) {
        file = file.mid(idx + 5);
    } else {
        const int slash = file.lastIndexOf(QLatin1Char('/'));
        if (slash >= 0) {
            file = file.mid(slash + 1);
        }
    }
    QString out = QStringLiteral("  ") + file
        + QLatin1Char(':') + QString::number(ctx.line);
    if (ctx.function != nullptr && *ctx.function != '\0') {
        // Function signatures from `__PRETTY_FUNCTION__` are noisy;
        // keep just the function name segment when it looks like
        // `ret ns::Class::fn(args)`.
        QString fn = QString::fromUtf8(ctx.function);
        const int paren = fn.indexOf(QLatin1Char('('));
        if (paren > 0) {
            fn.truncate(paren);
        }
        const int sp = fn.lastIndexOf(QLatin1Char(' '));
        if (sp >= 0) {
            fn = fn.mid(sp + 1);
        }
        out += QStringLiteral(" in ") + fn;
    }
    return out;
}

QString formatLine(QtMsgType type, const QMessageLogContext& ctx,
    const QString& msg)
{
    const QString ts = QDateTime::currentDateTimeUtc().toString(
        QStringLiteral("yyyy-MM-ddTHH:mm:ss.zzzZ"));
    const QString cat = QString::fromUtf8(
        ctx.category != nullptr ? ctx.category : "default");
    return ts + QStringLiteral("  ") + severityLabel(type)
        + QStringLiteral("  ") + cat
        + threadTag() + locationTag(type, ctx)
        + QStringLiteral("  ") + msg;
}

void writeStderr(QtMsgType type, const QString& line)
{
    QByteArray bytes;
    if (stderrIsTty()) {
        // Wrap just the severity token in colour. The token starts at
        // offset 27 (24-char ts + two spaces + 5-char label after ts).
        // Recomputing the offset each time is robust to format tweaks.
        bytes.reserve(line.size() + 16);
        const QString tsAndSep = line.left(26); // "yyyy-...zzzZ  "
        const QString rest = line.mid(26);
        // rest starts with the 5-char severity label.
        const QString sev = rest.left(5);
        const QString tail = rest.mid(5);
        bytes.append(tsAndSep.toUtf8());
        bytes.append(severityColor(type));
        bytes.append(sev.toUtf8());
        bytes.append("\033[0m");
        bytes.append(tail.toUtf8());
    } else {
        bytes = line.toUtf8();
    }
    bytes.append('\n');
    std::fwrite(bytes.constData(), 1, static_cast<size_t>(bytes.size()),
        stderr);
    std::fflush(stderr);
}

void writeFile(const QString& line)
{
    QFile* f = logFile();
    if (f == nullptr) {
        return;
    }
    QMutexLocker lk(&fileMutex());
    if (!f->isOpen()) {
        return;
    }
    const QByteArray bytes = (line + QLatin1Char('\n')).toUtf8();
    f->write(bytes);
    f->flush();
}

void messageHandler(QtMsgType type, const QMessageLogContext& ctx,
    const QString& msg)
{
    const QString line = formatLine(type, ctx, msg);
    writeStderr(type, line);
    writeFile(line);

    if (type == QtFatalMsg) {
        // Make sure the file is on disk before the default fatal
        // handler aborts. The previous handler (if any) is invoked
        // by Qt's fallback flow; we don't chain explicitly.
        QFile* f = logFile();
        if (f != nullptr) {
            QMutexLocker lk(&fileMutex());
            f->flush();
        }
        std::fflush(stderr);
        // Fall through: if a previous handler exists, give it a
        // chance to do its own thing (e.g., a test collector). Qt
        // will still abort because `type == QtFatalMsg`.
        if (previousHandler() != nullptr) {
            previousHandler()(type, ctx, msg);
        }
    }
}

void rotateInPlace(const QString& base)
{
    // Shift .N -> .N+1, dropping the oldest, then base -> .1.
    for (int i = kMaxArchives; i >= 1; --i) {
        const QString src = base + QStringLiteral(".") + QString::number(i);
        if (i == kMaxArchives) {
            QFile::remove(src);
            continue;
        }
        if (QFile::exists(src)) {
            const QString dst = base + QStringLiteral(".")
                + QString::number(i + 1);
            QFile::remove(dst);
            QFile::rename(src, dst);
        }
    }
    if (QFile::exists(base)) {
        const QString dst = base + QStringLiteral(".1");
        QFile::remove(dst);
        QFile::rename(base, dst);
    }
}

QString openLogFile()
{
    if (QStandardPaths::isTestModeEnabled()) {
        return {};
    }
    const QString dataDir = QStandardPaths::writableLocation(
        QStandardPaths::AppDataLocation);
    if (dataDir.isEmpty()) {
        return {};
    }
    const QString logDir = dataDir + QStringLiteral("/logs");
    if (!QDir().mkpath(logDir)) {
        std::fputs(
            "kinema: failed to create log directory; file logging disabled\n",
            stderr);
        return {};
    }
    const QString path = logDir + QStringLiteral("/kinema.log");
    if (QFileInfo(path).size() > kMaxBytes) {
        rotateInPlace(path);
    }
    auto* f = new QFile(path);
    if (!f->open(QIODevice::Append | QIODevice::Text)) {
        std::fprintf(stderr,
            "kinema: failed to open %s for append; file logging disabled\n",
            qUtf8Printable(path));
        delete f;
        return {};
    }
    logFile() = f;
    return path;
}

void writeSessionHeader(const QString& logPath)
{
    const QString libmpv =
#ifdef KINEMA_HAVE_LIBMPV
        QStringLiteral("on");
#else
        QStringLiteral("off");
#endif
    const QString line = QStringLiteral(
        "Kinema %1 starting · Qt %2 · OS %3 · libmpv=%4 · locale=%5%6")
        .arg(QCoreApplication::applicationVersion(),
             QString::fromUtf8(qVersion()),
             QSysInfo::prettyProductName(),
             libmpv,
             QLocale().name(),
             logPath.isEmpty()
                 ? QStringLiteral(" · log=stderr-only")
                 : QStringLiteral(" · log=") + logPath);
    qCInfo(KINEMA_APP).noquote() << line;
}

} // namespace

void installLogging()
{
    static bool installed = false;
    if (installed) {
        return;
    }
    installed = true;

    // Pattern is the canonical Qt fallback when our handler is
    // bypassed (e.g., a test installs its own collector). Honour
    // QT_MESSAGE_PATTERN if the env already pinned one.
    if (qEnvironmentVariableIsEmpty("QT_MESSAGE_PATTERN")) {
        qSetMessagePattern(QStringLiteral(
            "%{time yyyy-MM-ddTHH:mm:ss.zzzZ}  %{type}  %{category}"
            "%{if-warning}  %{file}:%{line} in %{function}%{endif}"
            "%{if-critical}  %{file}:%{line} in %{function}%{endif}"
            "%{if-fatal}  %{file}:%{line} in %{function}%{endif}"
            "  %{message}"));
    }

    const QString logPath = openLogFile();
    previousHandler() = qInstallMessageHandler(&messageHandler);

    writeSessionHeader(logPath);
}

} // namespace kinema::core

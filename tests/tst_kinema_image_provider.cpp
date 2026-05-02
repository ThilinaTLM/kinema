// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/KinemaImageProvider.h"

#include <QQuickImageResponse>
#include <QQuickTextureFactory>
#include <QSignalSpy>
#include <QSize>
#include <QTest>

using namespace kinema::ui::qml;

// Smoke-tests the contract that `Image.status` ends up at
// `Image.Error` (not `Image.Ready`) when the underlying load fails.
//
// QQuickPixmapReader decides the status from `errorString()`
// emptiness alone — a null `textureFactory()` is *not* enough — so
// the provider must report a non-empty error string for any path
// that resolves with a null QImage. This test pins that behaviour
// down at the seam, since the QML fallback chains in
// EpisodeRailCard / KinemaArtworkFrame depend on it.
class TstKinemaImageProvider : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void invalidUrlResolvesAsError()
    {
        // Null loader + empty `u=` query exercises the early-return
        // failure path without requiring any real network or
        // ImageLoader instance.
        KinemaImageProvider provider(nullptr);

        QQuickImageResponse* response
            = provider.requestImageResponse(
                QStringLiteral("poster?u="), QSize());
        QVERIFY(response);

        QSignalSpy spy(response, &QQuickImageResponse::finished);
        QVERIFY(spy.wait(2000));

        // Non-empty errorString() is what makes Qt set the Image
        // status to Error rather than Ready-with-null-texture.
        QVERIFY2(!response->errorString().isEmpty(),
            "provider must report a non-empty errorString on "
            "failed load so Image.status becomes Image.Error");

        // textureFactory() is allowed to return nullptr in the
        // failure path — Qt won't call it once errorString() is
        // non-empty, but we sanity-check the existing contract.
        QQuickTextureFactory* factory = response->textureFactory();
        QVERIFY(factory == nullptr);

        response->deleteLater();
    }
};

QTEST_MAIN(TstKinemaImageProvider)
#include "tst_kinema_image_provider.moc"

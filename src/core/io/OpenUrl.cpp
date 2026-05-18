// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/io/OpenUrl.h"

#include <KIO/OpenUrlJob>

namespace kinema::core::io {

void openExternal(const QUrl& url,
    QObject* parent, OpenExternalCallback callback)
{
    auto* job = new KIO::OpenUrlJob(url, parent);
    job->setRunExecutables(false);
    if (callback) {
        QObject::connect(job, &KJob::result, parent,
            [job, cb = std::move(callback)] {
                OpenExternalResult r;
                if (job->error()) {
                    r.ok = false;
                    r.errorString = job->errorString();
                } else {
                    r.ok = true;
                }
                cb(r);
            });
    }
    job->start();
}

} // namespace kinema::core::io

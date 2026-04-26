// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>

namespace kinema::core::language {

/**
 * Map an ISO 639-2 (3-letter) language code to an English display
 * name. Covers the ~25 most common codes seen on OpenSubtitles; any
 * unknown code is returned as-is so the picker still shows
 * *something* recognisable.
 *
 * No ICU dependency, no locale awareness — display names are always
 * English. The picker uses these for the language column and its
 * filter chips; localisation can come later.
 */
QString displayName(const QString& iso639_2);

/// Inverse lookup: lower-case English name → ISO 639-2. Returns an
/// empty string when no mapping is known. Currently unused outside
/// tests but kept symmetric with `displayName`.
QString codeForDisplayName(const QString& englishName);

} // namespace kinema::core::language

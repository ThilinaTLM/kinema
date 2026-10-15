#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
# SPDX-License-Identifier: Apache-2.0
#
# Cut a Kinema release:
#   scripts/release.sh <version> [--test]
#
#   1. Validate the version (X.Y.Z) and repo state (clean tree, on main,
#      in sync with origin).
#   2. Bump PROJECT_VERSION in CMakeLists.txt.
#   3. Finalize or insert a <release> entry into
#      data/dev.tlmtech.kinema.metainfo.xml (dated today) and validate it.
#   4. Optionally build and run the test suite (when --test is given).
#   5. Commit "chore(release): bump version to X.Y.Z", create the signed
#      annotated tag vX.Y.Z, and push main together with the tag.
#
# The tag push triggers .github/workflows/release.yml, which builds the
# artifacts and publishes the GitHub Release.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
    echo "usage: $0 <version> [--test]" >&2
    exit 1
}

die() {
    echo "error: $*" >&2
    exit 1
}

[[ $# -ge 1 ]] || usage
VERSION="$1"
shift
RUN_TEST=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --test) RUN_TEST=1 ;;
        *) usage ;;
    esac
    shift
done

# --- validate version -----------------------------------------------------
[[ "${VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] \
    || die "'${VERSION}' is not a plain X.Y.Z version (use -rcN tags only for pre-releases)."

cd "${REPO_ROOT}"

git rev-parse -q --verify "refs/tags/v${VERSION}" >/dev/null \
    && die "tag v${VERSION} already exists."

BRANCH="$(git branch --show-current)"
[[ "${BRANCH}" == "main" ]] || die "must be on main (currently on '${BRANCH}')."

[[ -z "$(git status --porcelain)" ]] || die "working tree is not clean."

git fetch --quiet origin
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] \
    || die "main is not in sync with origin/main (pull or push first)."

# --- bump versions ---------------------------------------------------------
sed -i "s/^\(\s*VERSION \)[0-9][0-9.]*/\1${VERSION}/" CMakeLists.txt
grep -q "VERSION ${VERSION}" CMakeLists.txt \
    || die "failed to update PROJECT_VERSION in CMakeLists.txt."

METAINFO="data/dev.tlmtech.kinema.metainfo.xml"
TODAY="$(date +%F)"
python3 - "$VERSION" "$TODAY" "$METAINFO" <<'EOF'
import re
import sys

version, today, path = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, encoding="utf-8") as f:
    text = f.read()

existing = re.search(r'<release\b[^>]*\bversion="' + re.escape(version) + r'"[^>]*>', text)
if existing:
    # main may already describe the upcoming development release. Finalize its
    # type/date rather than inserting a duplicate or retaining an old date.
    opening = existing.group(0)
    opening = re.sub(r'\s+(?:type|date)="[^"]*"', '', opening)
    opening = opening[:-1] + f' type="stable" date="{today}">'
    text = text[:existing.start()] + opening + text[existing.end():]
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)
    print(f"Finalized existing {version} release notes.")
    sys.exit(0)

entry = (
    f'    <release version="{version}" date="{today}">\n'
    f"      <description>\n"
    f"        <p>TODO: summarize the release.</p>\n"
    f"      </description>\n"
    f"    </release>\n"
)
new_text, n = re.subn(r"(  <releases>\n)", rf"\1{entry}", text, count=1)
if n != 1:
    sys.exit("could not find <releases> block in metainfo")
with open(path, "w", encoding="utf-8") as f:
    f.write(new_text)
EOF
grep -q "version=\"${VERSION}\"" "${METAINFO}" \
    || die "failed to insert release entry into ${METAINFO}."

appstreamcli validate --no-net "${METAINFO}" \
    || die "AppStream metadata validation failed."

# --- validate ----------------------------------------------------------------
if [[ "${RUN_TEST}" -eq 1 ]]; then
    echo ">> Building..."
    cmake --build build -j"$(nproc)"
    echo ">> Running tests..."
    ctest --test-dir build --output-on-failure
else
    echo ">> Skipping build/test (use --test to enable)."
fi

# --- commit, tag, push --------------------------------------------------------
LAST_TAG="$(git describe --tags --abbrev=0)"
git add CMakeLists.txt "${METAINFO}"
git commit -m "chore(release): bump version to ${VERSION}"

echo ">> Changes since ${LAST_TAG}:"
git log --oneline "${LAST_TAG}..HEAD"

read -r -p "Tag v${VERSION} and push main + tag? [y/N] " answer
[[ "${answer}" == "y" || "${answer}" == "Y" ]] || die "aborted before tagging (commit is kept)."

git tag -s "v${VERSION}" -m "Kinema ${VERSION}"
git push origin main "v${VERSION}"

echo ">> Done. Release workflow: https://github.com/$(git remote get-url origin | sed 's#.*github.com[:/]##; s#\.git$##')/actions"

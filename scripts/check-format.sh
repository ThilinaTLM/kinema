#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: $0 <base-revision> <head-revision>" >&2
    exit 2
fi

base_revision=$1
head_revision=$2
clang_format=${CLANG_FORMAT:-clang-format}

if ! command -v "$clang_format" >/dev/null 2>&1; then
    echo "error: formatter '$clang_format' was not found" >&2
    echo "Set CLANG_FORMAT to the clang-format binary to use." >&2
    exit 2
fi

if ! git clang-format -h >/dev/null 2>&1; then
    echo "error: git-clang-format was not found" >&2
    exit 2
fi

for revision in "$base_revision" "$head_revision"; do
    if ! git rev-parse --verify --quiet "${revision}^{commit}" >/dev/null; then
        echo "error: '$revision' is not a valid commit" >&2
        exit 2
    fi
done

output_file=$(mktemp)
trap 'rm -f "$output_file"' EXIT

set +e
git clang-format \
    --binary "$clang_format" \
    --diff \
    --extensions cpp,h,hpp \
    "$base_revision" \
    "$head_revision" >"$output_file"
format_status=$?
set -e

if grep -q '^diff --git ' "$output_file"; then
    cat "$output_file"
    echo >&2
    echo "error: changed C++ lines are not clang-formatted" >&2
    echo "Run 'git clang-format $base_revision' from the branch tip, then commit the result." >&2
    exit 1
fi

if [[ $format_status -ne 0 ]]; then
    cat "$output_file" >&2
    echo "error: git-clang-format failed" >&2
    exit 2
fi

cat "$output_file"

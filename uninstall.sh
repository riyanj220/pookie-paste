#!/usr/bin/env bash

set -euo pipefail

#
# If running locally from a cloned repository, delegate directly to scripts/uninstall.sh.
#
if [[ -f "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(
        cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1
        pwd
    )"

    if [[ -f "${SCRIPT_DIR}/scripts/uninstall.sh" ]]; then
        exec "${SCRIPT_DIR}/scripts/uninstall.sh" "$@"
    fi
fi

POOKIE_GITHUB_REPOSITORY="riyanj220/pookie-paste"

POOKIE_GITHUB_BASE_URL="https://github.com/${POOKIE_GITHUB_REPOSITORY}"

REQUESTED_VERSION="${POOKIE_VERSION:-latest}"

BOOTSTRAP_WORK_DIR=""

is_valid_release_version() {
    local version="$1"

    [[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]
}

cleanup() {
    if [[ -n "${BOOTSTRAP_WORK_DIR:-}" \
        && -d "$BOOTSTRAP_WORK_DIR" ]]
    then
        rm -rf "$BOOTSTRAP_WORK_DIR"
    fi
}

resolve_latest_version() {
    local effective_url
    local version

    effective_url="$(
        curl \
            --fail \
            --silent \
            --show-error \
            --location \
            --output /dev/null \
            --write-out '%{url_effective}' \
            "${POOKIE_GITHUB_BASE_URL}/releases/latest"
    )"

    version="${effective_url##*/}"

    if ! is_valid_release_version "$version"; then
        echo "Unable to determine the latest Pookie Paste release." >&2
        echo >&2
        echo "GitHub resolved to:" >&2
        echo "  ${effective_url}" >&2
        return 1
    fi

    printf '%s\n' "$version"
}

resolve_version() {
    local requested="$1"

    if [[ -z "$requested" || "$requested" == "latest" ]]; then
        resolve_latest_version
        return
    fi

    if ! is_valid_release_version "$requested"; then
        echo "Invalid Pookie Paste version: ${requested}" >&2
        echo >&2
        echo "Expected something like:" >&2
        echo "  v0.1.0" >&2
        echo "  latest" >&2
        return 1
    fi

    printf '%s\n' "$requested"
}

trap cleanup EXIT

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "Pookie Paste currently supports Linux only." >&2
    exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required to bootstrap Pookie Paste uninstaller." >&2
    exit 1
fi

if ! command -v tar >/dev/null 2>&1; then
    echo "tar is required to bootstrap Pookie Paste uninstaller." >&2
    exit 1
fi

VERSION="$(
    resolve_version "$REQUESTED_VERSION"
)"

BOOTSTRAP_WORK_DIR="$(
    mktemp \
        -d \
        "${TMPDIR:-/tmp}/pookie-paste-uninstall-bootstrap.XXXXXX"
)"

SOURCE_ARCHIVE="${BOOTSTRAP_WORK_DIR}/source.tar.gz"

SOURCE_DIR="${BOOTSTRAP_WORK_DIR}/source"

mkdir -p "$SOURCE_DIR"

SOURCE_ARCHIVE_URL="${POOKIE_GITHUB_BASE_URL}/archive/refs/tags/${VERSION}.tar.gz"

curl \
    --fail \
    --silent \
    --location \
    --retry 3 \
    --retry-delay 2 \
    --show-error \
    --output "$SOURCE_ARCHIVE" \
    "$SOURCE_ARCHIVE_URL"

tar \
    -xzf "$SOURCE_ARCHIVE" \
    -C "$SOURCE_DIR"

INNER_UNINSTALLER="$(
    find \
        "$SOURCE_DIR" \
        -type f \
        -name 'uninstall.sh' \
        -path '*/scripts/uninstall.sh' \
        -print \
        -quit
)"

if [[ -z "$INNER_UNINSTALLER" \
    || ! -f "$INNER_UNINSTALLER" ]]
then
    echo "The downloaded release does not contain scripts/uninstall.sh." >&2
    exit 1
fi

chmod +x "$INNER_UNINSTALLER"

"$INNER_UNINSTALLER" "$@"

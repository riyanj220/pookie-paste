#!/usr/bin/env bash

set -euo pipefail

POOKIE_GITHUB_REPOSITORY="riyanj220/pookie-paste"

POOKIE_GITHUB_BASE_URL="https://github.com/${POOKIE_GITHUB_REPOSITORY}"

REQUESTED_VERSION="${POOKIE_VERSION:-latest}"

FROM_SOURCE=false

BOOTSTRAP_WORK_DIR=""

usage() {
    cat <<EOF
Pookie Paste bootstrap installer

Usage:
  install.sh [options]

Options:
  --version <version>
      Install a specific Pookie Paste release.

      Examples:
        --version v0.1.0
        --version latest

  --from-source
      Build Pookie Paste from source instead of using
      the prebuilt release binary.

  -h, --help
      Show this help message.

Environment:
  POOKIE_VERSION
      Select a release version without passing --version.

Examples:
  Install latest stable release:

    curl -fsSL \
      https://raw.githubusercontent.com/${POOKIE_GITHUB_REPOSITORY}/main/install.sh \
      | bash

  Install a specific release:

    curl -fsSL \
      https://raw.githubusercontent.com/${POOKIE_GITHUB_REPOSITORY}/main/install.sh \
      | bash -s -- --version v0.1.0

  Using an environment variable:

    curl -fsSL \
      https://raw.githubusercontent.com/${POOKIE_GITHUB_REPOSITORY}/main/install.sh \
      | POOKIE_VERSION=v0.1.0 bash
EOF
}

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

while (( $# > 0 )); do
    case "$1" in
        --version)
            if (( $# < 2 )); then
                echo "--version requires a value." >&2
                exit 1
            fi

            REQUESTED_VERSION="$2"

            shift 2
            ;;

        --from-source)
            FROM_SOURCE=true
            shift
            ;;

        -h|--help)
            usage
            exit 0
            ;;

        *)
            echo "Unknown option: $1" >&2
            echo
            usage >&2
            exit 1
            ;;
    esac
done

trap cleanup EXIT

echo
echo "Pookie Paste bootstrap installer"
echo "================================"
echo

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "Pookie Paste currently supports Linux only." >&2
    exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required to bootstrap Pookie Paste." >&2
    exit 1
fi

if ! command -v tar >/dev/null 2>&1; then
    echo "tar is required to bootstrap Pookie Paste." >&2
    exit 1
fi

VERSION="$(
    resolve_version "$REQUESTED_VERSION"
)"

echo "Selected release:"
echo "  ${VERSION}"
echo

BOOTSTRAP_WORK_DIR="$(
    mktemp \
        -d \
        "${TMPDIR:-/tmp}/pookie-paste-bootstrap.XXXXXX"
)"

SOURCE_ARCHIVE="${BOOTSTRAP_WORK_DIR}/source.tar.gz"

SOURCE_DIR="${BOOTSTRAP_WORK_DIR}/source"

mkdir -p "$SOURCE_DIR"

SOURCE_ARCHIVE_URL="${POOKIE_GITHUB_BASE_URL}/archive/refs/tags/${VERSION}.tar.gz"

echo "Downloading installer files for ${VERSION}..."

curl \
    --fail \
    --location \
    --retry 3 \
    --retry-delay 2 \
    --show-error \
    --output "$SOURCE_ARCHIVE" \
    "$SOURCE_ARCHIVE_URL"

echo "Extracting installer files..."

tar \
    -xzf "$SOURCE_ARCHIVE" \
    -C "$SOURCE_DIR"

INNER_INSTALLER="$(
    find \
        "$SOURCE_DIR" \
        -mindepth 2 \
        -maxdepth 2 \
        -type f \
        -path '*/scripts/install.sh' \
        -print \
        -quit
)"

if [[ -z "$INNER_INSTALLER" \
    || ! -f "$INNER_INSTALLER" ]]
then
    echo "The downloaded release does not contain scripts/install.sh." >&2
    exit 1
fi

chmod +x "$INNER_INSTALLER"

echo
echo "Starting Pookie Paste installer..."
echo

if [[ "$FROM_SOURCE" == true ]]; then
    "$INNER_INSTALLER" \
        --from-source
else
    "$INNER_INSTALLER" \
        --version "$VERSION"
fi

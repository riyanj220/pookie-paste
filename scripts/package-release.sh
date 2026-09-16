#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(
    cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1
    pwd
)"

PROJECT_ROOT="$(
    cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1
    pwd
)"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/architecture.sh"

usage() {
    cat <<EOF
Usage:
  ./scripts/package-release.sh <version> [architecture]

Examples:
  ./scripts/package-release.sh v0.1.0
  ./scripts/package-release.sh v0.1.0 x86_64

The release binaries must already exist in:

  target/release/pookie-paste
  target/release/pookie-paste-ui
EOF
}

package_version_from_cargo_toml() {
    local cargo_toml="$1"

    awk '
        /^\[package\]$/ {
            in_package = 1
            next
        }

        /^\[/ && in_package {
            exit
        }

        in_package && /^version[[:space:]]*=/ {
            gsub(/^[^"]*"/, "")
            gsub(/".*$/, "")
            print
            exit
        }
    ' "$cargo_toml"
}

if (( $# < 1 || $# > 2 )); then
    usage >&2
    exit 1
fi

RELEASE_VERSION="$1"

RAW_ARCHITECTURE="${2:-$(uname -m)}"

ARCHITECTURE="$(
    detect_architecture "$RAW_ARCHITECTURE"
)"

if [[ "$ARCHITECTURE" == "unsupported" ]]; then
    echo "Unsupported release architecture: ${RAW_ARCHITECTURE}" >&2
    exit 1
fi

if [[ ! "$RELEASE_VERSION" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]; then
    echo "Invalid release version: ${RELEASE_VERSION}" >&2
    echo "Expected something like:" >&2
    echo "  v0.1.0" >&2
    echo "  v0.1.0-rc.1" >&2
    exit 1
fi

PLATFORM="$(
    release_platform "$ARCHITECTURE"
)"

VERSION_WITHOUT_V="${RELEASE_VERSION#v}"

#
# Pre-release tags such as:
#
#   v0.1.0-rc.1
#
# should still correspond to package version 0.1.0.
#
CARGO_VERSION="${VERSION_WITHOUT_V%%-*}"

DAEMON_PACKAGE_VERSION="$(
    package_version_from_cargo_toml \
        "${PROJECT_ROOT}/crates/daemon/Cargo.toml"
)"

UI_PACKAGE_VERSION="$(
    package_version_from_cargo_toml \
        "${PROJECT_ROOT}/crates/ui/Cargo.toml"
)"

if [[ "$DAEMON_PACKAGE_VERSION" != "$CARGO_VERSION" ]]; then
    echo "Release version does not match daemon Cargo.toml." >&2
    echo >&2
    echo "Release: ${CARGO_VERSION}" >&2
    echo "Daemon:  ${DAEMON_PACKAGE_VERSION}" >&2
    exit 1
fi

if [[ "$UI_PACKAGE_VERSION" != "$CARGO_VERSION" ]]; then
    echo "Release version does not match UI Cargo.toml." >&2
    echo >&2
    echo "Release: ${CARGO_VERSION}" >&2
    echo "UI:       ${UI_PACKAGE_VERSION}" >&2
    exit 1
fi

DAEMON_SOURCE="${PROJECT_ROOT}/target/release/pookie-paste"

UI_SOURCE="${PROJECT_ROOT}/target/release/pookie-paste-ui"

if [[ ! -x "$DAEMON_SOURCE" ]]; then
    echo "Release daemon binary was not found:" >&2
    echo "  ${DAEMON_SOURCE}" >&2
    echo >&2
    echo "Run:" >&2
    echo "  cargo build --release -p daemon -p ui" >&2
    exit 1
fi

if [[ ! -x "$UI_SOURCE" ]]; then
    echo "Release UI binary was not found:" >&2
    echo "  ${UI_SOURCE}" >&2
    echo >&2
    echo "Run:" >&2
    echo "  cargo build --release -p daemon -p ui" >&2
    exit 1
fi

DIST_DIR="${PROJECT_ROOT}/dist"

BUNDLE_NAME="pookie-paste-${RELEASE_VERSION}-${PLATFORM}"

BUNDLE_DIR="${DIST_DIR}/${BUNDLE_NAME}"

ARCHIVE_NAME="${BUNDLE_NAME}.tar.gz"

ARCHIVE_PATH="${DIST_DIR}/${ARCHIVE_NAME}"

CHECKSUM_PATH="${DIST_DIR}/SHA256SUMS"

echo
echo "Packaging Pookie Paste"
echo "======================"
echo
echo "Version:      ${RELEASE_VERSION}"
echo "Architecture: ${ARCHITECTURE}"
echo "Platform:     ${PLATFORM}"
echo

mkdir -p "$DIST_DIR"

rm -rf "$BUNDLE_DIR"

rm -f \
    "$ARCHIVE_PATH" \
    "$CHECKSUM_PATH"

mkdir -p \
    "${BUNDLE_DIR}/bin" \
    "${BUNDLE_DIR}/share/applications" \
    "${BUNDLE_DIR}/share/autostart" \
    "${BUNDLE_DIR}/share/pookie-paste/kwin"

install \
    -m 0755 \
    "$DAEMON_SOURCE" \
    "${BUNDLE_DIR}/bin/pookie-paste"

install \
    -m 0755 \
    "$UI_SOURCE" \
    "${BUNDLE_DIR}/bin/pookie-paste-ui"

install \
    -m 0644 \
    "${PROJECT_ROOT}/packaging/linux/io.github.riyanj220.PookiePaste.desktop" \
    "${BUNDLE_DIR}/share/applications/io.github.riyanj220.PookiePaste.desktop"

install \
    -m 0644 \
    "${PROJECT_ROOT}/packaging/linux/io.github.riyanj220.PookiePaste-autostart.desktop" \
    "${BUNDLE_DIR}/share/autostart/io.github.riyanj220.PookiePaste-autostart.desktop"

cp -a \
    "${PROJECT_ROOT}/extras/kwin/pookie-focus" \
    "${BUNDLE_DIR}/share/pookie-paste/kwin/pookie-focus"

install \
    -m 0644 \
    "${PROJECT_ROOT}/LICENSE" \
    "${BUNDLE_DIR}/LICENSE"

install \
    -m 0644 \
    "${PROJECT_ROOT}/README.md" \
    "${BUNDLE_DIR}/README.md"

printf '%s\n' \
    "$RELEASE_VERSION" \
    >"${BUNDLE_DIR}/RELEASE_VERSION"

echo "Creating release archive..."

tar \
    -C "$DIST_DIR" \
    -czf "$ARCHIVE_PATH" \
    "$BUNDLE_NAME"

echo "Generating SHA256 checksum..."

(
    cd "$DIST_DIR"

    sha256sum \
        "$ARCHIVE_NAME" \
        >SHA256SUMS
)

echo
echo "Release package created:"
echo
echo "  ${ARCHIVE_PATH}"
echo
echo "Checksum:"
echo "  ${CHECKSUM_PATH}"
echo

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
source "${SCRIPT_DIR}/lib/distro.sh"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/dependencies.sh"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/paths.sh"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/kde.sh"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/process.sh"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/architecture.sh"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/release.sh"

FROM_SOURCE=false

REQUESTED_VERSION="${POOKIE_VERSION:-latest}"

usage() {
    cat <<EOF
Usage:
  ./scripts/install.sh [options]

Options:
  --from-source
      Build Pookie Paste locally using Cargo instead of
      downloading a prebuilt GitHub release.

  --version <version>
      Install a specific prebuilt release.

      Examples:
        --version v0.1.1
        --version latest

  -h, --help
      Show this help message.

Environment:
  POOKIE_VERSION
      Alternative way to select a release version.

Examples:
  ./scripts/install.sh

  ./scripts/install.sh --version v0.1.1

  POOKIE_VERSION=v0.1.1 ./scripts/install.sh

  ./scripts/install.sh --from-source
EOF
}

while (( $# > 0 )); do
    case "$1" in
        --from-source)
            FROM_SOURCE=true
            shift
            ;;

        --version)
            if (( $# < 2 )); then
                echo "--version requires a value." >&2
                exit 1
            fi

            REQUESTED_VERSION="$2"

            shift 2
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

cleanup() {
    cleanup_release_bundle
}

trap cleanup EXIT

echo
echo "  ┌─ Pookie Paste"
echo "  │  Clipboard history for Linux"
echo "  └─"
echo

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "Pookie Paste currently supports Linux only." >&2
    exit 1
fi

DISTRO_FAMILY="$(
    detect_distro_family
)"

if [[ "$DISTRO_FAMILY" == "unsupported" ]]; then
    echo "This Linux distribution is not currently supported." >&2
    echo
    echo "Supported distribution families:"
    echo "  Debian / Ubuntu"
    echo "  Fedora / RHEL"
    echo "  Arch / Manjaro"
    echo "  openSUSE"
    exit 1
fi

IS_UPGRADE=false
if [[ -x "$POOKIE_DAEMON_DEST" \
    || -x "$POOKIE_UI_DEST" ]]
then
    IS_UPGRADE=true
fi

DAEMON_SOURCE=""

UI_SOURCE=""

DESKTOP_SOURCE=""

AUTOSTART_SOURCE=""

KWIN_SOURCE=""

if [[ "$FROM_SOURCE" == true ]]; then
    if ! command -v cc >/dev/null 2>&1 \
        || ! command -v make >/dev/null 2>&1 \
        || ! command -v curl >/dev/null 2>&1
    then
        install_source_dependencies \
            "$DISTRO_FAMILY"
    fi

    if ! command -v cargo >/dev/null 2>&1; then
        echo
        echo "Rust is not installed."
        echo "Installing Rust with rustup..."

        curl \
            --proto '=https' \
            --tlsv1.2 \
            --silent \
            --show-error \
            --fail \
            https://sh.rustup.rs |
            sh -s -- -y

        # shellcheck disable=SC1090
        source "${HOME}/.cargo/env"
    fi

    if ! command -v cargo >/dev/null 2>&1; then
        echo "Cargo is still unavailable after Rust installation." >&2
        exit 1
    fi

    echo
    echo "Building Pookie Paste..."

    (
        cd "$PROJECT_ROOT"

        cargo build \
            --release \
            -p daemon \
            -p ui
    )

    DAEMON_SOURCE="${PROJECT_ROOT}/target/release/pookie-paste"

    UI_SOURCE="${PROJECT_ROOT}/target/release/pookie-paste-ui"

    DESKTOP_SOURCE="${PROJECT_ROOT}/packaging/linux/io.github.riyanj220.PookiePaste.desktop"

    AUTOSTART_SOURCE="${PROJECT_ROOT}/packaging/linux/io.github.riyanj220.PookiePaste-autostart.desktop"

    KWIN_SOURCE="${PROJECT_ROOT}/extras/kwin/pookie-focus"
else
    if ! command -v curl >/dev/null 2>&1 \
        || ! command -v tar >/dev/null 2>&1 \
        || ! command -v sha256sum >/dev/null 2>&1
    then
        install_download_dependencies \
            "$DISTRO_FAMILY"
    fi

    ARCHITECTURE="$(
        detect_architecture
    )"

    if [[ "$ARCHITECTURE" == "unsupported" ]]; then
        echo "Unsupported CPU architecture:" >&2
        echo "  $(uname -m)" >&2
        exit 1
    fi

    #
    # Only x86_64 prebuilt artifacts are published during
    # the first release phase.
    #
    # architecture.sh already recognizes aarch64 so adding
    # the ARM release later does not require redesigning the
    # installer.
    #
    if [[ "$ARCHITECTURE" != "x86_64" ]]; then
        echo "A prebuilt Pookie Paste release is not currently available for:" >&2
        echo "  ${ARCHITECTURE}" >&2
        echo >&2
        echo "Current prebuilt support:" >&2
        echo "  x86_64" >&2
        echo >&2
        echo "You can still build Pookie Paste from source with:" >&2
        echo "  ./scripts/install.sh --from-source" >&2
        exit 1
    fi

    prepare_release_bundle \
        "$REQUESTED_VERSION" \
        "$ARCHITECTURE"

    DAEMON_SOURCE="${POOKIE_RELEASE_BUNDLE_DIR}/bin/pookie-paste"

    UI_SOURCE="${POOKIE_RELEASE_BUNDLE_DIR}/bin/pookie-paste-ui"

    DESKTOP_SOURCE="${POOKIE_RELEASE_BUNDLE_DIR}/share/applications/io.github.riyanj220.PookiePaste.desktop"

    AUTOSTART_SOURCE="${POOKIE_RELEASE_BUNDLE_DIR}/share/autostart/io.github.riyanj220.PookiePaste-autostart.desktop"

    KWIN_SOURCE="${POOKIE_RELEASE_BUNDLE_DIR}/share/pookie-paste/kwin/pookie-focus"
fi

#
# Nothing below this point is allowed to run until the new
# installation payload has been completely prepared.
#
# This protects an existing working installation from:
#
#   download failures
#   checksum failures
#   extraction failures
#   invalid release bundles
#   source-build failures
#

if [[ ! -x "$DAEMON_SOURCE" ]]; then
    echo "Prepared daemon binary is missing:" >&2
    echo "  ${DAEMON_SOURCE}" >&2
    exit 1
fi

if [[ ! -x "$UI_SOURCE" ]]; then
    echo "Prepared UI binary is missing:" >&2
    echo "  ${UI_SOURCE}" >&2
    exit 1
fi

if [[ ! -f "$DESKTOP_SOURCE" ]]; then
    echo "Prepared desktop file is missing:" >&2
    echo "  ${DESKTOP_SOURCE}" >&2
    exit 1
fi

if [[ ! -f "$AUTOSTART_SOURCE" ]]; then
    echo "Prepared autostart file is missing:" >&2
    echo "  ${AUTOSTART_SOURCE}" >&2
    exit 1
fi

if [[ ! -f "${KWIN_SOURCE}/metadata.json" \
    || ! -f "${KWIN_SOURCE}/contents/code/main.js" ]]
then
    echo "Prepared KWin helper is incomplete:" >&2
    echo "  ${KWIN_SOURCE}" >&2
    exit 1
fi

if [[ "$IS_UPGRADE" == true ]]; then
    echo "Updating Pookie Paste..."
else
    echo "Installing Pookie Paste..."
fi

stop_pookie

ensure_install_directories

install \
    -m 0755 \
    "$DAEMON_SOURCE" \
    "$POOKIE_DAEMON_DEST"

install \
    -m 0755 \
    "$UI_SOURCE" \
    "$POOKIE_UI_DEST"

install \
    -m 0644 \
    "$DESKTOP_SOURCE" \
    "$POOKIE_DESKTOP_DEST"

install \
    -m 0644 \
    "$AUTOSTART_SOURCE" \
    "$POOKIE_AUTOSTART_DEST"

if is_kde_session; then
    install_kde_dependencies \
        "$DISTRO_FAMILY"

    install_and_enable_kwin_helper \
        "$KWIN_SOURCE"
fi

start_pookie \
    "$POOKIE_DAEMON_DEST" \
    "$POOKIE_STATE_DIR"

echo
handle_pookie_onboarding \
    "$POOKIE_DAEMON_DEST" \
    "$POOKIE_SHORTCUT_STATUS" \
    "$POOKIE_SHORTCUT"

case ":${PATH}:" in
    *":${POOKIE_BIN_DIR}:"*)
        ;;

    *)
        echo
        echo "Note: ~/.local/bin isn't in this shell's PATH."
        echo "Pookie will work normally, but terminal commands may require the full path."
        ;;
esac
echo

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

echo
echo "Pookie Paste installer"
echo "======================"
echo

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "Pookie Paste currently supports Linux only." >&2
    exit 1
fi

DISTRO_FAMILY="$(detect_distro_family)"

if [[ "$DISTRO_FAMILY" == "unsupported" ]]; then
    echo "Unsupported Linux distribution." >&2
    echo
    echo "Supported families:"
    echo "  Debian / Ubuntu"
    echo "  Fedora / RHEL"
    echo "  Arch / Manjaro"
    echo "  openSUSE"
    exit 1
fi

echo "Detected distribution family: ${DISTRO_FAMILY}"

echo
echo "Checking build dependencies..."

if ! command -v cc >/dev/null 2>&1 \
    || ! command -v make >/dev/null 2>&1 \
    || ! command -v curl >/dev/null 2>&1
then
    install_base_dependencies "$DISTRO_FAMILY"
else
    echo "Build dependencies already available."
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo
    echo "Rust is not installed."
    echo "Installing Rust using rustup..."

    curl \
        --proto '=https' \
        --tlsv1.2 \
        -sSf \
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
echo "Building Pookie Paste release binaries..."

cd "$PROJECT_ROOT"

cargo build \
    --release \
    -p daemon \
    -p ui

DAEMON_SOURCE="${PROJECT_ROOT}/target/release/pookie-paste"

UI_SOURCE="${PROJECT_ROOT}/target/release/pookie-paste-ui"

if [[ ! -x "$DAEMON_SOURCE" ]]; then
    echo "Built daemon binary was not found." >&2
    exit 1
fi

if [[ ! -x "$UI_SOURCE" ]]; then
    echo "Built UI binary was not found." >&2
    exit 1
fi

echo
echo "Installing application files..."

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
    "${PROJECT_ROOT}/packaging/linux/io.github.riyanj220.PookiePaste.desktop" \
    "$POOKIE_DESKTOP_DEST"

install \
    -m 0644 \
    "${PROJECT_ROOT}/packaging/linux/io.github.riyanj220.PookiePaste-autostart.desktop" \
    "$POOKIE_AUTOSTART_DEST"

echo "Installed:"
echo "  $POOKIE_DAEMON_DEST"
echo "  $POOKIE_UI_DEST"

#
# KDE-specific direct-paste support.
#
if is_kde_session; then
    echo
    echo "KDE Plasma detected."

    install_kde_dependencies "$DISTRO_FAMILY"

    install_and_enable_kwin_helper \
        "${PROJECT_ROOT}/extras/kwin/pookie-focus"
else
    echo
    echo "KDE Plasma not detected."
    echo "Skipping KWin focus helper."
fi

#
# ~/.local/bin is the standard user-local executable location.
#
case ":${PATH}:" in
    *":${POOKIE_BIN_DIR}:"*)
        ;;

    *)
        echo
        echo "WARNING:"
        echo "${POOKIE_BIN_DIR} is not currently in PATH."
        echo
        echo "Add this to your shell configuration:"
        echo
        echo 'export PATH="$HOME/.local/bin:$PATH"'
        ;;
esac

echo
echo "Starting Pookie Paste..."

#
# Stop a currently running installed instance first.
#
if pgrep -u "$(id -u)" -x pookie-paste >/dev/null 2>&1; then
    pkill -u "$(id -u)" -x pookie-paste || true

    sleep 1
fi

#
# Ensure the state directory exists before redirecting logs into it.
#
STATE_DIR="${XDG_STATE_HOME:-${HOME}/.local/state}/pookie-paste"

mkdir -p "$STATE_DIR"

nohup "$POOKIE_DAEMON_DEST" \
    >"${STATE_DIR}/install-start.log" \
    2>&1 &

POOKIE_PID=$!

sleep 2

if kill -0 "$POOKIE_PID" 2>/dev/null; then
    echo
    echo "Pookie Paste is running."
else
    echo
    echo "Pookie Paste did not remain running." >&2
    echo "Check:"
    echo "  ${STATE_DIR}/install-start.log"
    exit 1
fi

echo
echo "Installation complete."
echo
echo "Use:"
echo
echo "    Super+V"
echo
echo "to open clipboard history."
echo

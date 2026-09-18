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

REQUESTED_VERSION="${POOKIE_VERSION:-latest}"

FROM_SOURCE=false

KEEP_TEMP=false

TEST_ROOT=""

TEST_HOME=""

REAL_HOME="${HOME:-}"

REAL_XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-}"

REAL_CARGO_HOME="${CARGO_HOME:-${REAL_HOME}/.cargo}"

REAL_RUSTUP_HOME="${RUSTUP_HOME:-${REAL_HOME}/.rustup}"

usage() {
    cat <<EOF
Pookie Paste installation smoke test

Usage:
  ./scripts/smoke-test-install.sh [options]

Options:
  --version <version>
      Test a specific prebuilt release.

      Examples:
        --version v0.1.1
        --version latest

  --from-source
      Test the source-build installation path instead
      of the public prebuilt bootstrap installer.

  --keep-temp
      Keep the temporary smoke-test directory after
      the test finishes.

  -h, --help
      Show this help message.

Examples:
  Test the latest published release:

    ./scripts/smoke-test-install.sh

  Test a specific release:

    ./scripts/smoke-test-install.sh \
      --version v0.1.1

  Test installation from source:

    ./scripts/smoke-test-install.sh \
      --from-source

Important:
  No Pookie Paste daemon may already be running.

  Run this test from a graphical Linux session because
  the daemon requires access to the desktop session.

  Persistent application data is isolated in a temporary
  HOME, but the real XDG_RUNTIME_DIR is preserved so the
  daemon can access Wayland/X11 session resources.

  Source-build smoke tests also preserve the invoking
  developer's Cargo/rustup toolchain locations. This keeps
  application data isolated without hiding the installed
  Rust toolchain when HOME is redirected.

  KDE-specific integration is intentionally skipped by
  this isolated smoke test. KDE is tested separately in
  the platform validation phase.
EOF
}

fail() {
    echo
    echo "SMOKE TEST FAILED"
    echo "================="
    echo
    echo "$1" >&2

    exit 1
}

pass() {
    echo "  [PASS] $1"
}

assert_file_exists() {
    local path="$1"
    local description="$2"

    if [[ ! -f "$path" ]]; then
        fail "${description} does not exist: ${path}"
    fi

    pass "$description"
}

assert_executable_exists() {
    local path="$1"
    local description="$2"

    if [[ ! -x "$path" ]]; then
        fail "${description} is missing or not executable: ${path}"
    fi

    pass "$description"
}

assert_directory_exists() {
    local path="$1"
    local description="$2"

    if [[ ! -d "$path" ]]; then
        fail "${description} does not exist: ${path}"
    fi

    pass "$description"
}

assert_socket_exists() {
    local path="$1"
    local description="$2"

    if [[ ! -S "$path" ]]; then
        fail "${description} does not exist: ${path}"
    fi

    pass "$description"
}

assert_path_missing() {
    local path="$1"
    local description="$2"

    if [[ -e "$path" || -L "$path" ]]; then
        fail "${description} still exists: ${path}"
    fi

    pass "$description"
}

pookie_running() {
    pgrep \
        -u "$(id -u)" \
        -x pookie-paste \
        >/dev/null 2>&1
}

wait_for_daemon() {
    local timeout_seconds="${1:-10}"
    local elapsed=0

    while ! pookie_running; do
        if (( elapsed >= timeout_seconds )); then
            return 1
        fi

        sleep 1

        elapsed=$((elapsed + 1))
    done

    return 0
}

stop_test_daemon_best_effort() {
    if ! pookie_running; then
        return 0
    fi

    pkill \
        -TERM \
        -u "$(id -u)" \
        -x pookie-paste \
        >/dev/null 2>&1 || true

    local elapsed=0

    while pookie_running; do
        if (( elapsed >= 5 )); then
            pkill \
                -KILL \
                -u "$(id -u)" \
                -x pookie-paste \
                >/dev/null 2>&1 || true

            break
        fi

        sleep 1

        elapsed=$((elapsed + 1))
    done
}

cleanup() {
    local exit_code=$?

    stop_test_daemon_best_effort

    if [[ -n "${TEST_ROOT:-}" \
        && -d "$TEST_ROOT" ]]
    then
        if [[ "$KEEP_TEMP" == true ]]; then
            echo
            echo "Temporary smoke-test environment preserved:"
            echo "  ${TEST_ROOT}"
        else
            rm -rf "$TEST_ROOT"
        fi
    fi

    return "$exit_code"
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

        --keep-temp)
            KEEP_TEMP=true
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
echo "Pookie Paste installation smoke test"
echo "===================================="
echo

if [[ "$(uname -s)" != "Linux" ]]; then
    fail "The smoke test currently supports Linux only."
fi

if [[ -z "${DISPLAY:-}" \
    && -z "${WAYLAND_DISPLAY:-}" ]]
then
    fail \
        "No graphical Linux session was detected. Run the smoke test from X11 or Wayland."
fi

if [[ -z "$REAL_XDG_RUNTIME_DIR" \
    || ! -d "$REAL_XDG_RUNTIME_DIR" ]]
then
    fail \
        "A valid XDG_RUNTIME_DIR is required for graphical-session testing."
fi

if pookie_running; then
    fail \
        "A Pookie Paste daemon is already running. Stop your normal Pookie instance before running this smoke test."
fi

if [[ ! -x "${PROJECT_ROOT}/install.sh" ]]; then
    fail \
        "Root bootstrap installer is missing or not executable: ${PROJECT_ROOT}/install.sh"
fi

if [[ ! -x "${PROJECT_ROOT}/scripts/install.sh" ]]; then
    fail \
        "Internal installer is missing or not executable: ${PROJECT_ROOT}/scripts/install.sh"
fi

if [[ ! -x "${PROJECT_ROOT}/scripts/uninstall.sh" ]]; then
    fail \
        "Uninstaller is missing or not executable: ${PROJECT_ROOT}/scripts/uninstall.sh"
fi

TEST_ROOT="$(
    mktemp \
        -d \
        "${TMPDIR:-/tmp}/pookie-paste-smoke.XXXXXX"
)"

TEST_HOME="${TEST_ROOT}/home"

mkdir -p \
    "$TEST_HOME" \
    "${TEST_HOME}/.local/bin" \
    "${TEST_HOME}/.local/share" \
    "${TEST_HOME}/.local/state" \
    "${TEST_HOME}/.config"

#
# Persistent Pookie files are redirected into this disposable
# HOME/XDG environment.
#
# XDG_RUNTIME_DIR intentionally remains the real graphical
# session runtime directory. Wayland, the session D-Bus, and
# desktop portals depend on resources stored there.
#
export HOME="$TEST_HOME"

export XDG_DATA_HOME="${TEST_HOME}/.local/share"

export XDG_CONFIG_HOME="${TEST_HOME}/.config"

export XDG_STATE_HOME="${TEST_HOME}/.local/state"

export XDG_RUNTIME_DIR="$REAL_XDG_RUNTIME_DIR"

#
# A source-build smoke test must use the developer's existing
# Rust toolchain even though HOME is redirected above.
#
# rustup normally resolves its toolchains from:
#
#   $HOME/.rustup
#
# and Cargo normally resolves its home from:
#
#   $HOME/.cargo
#
# Without preserving these locations, the rustup proxy can be
# found through the inherited PATH but cannot find the default
# toolchain, causing cargo to fail before the source build
# starts.
#
if [[ "$FROM_SOURCE" == true ]]; then
    export CARGO_HOME="$REAL_CARGO_HOME"
    export RUSTUP_HOME="$REAL_RUSTUP_HOME"

    export PATH="${CARGO_HOME}/bin:${PATH}"
fi

#
# The generic installation smoke test intentionally skips KDE
# helper installation.
#
# KDE/KWin integration is tested separately against the real
# Plasma session during platform validation.
#
export XDG_CURRENT_DESKTOP="PookieSmokeTest"

export PATH="${TEST_HOME}/.local/bin:${PATH}"

POOKIE_BIN_DIR="${TEST_HOME}/.local/bin"

POOKIE_DATA_DIR="${XDG_DATA_HOME}/pookie-paste"

POOKIE_STATE_DIR="${XDG_STATE_HOME}/pookie-paste"

POOKIE_RUNTIME_DIR="${XDG_RUNTIME_DIR}/pookie-paste"

POOKIE_DAEMON="${POOKIE_BIN_DIR}/pookie-paste"

POOKIE_UI="${POOKIE_BIN_DIR}/pookie-paste-ui"

POOKIE_DESKTOP="${XDG_DATA_HOME}/applications/io.github.riyanj220.PookiePaste.desktop"

POOKIE_AUTOSTART="${XDG_CONFIG_HOME}/autostart/io.github.riyanj220.PookiePaste-autostart.desktop"

POOKIE_DATABASE="${POOKIE_DATA_DIR}/pookie-paste.db"

POOKIE_START_LOG="${POOKIE_STATE_DIR}/install-start.log"

POOKIE_SOCKET="${POOKIE_RUNTIME_DIR}/pookie.sock"

echo "Temporary HOME:"
echo "  ${TEST_HOME}"

echo
echo "Graphical session runtime:"
echo "  ${XDG_RUNTIME_DIR}"

if [[ "$FROM_SOURCE" == true ]]; then
    echo
    echo "Rust toolchain:"
    echo "  CARGO_HOME=${CARGO_HOME}"
    echo "  RUSTUP_HOME=${RUSTUP_HOME}"
fi

echo

if [[ "$FROM_SOURCE" == true ]]; then
    echo "Smoke-test mode:"
    echo "  source build"
else
    echo "Smoke-test mode:"
    echo "  prebuilt bootstrap release"
    echo
    echo "Requested version:"
    echo "  ${REQUESTED_VERSION}"
fi

echo
echo "--------------------------------------------------"
echo "STEP 1 — Fresh installation"
echo "--------------------------------------------------"
echo

if [[ "$FROM_SOURCE" == true ]]; then
    "${PROJECT_ROOT}/scripts/install.sh" \
        --from-source
else
    "${PROJECT_ROOT}/install.sh" \
        --version "$REQUESTED_VERSION"
fi

echo
echo "Verifying fresh installation..."

assert_executable_exists \
    "$POOKIE_DAEMON" \
    "daemon binary installed"

assert_executable_exists \
    "$POOKIE_UI" \
    "UI binary installed"

assert_file_exists \
    "$POOKIE_DESKTOP" \
    "desktop entry installed"

assert_file_exists \
    "$POOKIE_AUTOSTART" \
    "autostart entry installed"

assert_directory_exists \
    "$POOKIE_DATA_DIR" \
    "application data directory created"

assert_directory_exists \
    "$POOKIE_STATE_DIR" \
    "application state directory created"

assert_file_exists \
    "$POOKIE_START_LOG" \
    "startup log created"

if ! wait_for_daemon 10; then
    fail "Pookie Paste daemon did not remain running after installation."
fi

pass "daemon is running"

assert_file_exists \
    "$POOKIE_DATABASE" \
    "SQLite database created"

assert_socket_exists \
    "$POOKIE_SOCKET" \
    "IPC socket created"

echo
echo "--------------------------------------------------"
echo "STEP 2 — Standard uninstall"
echo "--------------------------------------------------"
echo

"${PROJECT_ROOT}/scripts/uninstall.sh"

echo
echo "Verifying standard uninstall..."

assert_path_missing \
    "$POOKIE_DAEMON" \
    "daemon binary removed"

assert_path_missing \
    "$POOKIE_UI" \
    "UI binary removed"

assert_path_missing \
    "$POOKIE_DESKTOP" \
    "desktop entry removed"

assert_path_missing \
    "$POOKIE_AUTOSTART" \
    "autostart entry removed"

if pookie_running; then
    fail "Pookie Paste daemon is still running after uninstall."
fi

pass "daemon stopped"

assert_file_exists \
    "$POOKIE_DATABASE" \
    "database preserved after uninstall"

assert_directory_exists \
    "$POOKIE_STATE_DIR" \
    "application state preserved after uninstall"

echo
echo "--------------------------------------------------"
echo "STEP 3 — Reinstallation"
echo "--------------------------------------------------"
echo

if [[ "$FROM_SOURCE" == true ]]; then
    "${PROJECT_ROOT}/scripts/install.sh" \
        --from-source
else
    "${PROJECT_ROOT}/install.sh" \
        --version "$REQUESTED_VERSION"
fi

echo
echo "Verifying reinstallation..."

assert_executable_exists \
    "$POOKIE_DAEMON" \
    "daemon binary reinstalled"

assert_executable_exists \
    "$POOKIE_UI" \
    "UI binary reinstalled"

assert_file_exists \
    "$POOKIE_DATABASE" \
    "existing database preserved across reinstall"

if ! wait_for_daemon 10; then
    fail "Pookie Paste daemon did not remain running after reinstallation."
fi

pass "daemon running after reinstall"

assert_socket_exists \
    "$POOKIE_SOCKET" \
    "IPC socket restored after reinstall"

echo
echo "--------------------------------------------------"
echo "STEP 4 — Purge uninstall"
echo "--------------------------------------------------"
echo

"${PROJECT_ROOT}/scripts/uninstall.sh" \
    --purge

echo
echo "Verifying purge..."

assert_path_missing \
    "$POOKIE_DAEMON" \
    "daemon binary removed by purge"

assert_path_missing \
    "$POOKIE_UI" \
    "UI binary removed by purge"

assert_path_missing \
    "$POOKIE_DATA_DIR" \
    "application data removed by purge"

assert_path_missing \
    "$POOKIE_STATE_DIR" \
    "application state removed by purge"

if pookie_running; then
    fail "Pookie Paste daemon is still running after purge."
fi

pass "daemon stopped after purge"

echo
echo "=================================================="
echo "POOKIE PASTE SMOKE TEST PASSED"
echo "=================================================="
echo
echo "Validated:"
echo "  fresh installation"
echo "  daemon startup"
echo "  IPC socket creation"
echo "  desktop integration"
echo "  autostart integration"
echo "  database creation"
echo "  standard uninstall"
echo "  user-data preservation"
echo "  reinstallation"
echo "  purge uninstall"
echo
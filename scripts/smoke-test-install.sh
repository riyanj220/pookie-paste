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

    ./scripts/smoke-test-install.sh \\
      --version v0.1.1

  Test installation from source:

    ./scripts/smoke-test-install.sh \\
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

assert_nonempty_file() {
    local path="$1"
    local description="$2"

    if [[ ! -f "$path" ]]; then
        fail "${description} does not exist: ${path}"
    fi

    if [[ ! -s "$path" ]]; then
        fail "${description} is empty: ${path}"
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

assert_file_contains_exact_line() {
    local file="$1"
    local expected="$2"
    local description="$3"

    if ! grep -Fxq "$expected" "$file"; then
        fail "${description} is missing line '${expected}' in ${file}"
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

assert_valid_shortcut_status() {
    local daemon_path="$1"
    local raw_output

    if ! raw_output="$("$daemon_path" --shortcut-status --porcelain 2>/dev/null)"; then
        fail "Pookie Paste daemon failed to respond to --shortcut-status --porcelain request."
    fi

    local parsed_status=""
    local parsed_shortcut=""
    local key val

    while IFS='=' read -r key val || [[ -n "$key" ]]; do
        if [[ -z "$key" && -z "$val" ]]; then
            continue
        fi

        case "$key" in
            status)
                parsed_status="$val"
                ;;
            shortcut)
                parsed_shortcut="$val"
                ;;
        esac
    done <<< "$raw_output"

    case "$parsed_status" in
        ready|needs_setup|conflict|bound_unverified|unavailable)
            ;;
        *)
            fail "Unrecognized shortcut status: '${parsed_status}' in output:\n${raw_output}"
            ;;
    esac

    if [[ -z "$parsed_shortcut" ]]; then
        fail "Shortcut description is missing in output:\n${raw_output}"
    fi

    pass "shortcut status porcelain output is structurally valid (${parsed_status}, ${parsed_shortcut})"
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

# --------------------------------------------------
# Phase 1: Preflight checks & environment isolation
# --------------------------------------------------
echo "Phase 1 — Preflight & environment isolation"

if [[ "$(uname -s)" != "Linux" ]]; then
    fail "The smoke test currently supports Linux only."
fi

if [[ -z "${DISPLAY:-}" \
    && -z "${WAYLAND_DISPLAY:-}" ]]
then
    fail "No graphical Linux session was detected. Run the smoke test from X11 or Wayland."
fi

if [[ -z "$REAL_XDG_RUNTIME_DIR" \
    || ! -d "$REAL_XDG_RUNTIME_DIR" ]]
then
    fail "A valid XDG_RUNTIME_DIR is required for graphical-session testing."
fi

if pookie_running; then
    fail "A Pookie Paste daemon is already running. Stop your normal Pookie instance before running this smoke test."
fi

if [[ ! -x "${PROJECT_ROOT}/install.sh" ]]; then
    fail "Root bootstrap installer is missing or not executable: ${PROJECT_ROOT}/install.sh"
fi

if [[ ! -x "${PROJECT_ROOT}/scripts/install.sh" ]]; then
    fail "Internal installer is missing or not executable: ${PROJECT_ROOT}/scripts/install.sh"
fi

if [[ ! -x "${PROJECT_ROOT}/uninstall.sh" ]]; then
    fail "Root bootstrap uninstaller is missing or not executable: ${PROJECT_ROOT}/uninstall.sh"
fi

if [[ ! -x "${PROJECT_ROOT}/scripts/uninstall.sh" ]]; then
    fail "Internal uninstaller is missing or not executable: ${PROJECT_ROOT}/scripts/uninstall.sh"
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

export HOME="$TEST_HOME"
export XDG_DATA_HOME="${TEST_HOME}/.local/share"
export XDG_CONFIG_HOME="${TEST_HOME}/.config"
export XDG_STATE_HOME="${TEST_HOME}/.local/state"
export XDG_RUNTIME_DIR="$REAL_XDG_RUNTIME_DIR"

if [[ "$FROM_SOURCE" == true ]]; then
    export CARGO_HOME="$REAL_CARGO_HOME"
    export RUSTUP_HOME="$REAL_RUSTUP_HOME"
    export PATH="${CARGO_HOME}/bin:${PATH}"
fi

export XDG_CURRENT_DESKTOP="PookieSmokeTest"
export POOKIE_SKIP_ONBOARDING=1
export PATH="${TEST_HOME}/.local/bin:${PATH}"

# shellcheck disable=SC1091
source "${PROJECT_ROOT}/scripts/lib/paths.sh"

POOKIE_RUNTIME_DIR="${XDG_RUNTIME_DIR}/pookie-paste"
POOKIE_SOCKET="${POOKIE_RUNTIME_DIR}/pookie.sock"
POOKIE_DATABASE="${POOKIE_DATA_DIR}/pookie-paste.db"
POOKIE_CONFIG_FILE="${POOKIE_CONFIG_DIR}/config.toml"
POOKIE_START_LOG="${POOKIE_STATE_DIR}/install-start.log"

POOKIE_ICON_FILES=(
    "${POOKIE_ICONS_DIR}/scalable/apps/${POOKIE_APP_ID}.svg"
    "${POOKIE_ICONS_DIR}/256x256/apps/${POOKIE_APP_ID}.png"
    "${POOKIE_ICONS_DIR}/128x128/apps/${POOKIE_APP_ID}.png"
    "${POOKIE_ICONS_DIR}/64x64/apps/${POOKIE_APP_ID}.png"
    "${POOKIE_ICONS_DIR}/48x48/apps/${POOKIE_APP_ID}.png"
    "${POOKIE_ICONS_DIR}/32x32/apps/${POOKIE_APP_ID}.png"
)

assert_icons_installed() {
    local icon_path
    for icon_path in "${POOKIE_ICON_FILES[@]}"; do
        assert_nonempty_file "$icon_path" "icon installed: $(basename "$(dirname "$icon_path")")/$(basename "$icon_path")"
    done
}

assert_icons_missing() {
    local icon_path
    for icon_path in "${POOKIE_ICON_FILES[@]}"; do
        assert_path_missing "$icon_path" "icon removed: $(basename "$(dirname "$icon_path")")/$(basename "$icon_path")"
    done
}

pass "preflight checks and isolated test environment initialized"

# --------------------------------------------------
# Phase 2: Fresh installation
# --------------------------------------------------
echo
echo "Phase 2 — Fresh installation"

if [[ "$FROM_SOURCE" == true ]]; then
    "${PROJECT_ROOT}/scripts/install.sh" \
        --from-source
else
    "${PROJECT_ROOT}/install.sh" \
        --version "$REQUESTED_VERSION"
fi

assert_executable_exists "$POOKIE_DAEMON_DEST" "daemon binary installed"
assert_executable_exists "$POOKIE_UI_DEST" "UI binary installed"
assert_file_exists "$POOKIE_DESKTOP_DEST" "desktop entry installed"
assert_file_exists "$POOKIE_AUTOSTART_DEST" "autostart entry installed"

assert_file_contains_exact_line "$POOKIE_DESKTOP_DEST" "Name=Pookie Paste" "desktop entry has Name=Pookie Paste"
assert_file_contains_exact_line "$POOKIE_DESKTOP_DEST" "Exec=pookie-paste --toggle" "desktop entry has Exec=pookie-paste --toggle"
assert_file_contains_exact_line "$POOKIE_DESKTOP_DEST" "Icon=io.github.riyanj220.PookiePaste" "desktop entry has Icon=io.github.riyanj220.PookiePaste"
assert_file_contains_exact_line "$POOKIE_DESKTOP_DEST" "StartupWMClass=io.github.riyanj220.PookiePaste" "desktop entry has StartupWMClass=io.github.riyanj220.PookiePaste"

assert_file_contains_exact_line "$POOKIE_AUTOSTART_DEST" "Exec=pookie-paste" "autostart entry has Exec=pookie-paste"
assert_file_contains_exact_line "$POOKIE_AUTOSTART_DEST" "NoDisplay=true" "autostart entry has NoDisplay=true"

assert_icons_installed

assert_directory_exists "$POOKIE_DATA_DIR" "application data directory created"
assert_directory_exists "$POOKIE_STATE_DIR" "application state directory created"
assert_directory_exists "$POOKIE_CONFIG_DIR" "application config directory created"
assert_file_exists "$POOKIE_START_LOG" "startup log created"

# --------------------------------------------------
# Phase 3: Runtime readiness & IPC validation
# --------------------------------------------------
echo
echo "Phase 3 — Runtime readiness & IPC validation"

if ! wait_for_daemon 10; then
    fail "Pookie Paste daemon did not remain running after installation."
fi
pass "daemon process is running"

assert_file_exists "$POOKIE_DATABASE" "SQLite database created on startup"
assert_file_exists "$POOKIE_CONFIG_FILE" "default configuration bootstrapped on startup"
assert_socket_exists "$POOKIE_SOCKET" "IPC socket active"
assert_valid_shortcut_status "$POOKIE_DAEMON_DEST"

# --------------------------------------------------
# Phase 4: Persistence sentinels placement
# --------------------------------------------------
echo
echo "Phase 4 — Persistence sentinels placement"

DATA_SENTINEL="${POOKIE_DATA_DIR}/.smoke-data-sentinel"
STATE_SENTINEL="${POOKIE_STATE_DIR}/.smoke-state-sentinel"
CONFIG_SENTINEL="${POOKIE_CONFIG_DIR}/.smoke-config-sentinel"

touch "$DATA_SENTINEL"
touch "$STATE_SENTINEL"
touch "$CONFIG_SENTINEL"

pass "sentinel files placed in data, state, and config directories"

# --------------------------------------------------
# Phase 5: Standard uninstall (via root wrapper)
# --------------------------------------------------
echo
echo "Phase 5 — Standard uninstall (delegation via root uninstall.sh)"

"${PROJECT_ROOT}/uninstall.sh"

assert_path_missing "$POOKIE_DAEMON_DEST" "daemon binary removed"
assert_path_missing "$POOKIE_UI_DEST" "UI binary removed"
assert_path_missing "$POOKIE_DESKTOP_DEST" "desktop entry removed"
assert_path_missing "$POOKIE_AUTOSTART_DEST" "autostart entry removed"
assert_icons_missing

if pookie_running; then
    fail "Pookie Paste daemon is still running after uninstall."
fi
pass "daemon process stopped"

assert_file_exists "$POOKIE_DATABASE" "database preserved after standard uninstall"
assert_file_exists "$POOKIE_CONFIG_FILE" "config.toml preserved after standard uninstall"
assert_directory_exists "$POOKIE_DATA_DIR" "data directory preserved after standard uninstall"
assert_directory_exists "$POOKIE_STATE_DIR" "state directory preserved after standard uninstall"
assert_directory_exists "$POOKIE_CONFIG_DIR" "config directory preserved after standard uninstall"
assert_file_exists "$DATA_SENTINEL" "data sentinel preserved"
assert_file_exists "$STATE_SENTINEL" "state sentinel preserved"
assert_file_exists "$CONFIG_SENTINEL" "config sentinel preserved"

# --------------------------------------------------
# Phase 6: Reinstallation
# --------------------------------------------------
echo
echo "Phase 6 — Reinstallation & persistence verification"

if [[ "$FROM_SOURCE" == true ]]; then
    "${PROJECT_ROOT}/scripts/install.sh" \
        --from-source
else
    "${PROJECT_ROOT}/install.sh" \
        --version "$REQUESTED_VERSION"
fi

assert_executable_exists "$POOKIE_DAEMON_DEST" "daemon binary reinstalled"
assert_executable_exists "$POOKIE_UI_DEST" "UI binary reinstalled"
assert_file_exists "$POOKIE_DESKTOP_DEST" "desktop entry restored"
assert_file_exists "$POOKIE_AUTOSTART_DEST" "autostart entry restored"
assert_icons_installed

if ! wait_for_daemon 10; then
    fail "Pookie Paste daemon did not become active after reinstallation."
fi
pass "daemon process active after reinstall"

assert_socket_exists "$POOKIE_SOCKET" "IPC socket active after reinstall"
assert_file_exists "$POOKIE_DATABASE" "database intact across reinstall"
assert_file_exists "$POOKIE_CONFIG_FILE" "config.toml intact across reinstall"
assert_file_exists "$DATA_SENTINEL" "data sentinel intact across reinstall"
assert_file_exists "$STATE_SENTINEL" "state sentinel intact across reinstall"
assert_file_exists "$CONFIG_SENTINEL" "config sentinel intact across reinstall"
assert_valid_shortcut_status "$POOKIE_DAEMON_DEST"

# --------------------------------------------------
# Phase 7: Purge uninstall (via root wrapper --purge)
# --------------------------------------------------
echo
echo "Phase 7 — Purge uninstall (delegation via root uninstall.sh --purge)"

"${PROJECT_ROOT}/uninstall.sh" \
    --purge

assert_path_missing "$POOKIE_DAEMON_DEST" "daemon binary removed by purge"
assert_path_missing "$POOKIE_UI_DEST" "UI binary removed by purge"
assert_path_missing "$POOKIE_DESKTOP_DEST" "desktop entry removed by purge"
assert_path_missing "$POOKIE_AUTOSTART_DEST" "autostart entry removed by purge"
assert_icons_missing

if pookie_running; then
    fail "Pookie Paste daemon is still running after purge."
fi
pass "daemon process stopped by purge"

assert_path_missing "$POOKIE_DATA_DIR" "data directory removed by purge"
assert_path_missing "$POOKIE_STATE_DIR" "state directory removed by purge"
assert_path_missing "$POOKIE_CONFIG_DIR" "config directory removed by purge"
assert_path_missing "$DATA_SENTINEL" "data sentinel removed by purge"
assert_path_missing "$STATE_SENTINEL" "state sentinel removed by purge"
assert_path_missing "$CONFIG_SENTINEL" "config sentinel removed by purge"
assert_path_missing "$POOKIE_DATABASE" "database removed by purge"
assert_path_missing "$POOKIE_CONFIG_FILE" "config.toml removed by purge"

# --------------------------------------------------
# Phase 8: Idempotent uninstall
# --------------------------------------------------
echo
echo "Phase 8 — Idempotent uninstall verification"

"${PROJECT_ROOT}/scripts/uninstall.sh"
pass "re-running uninstaller on clean state succeeded without error"

"${PROJECT_ROOT}/scripts/uninstall.sh" --purge
pass "re-running purge uninstaller on clean state succeeded without error"

echo
echo "=================================================="
echo "POOKIE PASTE SMOKE TEST PASSED"
echo "=================================================="
echo
echo "Validated lifecycle:"
echo "  Phase 1: Preflight checks & environment isolation"
echo "  Phase 2: Fresh installation (binaries, desktop, autostart, icons, directories)"
echo "  Phase 3: Runtime readiness & IPC validation (database, config.toml, porcelain status)"
echo "  Phase 4: Persistence sentinels placement"
echo "  Phase 5: Standard uninstall via ./uninstall.sh (artifact removal & data/config preservation)"
echo "  Phase 6: Reinstallation & persistence verification"
echo "  Phase 7: Purge uninstall via ./uninstall.sh --purge (complete clean slate)"
echo "  Phase 8: Idempotent uninstaller verification"
echo
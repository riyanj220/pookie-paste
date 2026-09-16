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

INSTALL_FIRST=false

POOKIE_APP_ID="io.github.riyanj220.PookiePaste"

POOKIE_KWIN_PLUGIN_ID="pookie-focus"

POOKIE_BIN_DIR="${HOME}/.local/bin"

POOKIE_DATA_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}"

POOKIE_CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"

POOKIE_STATE_HOME="${XDG_STATE_HOME:-${HOME}/.local/state}"

POOKIE_RUNTIME_HOME="${XDG_RUNTIME_DIR:-}"

POOKIE_DAEMON="${POOKIE_BIN_DIR}/pookie-paste"

POOKIE_UI="${POOKIE_BIN_DIR}/pookie-paste-ui"

POOKIE_DESKTOP="${POOKIE_DATA_HOME}/applications/${POOKIE_APP_ID}.desktop"

POOKIE_AUTOSTART="${POOKIE_CONFIG_HOME}/autostart/${POOKIE_APP_ID}-autostart.desktop"

POOKIE_DATA_DIR="${POOKIE_DATA_HOME}/pookie-paste"

POOKIE_STATE_DIR="${POOKIE_STATE_HOME}/pookie-paste"

POOKIE_DATABASE="${POOKIE_DATA_DIR}/pookie-paste.db"

POOKIE_RESTORE_TOKEN="${POOKIE_STATE_DIR}/remote-desktop.restore-token"

POOKIE_START_LOG="${POOKIE_STATE_DIR}/install-start.log"

POOKIE_SOCKET="${POOKIE_RUNTIME_HOME}/pookie-paste/pookie.sock"

POOKIE_KWIN_DIR="${POOKIE_DATA_HOME}/kwin/scripts/${POOKIE_KWIN_PLUGIN_ID}"

PASS_COUNT=0

WARN_COUNT=0

usage() {
    cat <<EOF
Pookie Paste KDE Plasma Wayland validation

Usage:
  ./scripts/validate-kde-wayland.sh [options]

Options:
  --install
      Install or update Pookie Paste before validation.

  --version <version>
      Version used with --install.

      Examples:
        --version v0.1.1
        --version latest

  -h, --help
      Show this help message.

Examples:
  Validate an existing installation:

    ./scripts/validate-kde-wayland.sh

  Install and validate v0.1.1:

    ./scripts/validate-kde-wayland.sh \
      --install \
      --version v0.1.1

  Install and validate latest:

    ./scripts/validate-kde-wayland.sh \
      --install
EOF
}

fail() {
    echo
    echo "VALIDATION FAILED"
    echo "================="
    echo
    echo "$1" >&2
    exit 1
}

pass() {
    PASS_COUNT=$((PASS_COUNT + 1))
    echo "  [PASS] $1"
}

warn() {
    WARN_COUNT=$((WARN_COUNT + 1))
    echo "  [WARN] $1"
}

require_command() {
    local command_name="$1"

    if ! command -v "$command_name" >/dev/null 2>&1; then
        fail "Required command is unavailable: ${command_name}"
    fi

    pass "command available: ${command_name}"
}

assert_file() {
    local path="$1"
    local description="$2"

    if [[ ! -f "$path" ]]; then
        fail "${description} is missing: ${path}"
    fi

    pass "$description"
}

assert_executable() {
    local path="$1"
    local description="$2"

    if [[ ! -x "$path" ]]; then
        fail "${description} is missing or not executable: ${path}"
    fi

    pass "$description"
}

assert_directory() {
    local path="$1"
    local description="$2"

    if [[ ! -d "$path" ]]; then
        fail "${description} is missing: ${path}"
    fi

    pass "$description"
}

assert_socket() {
    local path="$1"
    local description="$2"

    if [[ ! -S "$path" ]]; then
        fail "${description} is missing: ${path}"
    fi

    pass "$description"
}

pookie_running() {
    pgrep \
        -u "$(id -u)" \
        -x pookie-paste \
        >/dev/null 2>&1
}

wait_for_pookie() {
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

find_qdbus() {
    if command -v qdbus6 >/dev/null 2>&1; then
        printf '%s\n' "qdbus6"
        return 0
    fi

    if command -v qdbus-qt6 >/dev/null 2>&1; then
        printf '%s\n' "qdbus-qt6"
        return 0
    fi

    if command -v qdbus >/dev/null 2>&1; then
        printf '%s\n' "qdbus"
        return 0
    fi

    return 1
}

kwin_helper_listed() {
    kpackagetool6 \
        --type=KWin/Script \
        --list 2>/dev/null |
        grep -Fxq "$POOKIE_KWIN_PLUGIN_ID"
}

kwin_helper_enabled() {
    local value

    value="$(
        kreadconfig6 \
            --file kwinrc \
            --group Plugins \
            --key "${POOKIE_KWIN_PLUGIN_ID}Enabled" \
            2>/dev/null || true
    )"

    [[ "${value,,}" == "true" ]]
}

check_dbus_service_owner() {
    if ! command -v busctl >/dev/null 2>&1; then
        warn "busctl is unavailable; skipping Pookie D-Bus ownership check."
        return 0
    fi

    if busctl \
        --user \
        --no-pager \
        --no-legend \
        list 2>/dev/null |
        awk '{print $1}' |
        grep -Fxq "$POOKIE_APP_ID"
    then
        pass "Pookie focus D-Bus service is owned"
    else
        warn \
            "Pookie D-Bus service ${POOKIE_APP_ID} was not visible through busctl."
    fi
}

check_global_shortcuts_portal() {
    if command -v gdbus >/dev/null 2>&1; then
        local result

        result="$(
            gdbus call \
                --session \
                --dest org.freedesktop.portal.Desktop \
                --object-path /org/freedesktop/portal/desktop \
                --method org.freedesktop.DBus.Properties.Get \
                org.freedesktop.portal.GlobalShortcuts \
                version \
                2>/dev/null || true
        )"

        if [[ -n "$result" ]]; then
            pass "XDG GlobalShortcuts portal is available"
            echo "         ${result}"
        else
            fail "XDG GlobalShortcuts portal could not be queried."
        fi

        return 0
    fi

    if command -v busctl >/dev/null 2>&1; then
        if busctl \
            --user \
            introspect \
            org.freedesktop.portal.Desktop \
            /org/freedesktop/portal/desktop \
            org.freedesktop.portal.GlobalShortcuts \
            >/dev/null 2>&1
        then
            pass "XDG GlobalShortcuts portal is available"
            return 0
        fi
    fi

    fail "Unable to verify the XDG GlobalShortcuts portal."
}

check_remote_desktop_portal() {
    if command -v gdbus >/dev/null 2>&1; then
        local result

        result="$(
            gdbus call \
                --session \
                --dest org.freedesktop.portal.Desktop \
                --object-path /org/freedesktop/portal/desktop \
                --method org.freedesktop.DBus.Properties.Get \
                org.freedesktop.portal.RemoteDesktop \
                version \
                2>/dev/null || true
        )"

        if [[ -n "$result" ]]; then
            pass "XDG RemoteDesktop portal is available"
            echo "         ${result}"
        else
            fail "XDG RemoteDesktop portal could not be queried."
        fi

        return 0
    fi

    if command -v busctl >/dev/null 2>&1; then
        if busctl \
            --user \
            introspect \
            org.freedesktop.portal.Desktop \
            /org/freedesktop/portal/desktop \
            org.freedesktop.portal.RemoteDesktop \
            >/dev/null 2>&1
        then
            pass "XDG RemoteDesktop portal is available"
            return 0
        fi
    fi

    fail "Unable to verify the XDG RemoteDesktop portal."
}

check_restore_token_permissions() {
    if [[ ! -f "$POOKIE_RESTORE_TOKEN" ]]; then
        warn \
            "RemoteDesktop restore token does not exist yet. It may be created after the first successful direct-paste authorization."
        return 0
    fi

    local mode

    mode="$(
        stat \
            -c '%a' \
            "$POOKIE_RESTORE_TOKEN"
    )"

    if [[ "$mode" != "600" ]]; then
        fail \
            "RemoteDesktop restore token has insecure permissions: ${mode}"
    fi

    pass "RemoteDesktop restore token permissions are 600"
}

manual_check() {
    local prompt="$1"
    local answer

    while true; do
        read \
            -r \
            -p "${prompt} [y/n]: " \
            answer

        case "${answer,,}" in
            y|yes)
                pass "$prompt"
                return 0
                ;;

            n|no)
                fail "Manual validation failed: ${prompt}"
                ;;

            *)
                echo "Please answer y or n."
                ;;
        esac
    done
}

while (( $# > 0 )); do
    case "$1" in
        --install)
            INSTALL_FIRST=true
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

echo
echo "Pookie Paste KDE Plasma Wayland validation"
echo "=========================================="
echo

if [[ "$(uname -s)" != "Linux" ]]; then
    fail "This validation currently supports Linux only."
fi

if [[ "${XDG_SESSION_TYPE:-}" != "wayland" ]]; then
    fail \
        "Expected a Wayland session, found: ${XDG_SESSION_TYPE:-unset}"
fi

case ":${XDG_CURRENT_DESKTOP,,}:" in
    *:kde:*|*:plasma:*)
        pass "KDE Plasma desktop detected"
        ;;

    *)
        fail \
            "Expected KDE Plasma, found: ${XDG_CURRENT_DESKTOP:-unset}"
        ;;
esac

pass "Wayland session detected"

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
    fail "WAYLAND_DISPLAY is not set."
fi

pass "Wayland display available: ${WAYLAND_DISPLAY}"

if [[ -z "${XDG_RUNTIME_DIR:-}" \
    || ! -d "${XDG_RUNTIME_DIR}" ]]
then
    fail "XDG_RUNTIME_DIR is unavailable."
fi

pass "graphical runtime directory available: ${XDG_RUNTIME_DIR}"

echo
echo "Environment:"
echo "  Session:  ${XDG_SESSION_TYPE}"
echo "  Desktop:  ${XDG_CURRENT_DESKTOP}"
echo "  Wayland:  ${WAYLAND_DISPLAY}"
echo "  Runtime:  ${XDG_RUNTIME_DIR}"

if command -v plasmashell >/dev/null 2>&1; then
    echo "  Plasma:   $(plasmashell --version 2>/dev/null || true)"
fi

if command -v kwin_wayland >/dev/null 2>&1; then
    echo "  KWin:     $(kwin_wayland --version 2>/dev/null || true)"
fi

echo
echo "--------------------------------------------------"
echo "STEP 1 — Required KDE/session tools"
echo "--------------------------------------------------"
echo

require_command kpackagetool6
require_command kwriteconfig6
require_command kreadconfig6
require_command pgrep
require_command stat

QDBUS_COMMAND="$(
    find_qdbus
)" || fail "No Qt D-Bus command was found."

pass "Qt D-Bus utility available: ${QDBUS_COMMAND}"

echo
echo "--------------------------------------------------"
echo "STEP 2 — Install/update release"
echo "--------------------------------------------------"
echo

if [[ "$INSTALL_FIRST" == true ]]; then
    if [[ ! -x "${PROJECT_ROOT}/install.sh" ]]; then
        fail \
            "Root bootstrap installer is missing or not executable."
    fi

    echo "Installing:"
    echo "  ${REQUESTED_VERSION}"
    echo

    "${PROJECT_ROOT}/install.sh" \
        --version "$REQUESTED_VERSION"
else
    echo "Using the currently installed Pookie Paste release."
fi

echo
echo "--------------------------------------------------"
echo "STEP 3 — Installed files"
echo "--------------------------------------------------"
echo

assert_executable \
    "$POOKIE_DAEMON" \
    "Pookie daemon installed"

assert_executable \
    "$POOKIE_UI" \
    "Pookie UI installed"

assert_file \
    "$POOKIE_DESKTOP" \
    "desktop entry installed"

assert_file \
    "$POOKIE_AUTOSTART" \
    "autostart entry installed"

assert_directory \
    "$POOKIE_DATA_DIR" \
    "application data directory exists"

assert_directory \
    "$POOKIE_STATE_DIR" \
    "application state directory exists"

echo
echo "--------------------------------------------------"
echo "STEP 4 — KDE/KWin integration"
echo "--------------------------------------------------"
echo

if ! kwin_helper_listed; then
    fail \
        "KWin helper ${POOKIE_KWIN_PLUGIN_ID} is not installed."
fi

pass "KWin helper is listed by kpackagetool6"

assert_file \
    "${POOKIE_KWIN_DIR}/metadata.json" \
    "KWin helper metadata installed"

assert_file \
    "${POOKIE_KWIN_DIR}/contents/code/main.js" \
    "KWin helper script installed"

if ! kwin_helper_enabled; then
    fail "KWin helper is installed but not enabled."
fi

pass "KWin helper is enabled"

echo
echo "--------------------------------------------------"
echo "STEP 5 — Portal availability"
echo "--------------------------------------------------"
echo

check_global_shortcuts_portal

check_remote_desktop_portal

echo
echo "--------------------------------------------------"
echo "STEP 6 — Daemon/runtime"
echo "--------------------------------------------------"
echo

if ! pookie_running; then
    echo "Pookie Paste is not currently running."
    echo "Starting installed daemon..."
    echo

    mkdir -p "$POOKIE_STATE_DIR"

    nohup "$POOKIE_DAEMON" \
        >>"$POOKIE_START_LOG" \
        2>&1 &

    if ! wait_for_pookie 10; then
        echo
        echo "Last startup log lines:"
        echo

        tail \
            -80 \
            "$POOKIE_START_LOG" \
            2>/dev/null || true

        fail "Pookie Paste daemon failed to start."
    fi
fi

pass "Pookie daemon is running"

assert_file \
    "$POOKIE_DATABASE" \
    "SQLite database exists"

assert_socket \
    "$POOKIE_SOCKET" \
    "IPC socket exists"

check_dbus_service_owner

check_restore_token_permissions

echo
echo "--------------------------------------------------"
echo "STEP 7 — Manual KDE Wayland integration test"
echo "--------------------------------------------------"
echo
echo "Do the following now:"
echo
echo "  1. Open a text editor or terminal with an editable field."
echo
echo "  2. Copy a unique piece of text."
echo "  3. Keep that application focused."
echo "  4. Press Super+V."
echo "  5. Confirm the Pookie popup opens."
echo "  6. Select the copied history item."
echo "  7. Confirm the original window becomes active again."
echo "  8. Confirm the selected text is pasted automatically."
echo

manual_check \
    "Super+V opened the Pookie Paste popup"

manual_check \
    "The copied item appeared in clipboard history"

manual_check \
    "Selecting the item restored the original KDE window"

manual_check \
    "The selected item was pasted directly into the original application"

echo
echo "--------------------------------------------------"
echo "STEP 8 — Post-direct-paste security/state checks"
echo "--------------------------------------------------"
echo

check_restore_token_permissions

echo
echo "Recent Pookie log:"
echo

tail \
    -40 \
    "$POOKIE_START_LOG" \
    2>/dev/null || true

echo
echo "=================================================="
echo "FEDORA KDE WAYLAND VALIDATION PASSED"
echo "=================================================="
echo
echo "Automatic/manual checks passed: ${PASS_COUNT}"

if (( WARN_COUNT > 0 )); then
    echo "Warnings:                       ${WARN_COUNT}"
else
    echo "Warnings:                       0"
fi

echo
echo "Validated:"
echo "  KDE Plasma Wayland session"
echo "  production binaries"
echo "  desktop integration"
echo "  autostart integration"
echo "  KWin focus helper installation"
echo "  KWin focus helper enablement"
echo "  XDG GlobalShortcuts portal"
echo "  XDG RemoteDesktop portal"
echo "  daemon startup"
echo "  SQLite storage"
echo "  IPC socket"
echo "  Super+V popup activation"
echo "  clipboard history"
echo "  KDE focus restoration"
echo "  Portal/EIS direct paste"
echo "  restore-token permissions"
echo

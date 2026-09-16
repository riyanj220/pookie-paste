#!/usr/bin/env bash

POOKIE_APP_ID="io.github.riyanj220.PookiePaste"

POOKIE_BIN_DIR="${HOME}/.local/bin"

POOKIE_DATA_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}"

POOKIE_CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"

POOKIE_STATE_HOME="${XDG_STATE_HOME:-${HOME}/.local/state}"

POOKIE_APPLICATIONS_DIR="${POOKIE_DATA_HOME}/applications"

POOKIE_AUTOSTART_DIR="${POOKIE_CONFIG_HOME}/autostart"

POOKIE_DATA_DIR="${POOKIE_DATA_HOME}/pookie-paste"

POOKIE_STATE_DIR="${POOKIE_STATE_HOME}/pookie-paste"

POOKIE_DAEMON_DEST="${POOKIE_BIN_DIR}/pookie-paste"

POOKIE_UI_DEST="${POOKIE_BIN_DIR}/pookie-paste-ui"

POOKIE_DESKTOP_DEST="${POOKIE_APPLICATIONS_DIR}/${POOKIE_APP_ID}.desktop"

POOKIE_AUTOSTART_DEST="${POOKIE_AUTOSTART_DIR}/${POOKIE_APP_ID}-autostart.desktop"

ensure_install_directories() {
    mkdir -p \
        "$POOKIE_BIN_DIR" \
        "$POOKIE_APPLICATIONS_DIR" \
        "$POOKIE_AUTOSTART_DIR" \
        "$POOKIE_DATA_DIR" \
        "$POOKIE_STATE_DIR"
}

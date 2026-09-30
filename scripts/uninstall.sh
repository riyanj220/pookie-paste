#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(
    cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1
    pwd
)"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/paths.sh"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/kde.sh"

# shellcheck disable=SC1091
source "${SCRIPT_DIR}/lib/process.sh"

PURGE=false

usage() {
    cat <<EOF
Usage:
  ./scripts/uninstall.sh
  ./scripts/uninstall.sh --purge

Options:
  --purge    Also remove settings, clipboard history,
             database, and other user data.

  -h, --help Show this help message.
EOF
}

while (( $# > 0 )); do
    case "$1" in
        --purge)
            PURGE=true
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

echo
echo "  ┌─ Pookie Paste"
echo "  │  Uninstall"
echo "  └─"
echo

stop_pookie

rm -f \
    "$POOKIE_DAEMON_DEST" \
    "$POOKIE_UI_DEST" \
    "$POOKIE_DESKTOP_DEST" \
    "$POOKIE_AUTOSTART_DEST" \
    "${POOKIE_ICONS_DIR}/scalable/apps/${POOKIE_APP_ID}.svg" \
    "${POOKIE_ICONS_DIR}/256x256/apps/${POOKIE_APP_ID}.png" \
    "${POOKIE_ICONS_DIR}/128x128/apps/${POOKIE_APP_ID}.png" \
    "${POOKIE_ICONS_DIR}/64x64/apps/${POOKIE_APP_ID}.png" \
    "${POOKIE_ICONS_DIR}/48x48/apps/${POOKIE_APP_ID}.png" \
    "${POOKIE_ICONS_DIR}/32x32/apps/${POOKIE_APP_ID}.png"

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "${POOKIE_ICONS_DIR}" 2>/dev/null || true
fi

uninstall_kwin_helper || true

if [[ "$PURGE" == true ]]; then
    rm -rf \
        "$POOKIE_DATA_DIR" \
        "$POOKIE_STATE_DIR" \
        "$POOKIE_CONFIG_DIR"

    echo "Pookie Paste and all user data have been removed."
else
    echo "Pookie Paste has been uninstalled."
    echo
    echo "Your settings and clipboard history were preserved."
    echo "Use --purge to remove them as well."
fi
echo

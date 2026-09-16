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
  --purge    Also remove Pookie Paste history, database,
             portal authorization state, and other user data.

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
echo "Pookie Paste uninstaller"
echo "========================"
echo

stop_pookie

echo
echo "Removing application files..."

rm -f \
    "$POOKIE_DAEMON_DEST" \
    "$POOKIE_UI_DEST" \
    "$POOKIE_DESKTOP_DEST" \
    "$POOKIE_AUTOSTART_DEST"

echo "Removing KDE integration..."

uninstall_kwin_helper || true

if [[ "$PURGE" == true ]]; then
    echo
    echo "Purging Pookie Paste user data..."

    rm -rf \
        "$POOKIE_DATA_DIR" \
        "$POOKIE_STATE_DIR"

    echo "Pookie Paste user data removed."
else
    echo
    echo "User data was preserved:"
    echo "  $POOKIE_DATA_DIR"
    echo "  $POOKIE_STATE_DIR"
    echo
    echo "Run:"
    echo
    echo "  ./scripts/uninstall.sh --purge"
    echo
    echo "to remove it as well."
fi

echo
echo "Pookie Paste has been uninstalled."

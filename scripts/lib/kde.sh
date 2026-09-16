#!/usr/bin/env bash

POOKIE_KWIN_PLUGIN_ID="pookie-focus"

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

is_kde_session() {
    local desktop="${XDG_CURRENT_DESKTOP:-}"

    case ":${desktop,,}:" in
        *:kde:*|*:plasma:*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

require_kde_tools() {
    local missing=0

    if ! command -v kpackagetool6 >/dev/null 2>&1; then
        echo "Missing required KDE command: kpackagetool6" >&2
        missing=1
    fi

    if ! command -v kwriteconfig6 >/dev/null 2>&1; then
        echo "Missing required KDE command: kwriteconfig6" >&2
        missing=1
    fi

    if ! find_qdbus >/dev/null 2>&1; then
        echo "Missing Qt 6 D-Bus command: qdbus6, qdbus-qt6, or qdbus" >&2
        missing=1
    fi

    if (( missing != 0 )); then
        return 1
    fi
}

kwin_helper_installed() {
    kpackagetool6 \
        --type=KWin/Script \
        --list 2>/dev/null |
        grep -Fxq "$POOKIE_KWIN_PLUGIN_ID"
}

remove_kwin_helper() {
    if ! kwin_helper_installed; then
        return 0
    fi

    echo "Removing existing Pookie Paste KWin helper..."

    kpackagetool6 \
        --type=KWin/Script \
        --remove "$POOKIE_KWIN_PLUGIN_ID"
}

install_kwin_helper() {
    local package_path="$1"

    if [[ ! -f "$package_path/metadata.json" ]]; then
        echo "Invalid KWin helper package: $package_path" >&2
        return 1
    fi

    if [[ ! -f "$package_path/contents/code/main.js" ]]; then
        echo "KWin helper main.js is missing: $package_path" >&2
        return 1
    fi

    require_kde_tools

    #
    # Reinstall instead of assuming kpackagetool6 will update
    # an existing package correctly.
    #
    # This makes install.sh idempotent and ensures that new
    # helper code replaces an older installed version.
    #
    remove_kwin_helper

    echo "Installing Pookie Paste KWin helper..."

    kpackagetool6 \
        --type=KWin/Script \
        --install "$package_path"

    if ! kwin_helper_installed; then
        echo "KWin helper installation could not be verified." >&2
        return 1
    fi
}

enable_kwin_helper() {
    require_kde_tools

    echo "Enabling Pookie Paste KWin helper..."

    kwriteconfig6 \
        --file kwinrc \
        --group Plugins \
        --key "${POOKIE_KWIN_PLUGIN_ID}Enabled" \
        true
}

reconfigure_kwin() {
    local qdbus_command

    qdbus_command="$(find_qdbus)" || {
        echo "Unable to find a Qt D-Bus command." >&2
        return 1
    }

    echo "Reloading KWin configuration..."

    "$qdbus_command" \
        org.kde.KWin \
        /KWin \
        reconfigure
}

install_and_enable_kwin_helper() {
    local package_path="$1"

    if ! is_kde_session; then
        echo "KDE Plasma session not detected; skipping KWin helper."
        return 0
    fi

    install_kwin_helper "$package_path"

    enable_kwin_helper

    reconfigure_kwin

    echo "Pookie Paste KWin focus helper is ready."
}

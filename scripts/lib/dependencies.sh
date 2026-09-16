#!/usr/bin/env bash

install_base_dependencies() {
    local family="$1"

    case "$family" in
        debian)
            sudo apt-get update

            sudo apt-get install -y \
                build-essential \
                curl \
                pkg-config
            ;;

        fedora)
            sudo dnf install -y \
                gcc \
                gcc-c++ \
                make \
                curl \
                pkgconf-pkg-config
            ;;

        arch)
            sudo pacman -S --needed --noconfirm \
                base-devel \
                curl
            ;;

        opensuse)
            sudo zypper --non-interactive install \
                gcc \
                gcc-c++ \
                make \
                curl \
                pkg-config
            ;;

        *)
            echo "Unsupported Linux distribution." >&2
            return 1
            ;;
    esac
}

install_kde_dependencies() {
    local family="$1"

    #
    # Most Plasma installations already provide these.
    # Only install them when a required command is missing.
    #
    if command -v kpackagetool6 >/dev/null 2>&1 \
        && command -v kwriteconfig6 >/dev/null 2>&1 \
        && {
            command -v qdbus6 >/dev/null 2>&1 \
            || command -v qdbus-qt6 >/dev/null 2>&1 \
            || command -v qdbus >/dev/null 2>&1
        }
    then
        return 0
    fi

    echo "Installing KDE helper utilities..."

    case "$family" in
        fedora)
            sudo dnf install -y \
                kf6-kpackage \
                kf6-kconfig \
                qt6-qttools
            ;;

        arch)
            sudo pacman -S --needed --noconfirm \
                kpackage \
                kconfig \
                qt6-tools
            ;;

        debian)
            #
            # Package naming varies more across Debian/Ubuntu
            # Plasma releases. Try the Frameworks 6 package
            # names first.
            #
            sudo apt-get install -y \
                kpackagetool6 \
                kwriteconfig6 \
                qdbus-qt6 2>/dev/null || {
                    echo \
                        "KDE Plasma 6 tools were not found under the expected Debian package names." \
                        >&2
                    return 1
                }
            ;;

        opensuse)
            #
            # Plasma systems normally already have the
            # corresponding KF6/Qt6 tools installed.
            #
            if ! command -v kpackagetool6 >/dev/null 2>&1 \
                || ! command -v kwriteconfig6 >/dev/null 2>&1
            then
                echo \
                    "Required KDE Plasma 6 tools are missing on this openSUSE installation." \
                    >&2
                return 1
            fi
            ;;

        *)
            return 1
            ;;
    esac
}

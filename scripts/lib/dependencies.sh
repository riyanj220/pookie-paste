#!/usr/bin/env bash

install_download_dependencies() {
    local family="$1"

    case "$family" in
        debian)
            sudo apt-get update

            sudo apt-get install -y \
                ca-certificates \
                coreutils \
                curl \
                tar
            ;;

        fedora)
            sudo dnf install -y \
                ca-certificates \
                coreutils \
                curl \
                tar
            ;;

        arch)
            sudo pacman -S --needed --noconfirm \
                ca-certificates \
                coreutils \
                curl \
                tar
            ;;

        opensuse)
            sudo zypper --non-interactive install \
                ca-certificates \
                coreutils \
                curl \
                tar
            ;;

        *)
            echo "Unsupported Linux distribution." >&2
            return 1
            ;;
    esac
}

install_source_dependencies() {
    local family="$1"

    case "$family" in
        debian)
            sudo apt-get update

            sudo apt-get install -y \
                build-essential \
                ca-certificates \
                curl \
                pkg-config
            ;;

        fedora)
            sudo dnf install -y \
                gcc \
                gcc-c++ \
                make \
                ca-certificates \
                curl \
                pkgconf-pkg-config
            ;;

        arch)
            sudo pacman -S --needed --noconfirm \
                base-devel \
                ca-certificates \
                curl
            ;;

        opensuse)
            sudo zypper --non-interactive install \
                gcc \
                gcc-c++ \
                make \
                ca-certificates \
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
            # KDE Plasma 6 package naming differs across
            # Debian and Ubuntu releases.
            #
            # Try the expected command packages first.
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
            # Plasma systems normally already contain these.
            #
            # We intentionally do not guess package names here
            # until the openSUSE packaging path is tested.
            #
            if ! command -v kpackagetool6 >/dev/null 2>&1 \
                || ! command -v kwriteconfig6 >/dev/null 2>&1 \
                || ! {
                    command -v qdbus6 >/dev/null 2>&1 \
                    || command -v qdbus-qt6 >/dev/null 2>&1 \
                    || command -v qdbus >/dev/null 2>&1
                }
            then
                echo \
                    "Required KDE Plasma 6 tools are missing on this openSUSE installation." \
                    >&2
                return 1
            fi
            ;;

        *)
            echo "Unsupported Linux distribution." >&2
            return 1
            ;;
    esac
}

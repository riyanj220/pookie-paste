#!/usr/bin/env bash

detect_distro_family() {
    if [[ ! -r /etc/os-release ]]; then
        echo "unsupported"
        return
    fi

    # shellcheck disable=SC1091
    . /etc/os-release

    local id="${ID:-}"
    local like="${ID_LIKE:-}"

    case "$id" in
        debian|ubuntu|linuxmint|pop|elementary|zorin)
            echo "debian"
            return
            ;;

        fedora|rhel|centos|rocky|almalinux)
            echo "fedora"
            return
            ;;

        arch|manjaro|endeavouros)
            echo "arch"
            return
            ;;

        opensuse*|sles)
            echo "opensuse"
            return
            ;;
    esac

    case "$like" in
        *debian*|*ubuntu*)
            echo "debian"
            ;;

        *fedora*|*rhel*)
            echo "fedora"
            ;;

        *arch*)
            echo "arch"
            ;;

        *suse*)
            echo "opensuse"
            ;;

        *)
            echo "unsupported"
            ;;
    esac
}

#!/usr/bin/env bash

detect_architecture() {
    local architecture="${1:-}"

    if [[ -z "$architecture" ]]; then
        architecture="$(uname -m)"
    fi

    case "$architecture" in
        x86_64|amd64)
            printf '%s\n' "x86_64"
            ;;

        aarch64|arm64)
            printf '%s\n' "aarch64"
            ;;

        *)
            printf '%s\n' "unsupported"
            ;;
    esac
}

release_platform() {
    local architecture="$1"

    case "$architecture" in
        x86_64)
            printf '%s\n' "linux-x86_64"
            ;;

        aarch64)
            printf '%s\n' "linux-aarch64"
            ;;

        *)
            return 1
            ;;
    esac
}

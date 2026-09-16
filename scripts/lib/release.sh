#!/usr/bin/env bash

POOKIE_GITHUB_REPOSITORY="riyanj220/pookie-paste"

POOKIE_GITHUB_BASE_URL="https://github.com/${POOKIE_GITHUB_REPOSITORY}"

POOKIE_RELEASE_WORK_DIR=""

POOKIE_RELEASE_BUNDLE_DIR=""

POOKIE_RESOLVED_VERSION=""

is_valid_release_version() {
    local version="$1"

    [[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]
}

resolve_latest_release_version() {
    local latest_url
    local effective_url
    local version

    latest_url="${POOKIE_GITHUB_BASE_URL}/releases/latest"

    effective_url="$(
        curl \
            --fail \
            --silent \
            --show-error \
            --location \
            --output /dev/null \
            --write-out '%{url_effective}' \
            "$latest_url"
    )"

    version="${effective_url##*/}"

    if ! is_valid_release_version "$version"; then
        echo "Unable to determine the latest Pookie Paste release." >&2
        echo "GitHub resolved to:" >&2
        echo "  ${effective_url}" >&2
        return 1
    fi

    printf '%s\n' "$version"
}

resolve_release_version() {
    local requested_version="${1:-latest}"

    if [[ -z "$requested_version" || "$requested_version" == "latest" ]]; then
        resolve_latest_release_version
        return
    fi

    if ! is_valid_release_version "$requested_version"; then
        echo "Invalid Pookie Paste version: ${requested_version}" >&2
        echo "Expected:" >&2
        echo "  latest" >&2
        echo "  v0.1.0" >&2
        return 1
    fi

    printf '%s\n' "$requested_version"
}

release_asset_name() {
    local version="$1"
    local platform="$2"

    printf '%s\n' \
        "pookie-paste-${version}-${platform}.tar.gz"
}

verify_release_checksum() {
    local work_dir="$1"
    local asset_name="$2"

    local checksums_file="${work_dir}/SHA256SUMS"
    local selected_checksum="${work_dir}/SELECTED_SHA256SUM"

    if [[ ! -f "$checksums_file" ]]; then
        echo "Release checksum file is missing." >&2
        return 1
    fi

    awk \
        -v asset="$asset_name" \
        '
        {
            file = $2
            sub(/^\*/, "", file)

            if (file == asset) {
                print
                found = 1
            }
        }

        END {
            if (!found) {
                exit 1
            }
        }
        ' \
        "$checksums_file" \
        >"$selected_checksum" || {
            echo "No checksum entry was found for:" >&2
            echo "  ${asset_name}" >&2
            return 1
        }

    echo "Verifying release checksum..."

    (
        cd "$work_dir"

        sha256sum \
            --check \
            "$(basename "$selected_checksum")"
    )
}

validate_release_bundle() {
    local bundle_dir="$1"
    local expected_version="$2"

    local daemon_binary="${bundle_dir}/bin/pookie-paste"
    local ui_binary="${bundle_dir}/bin/pookie-paste-ui"

    local desktop_file="${bundle_dir}/share/applications/io.github.riyanj220.PookiePaste.desktop"

    local autostart_file="${bundle_dir}/share/autostart/io.github.riyanj220.PookiePaste-autostart.desktop"

    local kwin_package="${bundle_dir}/share/pookie-paste/kwin/pookie-focus"

    local release_version_file="${bundle_dir}/RELEASE_VERSION"

    if [[ ! -x "$daemon_binary" ]]; then
        echo "Release bundle is missing the Pookie daemon." >&2
        return 1
    fi

    if [[ ! -x "$ui_binary" ]]; then
        echo "Release bundle is missing the Pookie UI." >&2
        return 1
    fi

    if [[ ! -f "$desktop_file" ]]; then
        echo "Release bundle is missing the desktop entry." >&2
        return 1
    fi

    if [[ ! -f "$autostart_file" ]]; then
        echo "Release bundle is missing the autostart entry." >&2
        return 1
    fi

    if [[ ! -f "${kwin_package}/metadata.json" ]]; then
        echo "Release bundle is missing the KWin helper metadata." >&2
        return 1
    fi

    if [[ ! -f "${kwin_package}/contents/code/main.js" ]]; then
        echo "Release bundle is missing the KWin helper script." >&2
        return 1
    fi

    if [[ ! -f "$release_version_file" ]]; then
        echo "Release bundle is missing RELEASE_VERSION." >&2
        return 1
    fi

    local actual_version

    actual_version="$(
        tr -d '[:space:]' \
            <"$release_version_file"
    )"

    if [[ "$actual_version" != "$expected_version" ]]; then
        echo "Release bundle version mismatch." >&2
        echo "Expected: ${expected_version}" >&2
        echo "Found:    ${actual_version}" >&2
        return 1
    fi
}

prepare_release_bundle() {
    local requested_version="$1"
    local architecture="$2"

    local platform
    local version
    local asset_name
    local release_base_url
    local archive_path
    local checksums_path
    local extract_dir
    local bundle_name
    local bundle_dir

    platform="$(
        release_platform "$architecture"
    )" || {
        echo "Unsupported release architecture: ${architecture}" >&2
        return 1
    }

    version="$(
        resolve_release_version "$requested_version"
    )"

    asset_name="$(
        release_asset_name \
            "$version" \
            "$platform"
    )"

    release_base_url="${POOKIE_GITHUB_BASE_URL}/releases/download/${version}"

    POOKIE_RELEASE_WORK_DIR="$(
        mktemp \
            -d \
            "${TMPDIR:-/tmp}/pookie-paste-release.XXXXXX"
    )"

    archive_path="${POOKIE_RELEASE_WORK_DIR}/${asset_name}"

    checksums_path="${POOKIE_RELEASE_WORK_DIR}/SHA256SUMS"

    extract_dir="${POOKIE_RELEASE_WORK_DIR}/extracted"

    mkdir -p "$extract_dir"

    echo
    echo "Preparing Pookie Paste release"
    echo "=============================="
    echo
    echo "Version:      ${version}"
    echo "Architecture: ${architecture}"
    echo "Platform:     ${platform}"
    echo

    echo "Downloading release archive..."

    curl \
        --fail \
        --location \
        --retry 3 \
        --retry-delay 2 \
        --show-error \
        --output "$archive_path" \
        "${release_base_url}/${asset_name}"

    echo "Downloading release checksum..."

    curl \
        --fail \
        --location \
        --retry 3 \
        --retry-delay 2 \
        --show-error \
        --output "$checksums_path" \
        "${release_base_url}/SHA256SUMS"

    verify_release_checksum \
        "$POOKIE_RELEASE_WORK_DIR" \
        "$asset_name"

    echo "Extracting release..."

    tar \
        -xzf "$archive_path" \
        -C "$extract_dir"

    bundle_name="pookie-paste-${version}-${platform}"

    bundle_dir="${extract_dir}/${bundle_name}"

    if [[ ! -d "$bundle_dir" ]]; then
        echo "Expected release bundle directory was not found:" >&2
        echo "  ${bundle_dir}" >&2
        return 1
    fi

    echo "Validating release bundle..."

    validate_release_bundle \
        "$bundle_dir" \
        "$version"

    POOKIE_RELEASE_BUNDLE_DIR="$bundle_dir"

    POOKIE_RESOLVED_VERSION="$version"

    echo
    echo "Release bundle is ready."
}

cleanup_release_bundle() {
    if [[ -n "${POOKIE_RELEASE_WORK_DIR:-}" \
        && -d "$POOKIE_RELEASE_WORK_DIR" ]]
    then
        rm -rf "$POOKIE_RELEASE_WORK_DIR"
    fi

    POOKIE_RELEASE_WORK_DIR=""
    POOKIE_RELEASE_BUNDLE_DIR=""
}

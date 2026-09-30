#!/usr/bin/env bash

pookie_is_running() {
    pgrep \
        -u "$(id -u)" \
        -x pookie-paste \
        >/dev/null 2>&1
}

wait_for_pookie_exit() {
    local timeout_seconds="${1:-5}"
    local elapsed=0

    while pookie_is_running; do
        if (( elapsed >= timeout_seconds )); then
            return 1
        fi

        sleep 1
        elapsed=$((elapsed + 1))
    done

    return 0
}

stop_pookie() {
    if ! pookie_is_running; then
        return 0
    fi

    pkill \
        -u "$(id -u)" \
        -x pookie-paste \
        >/dev/null 2>&1 || true

    if wait_for_pookie_exit 5; then
        return 0
    fi

    echo "Pookie Paste did not stop gracefully; forcing shutdown..."

    pkill \
        -9 \
        -u "$(id -u)" \
        -x pookie-paste \
        >/dev/null 2>&1 || true

    if ! wait_for_pookie_exit 2; then
        echo "Unable to stop Pookie Paste." >&2
        return 1
    fi
}

#
# Parses the machine-readable porcelain shortcut status output.
# Expected format:
#   status=<ready|needs_setup|conflict|bound_unverified|unavailable>
#   shortcut=<display shortcut>
#
# Sets POOKIE_SHORTCUT_STATUS and POOKIE_SHORTCUT on success.
# Returns 0 on success, 1 on malformed or incomplete output.
#
parse_shortcut_status_porcelain() {
    local raw_output="$1"
    local parsed_status=""
    local parsed_shortcut=""
    local key val

    if [[ -z "$raw_output" ]]; then
        return 1
    fi

    while IFS='=' read -r key val || [[ -n "$key" ]]; do
        if [[ -z "$key" && -z "$val" ]]; then
            continue
        fi

        case "$key" in
            status)
                if [[ -n "$parsed_status" ]]; then
                    return 1
                fi
                case "$val" in
                    ready|needs_setup|conflict|bound_unverified|unavailable)
                        parsed_status="$val"
                        ;;
                    *)
                        return 1
                        ;;
                esac
                ;;
            shortcut)
                if [[ -n "$parsed_shortcut" ]]; then
                    return 1
                fi
                if [[ -z "$val" ]]; then
                    return 1
                fi
                parsed_shortcut="$val"
                ;;
            *)
                return 1
                ;;
        esac
    done <<< "$raw_output"

    if [[ -z "$parsed_status" || -z "$parsed_shortcut" ]]; then
        return 1
    fi

    POOKIE_SHORTCUT_STATUS="$parsed_status"
    POOKIE_SHORTCUT="$parsed_shortcut"
    return 0
}

#
# Polls the installed daemon until it responds with a valid
# porcelain shortcut status or until the timeout is reached.
#
wait_for_pookie_ready() {
    local daemon_path="$1"
    local pid="$2"
    local timeout_seconds="${3:-10}"
    local attempts=$((timeout_seconds * 5))
    local i=0
    local raw_output

    while (( i < attempts )); do
        if [[ -n "$pid" ]] && ! kill -0 "$pid" 2>/dev/null; then
            return 1
        fi

        if raw_output="$("$daemon_path" --shortcut-status --porcelain 2>/dev/null)"; then
            if parse_shortcut_status_porcelain "$raw_output"; then
                return 0
            fi
        fi

        sleep 0.2
        i=$((i + 1))
    done

    return 1
}

start_pookie() {
    local daemon_path="$1"
    local state_dir="$2"

    if [[ ! -x "$daemon_path" ]]; then
        echo "Pookie Paste daemon is not executable: $daemon_path" >&2
        return 1
    fi

    if pookie_is_running; then
        if wait_for_pookie_ready "$daemon_path" "" 5; then
            return 0
        fi

        echo "Pookie Paste is running but failed to respond to status requests." >&2
        return 1
    fi

    mkdir -p "$state_dir"

    nohup "$daemon_path" \
        >"${state_dir}/install-start.log" \
        2>&1 &

    local pid=$!

    if wait_for_pookie_ready "$daemon_path" "$pid" 10; then
        return 0
    fi

    if ! kill -0 "$pid" 2>/dev/null; then
        echo "Pookie Paste daemon process terminated unexpectedly." >&2
    else
        echo "Pookie Paste daemon failed to become responsive." >&2
    fi
    echo "Check:" >&2
    echo "  ${state_dir}/install-start.log" >&2

    return 1
}

#
# Handles post-install onboarding decisions based on authoritative
# shortcut status parsed in Step 5A.
#
handle_pookie_onboarding() {
    local daemon_path="$1"
    local status="$2"
    local shortcut="$3"

    case "$status" in
        ready)
            echo "Pookie Paste is ready."
            echo "Press ${shortcut} to open clipboard history."
            ;;

        bound_unverified)
            echo "Pookie Paste is installed."
            echo "Shortcut binding detected."
            echo "Press ${shortcut} to test it."
            ;;

        needs_setup)
            echo "Pookie Paste is installed."

            if "$daemon_path" --shortcut-setup 2>/dev/null; then
                echo "Finish shortcut setup in the window that just opened."
            else
                echo "Shortcut setup still needs to be completed."
                echo
                echo "Could not open the setup window automatically."
                echo "Open Pookie Paste from your application menu."
            fi
            ;;

        conflict)
            echo "Pookie Paste is installed."
            echo "${shortcut} is already in use."

            if "$daemon_path" --shortcut-setup 2>/dev/null; then
                echo "Choose another shortcut in the window that just opened."
            else
                echo
                echo "Could not open the setup window automatically."
                echo "Open Pookie Paste from your application menu."
            fi
            ;;

        unavailable)
            echo "Pookie Paste is installed, but the global shortcut is unavailable."
            echo "Open Pookie Paste from your application menu to review shortcut settings."
            ;;

        *)
            echo "Warning: Unrecognized shortcut status: ${status}" >&2
            echo "Open Pookie Paste from your application menu to review shortcut settings."
            return 1
            ;;
    esac

    return 0
}



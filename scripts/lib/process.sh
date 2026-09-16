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

    echo "Stopping Pookie Paste..."

    pkill \
        -u "$(id -u)" \
        -x pookie-paste \
        >/dev/null 2>&1 || true

    if wait_for_pookie_exit 5; then
        echo "Pookie Paste stopped."
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

    echo "Pookie Paste stopped."
}

start_pookie() {
    local daemon_path="$1"
    local state_dir="$2"

    if [[ ! -x "$daemon_path" ]]; then
        echo "Pookie Paste daemon is not executable: $daemon_path" >&2
        return 1
    fi

    if pookie_is_running; then
        echo "Pookie Paste is already running."
        return 0
    fi

    mkdir -p "$state_dir"

    echo "Starting Pookie Paste..."

    nohup "$daemon_path" \
        >"${state_dir}/install-start.log" \
        2>&1 &

    local pid=$!

    sleep 2

    if kill -0 "$pid" 2>/dev/null; then
        echo "Pookie Paste is running."
        return 0
    fi

    echo "Pookie Paste did not remain running." >&2
    echo "Check:"
    echo "  ${state_dir}/install-start.log"

    return 1
}

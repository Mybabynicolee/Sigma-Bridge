#!/usr/bin/env bash
set -u

BINARY="./target/release/sigmabridge"
LOG_FILE="sigmabridge.log"
SESSION_NAME="sigmabridge"

build_release() {
    echo "Building release binary..."
    if ! cargo build --release; then
        echo "Build failed, aborting."
        exit 1
    fi
}

prepare() {
    if [ ! -f "config.json" ]; then
        echo "config.json not found."
        exit 1
    fi
    if [ ! -s "data.txt" ]; then
        echo "data.txt is empty, add one refresh token per line first."
        exit 1
    fi
    if [ ! -x "$BINARY" ] || [ "Cargo.toml" -nt "$BINARY" ]; then
        build_release
    fi
}

mode_direct() {
    prepare
    exec "$BINARY" "${1:-serve}"
}

mode_nohup() {
    prepare
    nohup "$BINARY" "${1:-serve}" >> "$LOG_FILE" 2>&1 &
    echo "Started in background (PID $!)"
    echo "Logs: tail -f $LOG_FILE"
}

mode_screen() {
    if ! command -v screen >/dev/null 2>&1; then
        echo "screen not found, install with: apt install screen"
        exit 1
    fi
    prepare
    screen -dmS "$SESSION_NAME" "$BINARY" "${1:-serve}"
    echo "Started in screen session '$SESSION_NAME'"
    echo "Attach : screen -r $SESSION_NAME"
    echo "Detach : Ctrl+A then D"
}

mode_stop() {
    if command -v screen >/dev/null 2>&1 && screen -ls | grep -q "$SESSION_NAME"; then
        screen -S "$SESSION_NAME" -X quit
        echo "Screen session stopped."
    fi
    if pkill -f "$BINARY" >/dev/null 2>&1; then
        echo "Process stopped."
    else
        echo "No running process found."
    fi
}

case "${1:-serve}" in
    serve|probe|selftest)
        mode_direct "$1"
        ;;
    bg)
        mode_nohup "serve"
        ;;
    screen)
        mode_screen "serve"
        ;;
    stop)
        mode_stop
        ;;
    build)
        build_release
        ;;
    *)
        echo "Usage: ./run.sh [serve|probe|selftest|bg|screen|stop|build]"
        exit 1
        ;;
esac

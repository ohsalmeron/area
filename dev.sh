#!/bin/bash
# Development test script - mirrors area-navigator-session but in Xephyr
# This is our test environment before deploying to LightDM

# Kill background processes on exit
trap "kill 0" EXIT

echo "🚀 Starting Area Desktop in Xephyr (Test Environment)..."

# Save the host display (your actual desktop)
HOST_DISPLAY=$DISPLAY
TARGET_DISPLAY=":6"

if [ -z "$HOST_DISPLAY" ]; then
    echo "⚠️  DISPLAY is not set. Defaulting to :0"
    HOST_DISPLAY=":0"
fi

# 1. Cleanup
echo "🧹 Cleaning up..."
pkill -9 Xephyr 2>/dev/null || true
pkill -x area-focus 2>/dev/null || true
pkill -x area-panel 2>/dev/null || true

# Clean up X locks and sockets for target display
rm -f /tmp/.X${TARGET_DISPLAY#:}-lock
rm -f /tmp/.X11-unix/X${TARGET_DISPLAY#:}

# 2. Start Xephyr
echo "🖥️  Starting Xephyr on $TARGET_DISPLAY..."
DISPLAY=$HOST_DISPLAY Xephyr $TARGET_DISPLAY -screen 1920x1080 -ac -br -reset +extension GLX +extension RENDER +extension COMPOSITE -nolisten tcp > /dev/null 2>&1 &

# Wait for Xephyr to be ready (minimal wait)
for i in {1..10}; do
    if DISPLAY=$TARGET_DISPLAY xset q > /dev/null 2>&1; then
        break
    fi
    sleep 0.1
done

if ! ps aux | grep -v grep | grep -q "Xephyr $TARGET_DISPLAY"; then
    echo "❌ Failed to start Xephyr"
    exit 1
fi
echo "✓ Xephyr ready"

# 3. Set up environment (mirror session script)
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export XDG_CURRENT_DESKTOP=Area
export XDG_SESSION_TYPE=x11
export DISPLAY=$TARGET_DISPLAY

# Add binaries to PATH
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -d "$PROJECT_ROOT/xfce-rs/target/release" ]; then
    export PATH="$PROJECT_ROOT/xfce-rs/target/release:$PATH"
fi
if [ -d "$HOME/.local/bin" ]; then
    export PATH="$HOME/.local/bin:$PATH"
fi
if [ -d "/usr/local/bin" ]; then
    export PATH="/usr/local/bin:$PATH"
fi

# 4. Find binaries (fast lookup, no building)
AREA_FOCUS=""
if command -v area-focus >/dev/null 2>&1; then
    AREA_FOCUS="$(command -v area-focus)"
elif [ -f "$HOME/.local/bin/area-focus" ]; then
    AREA_FOCUS="$HOME/.local/bin/area-focus"
elif [ -f "$PROJECT_ROOT/target/release/area-focus" ]; then
    AREA_FOCUS="$PROJECT_ROOT/target/release/area-focus"
fi

AREA_PANEL=""
if command -v area-panel >/dev/null 2>&1; then
    AREA_PANEL="$(command -v area-panel)"
elif [ -f "$HOME/.local/bin/area-panel" ]; then
    AREA_PANEL="$HOME/.local/bin/area-panel"
elif [ -f "$PROJECT_ROOT/target/release/area-panel" ]; then
    AREA_PANEL="$PROJECT_ROOT/target/release/area-panel"
fi

# 5. Launch services in parallel (staged for fast startup)
if [ -n "$AREA_FOCUS" ] && [ -x "$AREA_FOCUS" ]; then
    echo "🎯 Starting area-focus..."
    export RUST_LOG="${RUST_LOG:-info}"
    DISPLAY=$TARGET_DISPLAY "$AREA_FOCUS" > /tmp/area-focus-xephyr.log 2>&1 &
    FOCUS_PID=$!
else
    echo "⚠️  area-focus not found"
fi

if [ -n "$AREA_PANEL" ] && [ -x "$AREA_PANEL" ]; then
    echo "📊 Starting area-panel..."
    DISPLAY=$TARGET_DISPLAY "$AREA_PANEL" > /tmp/area-panel-xephyr.log 2>&1 &
    PANEL_PID=$!
else
    echo "⚠️  area-panel not found"
fi

# 6. Launch Navigator (staged after services)
NAVIGATOR=""
if command -v navigator >/dev/null 2>&1; then
    NAVIGATOR="$(command -v navigator)"
elif [ -f "$PROJECT_ROOT/xfce-rs/target/release/navigator" ]; then
    NAVIGATOR="$PROJECT_ROOT/xfce-rs/target/release/navigator"
elif [ -f "$HOME/.local/bin/navigator" ]; then
    NAVIGATOR="$HOME/.local/bin/navigator"
fi

if [ -n "$NAVIGATOR" ] && [ -x "$NAVIGATOR" ]; then
    echo "🧭 Starting Navigator..."
    WAYLAND_DISPLAY="" DISPLAY=$TARGET_DISPLAY "$NAVIGATOR" &
    NAVIGATOR_PID=$!
else
    echo "❌ Navigator not found"
    exit 1
fi

# 7. Quick status check and summary
echo ""
echo "✅ Area Desktop launched!"
echo ""
echo "Components:"
echo "  - Xephyr: $TARGET_DISPLAY"
[ -n "$FOCUS_PID" ] && echo "  - area-focus: PID $FOCUS_PID"
[ -n "$PANEL_PID" ] && echo "  - area-panel: PID $PANEL_PID"
[ -n "$NAVIGATOR_PID" ] && echo "  - Navigator: PID $NAVIGATOR_PID"
echo ""
echo "Hotkeys: Alt+Tab | F11 | Alt+F4 | Super"
echo "Logs: /tmp/area-*-xephyr.log"
echo ""
wait

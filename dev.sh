#!/bin/bash

# Kill background processes on exit
trap "kill 0" EXIT

echo "🚀 Starting Area Desktop in Xephyr..."

# Save the host display (your actual desktop)
HOST_DISPLAY=$DISPLAY
TARGET_DISPLAY=":6"

if [ -z "$HOST_DISPLAY" ]; then
    echo "⚠️  DISPLAY is not set. Defaulting to :0"
    HOST_DISPLAY=":0"
fi

# 1. Aggressive Cleanup
pkill -9 Xephyr 2>/dev/null || true
pkill -9 area-wm 2>/dev/null || true
pkill -9 area-shell 2>/dev/null || true

# Clean up IPC socket
IPC_SOCKET="/run/user/$(id -u)/area-wm.sock"
rm -f "$IPC_SOCKET"

# Clean up X locks and sockets for target display
rm -f /tmp/.X${TARGET_DISPLAY#:}-lock
rm -f /tmp/.X11-unix/X${TARGET_DISPLAY#:}
sleep 1

# 2. Start Xephyr
echo "🖥️  Starting Xephyr on $TARGET_DISPLAY (Host: $HOST_DISPLAY)"
# We explicitly set DISPLAY to the host for Xephyr itself
DISPLAY=$HOST_DISPLAY Xephyr $TARGET_DISPLAY -screen 1280x720 -ac -br -reset +extension GLX +extension RENDER +extension COMPOSITE -nolisten tcp &
sleep 2

# Verify Xephyr is running
if ! ps aux | grep -v grep | grep -q "Xephyr $TARGET_DISPLAY"; then
    echo "❌ Failed to start Xephyr on $TARGET_DISPLAY."
    exit 1
fi

# 3. Run Window Manager
echo "🏠 Launching area-wm..."
DISPLAY=$TARGET_DISPLAY cargo run --bin area-wm &
sleep 2

# 4. Run Shell
echo "🎨 Launching area-shell..."
DISPLAY=$TARGET_DISPLAY cargo run --bin area-shell &
sleep 2

# Add navigator to PATH for Super key launching
export PATH="$PWD/xfce-rs/target/release:$PATH"

# 5. Launch Navigator (app launcher) - auto-launch for now
echo "🧭 Launching Navigator..."
sleep 1
WAYLAND_DISPLAY="" DISPLAY=$TARGET_DISPLAY navigator &

# Keep script alive
echo "✅ Desktop ready! Press Super key for Navigator (WIP), F9 for Overview, F10 for Jelly Window test."
wait

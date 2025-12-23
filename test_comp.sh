#!/bin/bash
trap "kill 0" EXIT

echo "🚀 Starting Test Environment..."

# Clean up
pkill -9 Xephyr 2>/dev/null
rm -f /tmp/area-wm.sock

# Start Xephyr
Xephyr :6 -screen 1280x720 -ac -br -reset +extension GLX +extension RENDER +extension COMPOSITE -nolisten tcp &
XEPHYR_PID=$!
export DISPLAY=:6

# Force software rendering for consistent testing in Xephyr
export LIBGL_ALWAYS_SOFTWARE=1
export GALLIUM_DRIVER=llvmpipe

# Wait for Xephyr
sleep 1

# Start WM (debug)
echo "🏠 Launching area-wm..."
export RUST_LOG=debug
cargo run --bin area-wm &
WM_PID=$!

# Wait for WM
sleep 2

# Start Compositor (debug)
echo "🎨 Launching area-comp..."
export RUST_BACKTRACE=1
cargo run --bin area-comp &
COMP_PID=$!

# Start Client
sleep 2
# Start Client
sleep 2
echo "🔢 Launching gnome-calculator..."
gnome-calculator &

wait

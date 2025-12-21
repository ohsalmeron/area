#!/bin/bash
# Quick reload script for area-focus - rebuild and restart without logging out

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo "🔄 Reloading area-focus..."

# 1. Kill existing area-focus process
echo "  Killing existing area-focus..."
pkill -x area-focus 2>/dev/null || echo "  (No running area-focus found)"
sleep 0.5

# 2. Build new version
echo "  Building area-focus..."
cargo build --release --bin area-focus

# 3. Install to ~/.local/bin
echo "  Installing..."
install -m 755 "$PROJECT_ROOT/target/release/area-focus" "$HOME/.local/bin/area-focus"

# 4. Start new instance
echo "  Starting area-focus..."
export DISPLAY="${DISPLAY:-:0}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export RUST_LOG=info
nohup area-focus > /tmp/area-focus.log 2>&1 &
FOCUS_PID=$!

sleep 0.5
if ps -p $FOCUS_PID > /dev/null 2>&1; then
    echo "✅ area-focus reloaded! (PID: $FOCUS_PID)"
    echo "   Logs: tail -f /tmp/area-focus.log"
else
    echo "❌ area-focus failed to start. Check logs: tail /tmp/area-focus.log"
    exit 1
fi


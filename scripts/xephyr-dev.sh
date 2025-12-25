#!/bin/bash
# =============================================================================
# Area Desktop Environment - Xephyr Development Script
# =============================================================================
# Launches Xephyr with the area desktop environment for development/testing
#
# Usage:
#   ./xephyr-dev.sh           # Launch with defaults (1280x720 on :99)
#   ./xephyr-dev.sh --watch   # Auto-reload on source changes
#   ./xephyr-dev.sh --size 1920x1080  # Custom resolution
#   ./xephyr-dev.sh --display :98     # Custom display number
#
# Requirements:
#   - Xephyr: sudo apt install xserver-xephyr (Debian/Ubuntu)
#             sudo dnf install xorg-x11-server-Xephyr (Fedora)
#             sudo pacman -S xorg-server-xephyr (Arch)
# =============================================================================

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# -----------------------------------------------------------------------------
# Configuration defaults
# -----------------------------------------------------------------------------
DISPLAY_NUM=":99"
SCREEN_SIZE="1280x720"
WATCH_MODE=false
DEBUG_MODE=false
KEEP_XEPHYR=false

# -----------------------------------------------------------------------------
# Parse arguments
# -----------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case $1 in
        --watch|-w)
            WATCH_MODE=true
            shift
            ;;
        --size|-s)
            SCREEN_SIZE="$2"
            shift 2
            ;;
        --display|-d)
            DISPLAY_NUM="$2"
            shift 2
            ;;
        --debug)
            DEBUG_MODE=true
            LOG_LEVEL="debug"
            shift
            ;;
        --trace)
            DEBUG_MODE=true
            LOG_LEVEL="trace"
            shift
            ;;
        --keep)
            KEEP_XEPHYR=true
            shift
            ;;
        --help|-h)
            echo "Area Desktop Environment - Xephyr Development Script"
            echo ""
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --watch, -w          Auto-rebuild and restart on source changes"
            echo "  --size, -s SIZE      Screen size (default: 1280x720)"
            echo "  --display, -d DISP   Display number (default: :99)"
            echo "  --display, -d DISP   Display number (default: :99)"
            echo "  --debug              Enable debug build and RUST_LOG=debug"
            echo "  --trace              Enable debug build and RUST_LOG=trace"
            echo "  --keep               Keep Xephyr running after area exits"
            echo "  --help, -h           Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                          # Launch with defaults"
            echo "  $0 --watch                  # Auto-reload mode"
            echo "  $0 --size 1920x1080         # Full HD resolution"
            echo "  $0 --display :98 --debug    # Custom display with debug logging"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# -----------------------------------------------------------------------------
# Check dependencies
# -----------------------------------------------------------------------------
check_command() {
    if ! command -v "$1" &>/dev/null; then
        echo "❌ Error: '$1' is not installed."
        echo ""
        echo "Install with:"
        echo "  Debian/Ubuntu: sudo apt install $2"
        echo "  Fedora:        sudo dnf install $3"
        echo "  Arch:          sudo pacman -S $4"
        exit 1
    fi
}

check_command "Xephyr" "xserver-xephyr" "xorg-x11-server-Xephyr" "xorg-server-xephyr"
check_command "cargo" "cargo" "cargo" "cargo"

# -----------------------------------------------------------------------------
# Cleanup function
# -----------------------------------------------------------------------------
XEPHYR_PID=""
AREA_PID=""

cleanup() {
    echo ""
    echo "🧹 Cleaning up..."
    
    # Kill area if running (kill entire process group to catch background processes)
    if [[ -n "$AREA_PID" ]] && kill -0 "$AREA_PID" 2>/dev/null; then
        echo "   Stopping area (PID: $AREA_PID)..."
        # Kill process group to ensure all child processes are terminated
        kill -TERM -"$AREA_PID" 2>/dev/null || kill "$AREA_PID" 2>/dev/null || true
        sleep 0.5
        # Force kill if still running
        if kill -0 "$AREA_PID" 2>/dev/null; then
            kill -KILL "$AREA_PID" 2>/dev/null || true
        fi
        wait "$AREA_PID" 2>/dev/null || true
    fi
    
    # Kill Xephyr if running (unless --keep was specified)
    if [[ "$KEEP_XEPHYR" != "true" ]] && [[ -n "$XEPHYR_PID" ]] && kill -0 "$XEPHYR_PID" 2>/dev/null; then
        echo "   Stopping Xephyr (PID: $XEPHYR_PID)..."
        kill "$XEPHYR_PID" 2>/dev/null || true
        wait "$XEPHYR_PID" 2>/dev/null || true
    fi
    
    echo "✅ Cleanup complete"
}

trap cleanup EXIT INT TERM

# -----------------------------------------------------------------------------
# Build area
# -----------------------------------------------------------------------------
build_area() {
    echo "🔨 Building area..."
    
    if [[ "$DEBUG_MODE" == "true" ]]; then
        if ! cargo build --bin area 2>&1; then
            echo "❌ Build failed!"
            return 1
        fi
    else
        if ! cargo build --release --bin area 2>&1; then
            echo "❌ Build failed!"
            return 1
        fi
    fi
    
    echo "✅ Build successful"
    return 0
}

# -----------------------------------------------------------------------------
# Start Xephyr
# -----------------------------------------------------------------------------
start_xephyr() {
    # Check if Xephyr is already running on this display
    if xdpyinfo -display "$DISPLAY_NUM" &>/dev/null; then
        echo "⚠️  Display $DISPLAY_NUM is already in use"
        
        # Check if it's our Xephyr
        if pgrep -f "Xephyr.*$DISPLAY_NUM" &>/dev/null; then
            echo "   Found existing Xephyr on $DISPLAY_NUM, will reuse it"
            XEPHYR_PID=$(pgrep -f "Xephyr.*$DISPLAY_NUM")
            return 0
        else
            echo "❌ Display $DISPLAY_NUM is in use by another X server"
            echo "   Try a different display: $0 --display :100"
            exit 1
        fi
    fi
    
    echo "🖥️  Starting Xephyr ($SCREEN_SIZE on $DISPLAY_NUM)..."
    
    # Start Xephyr with:
    # -ac: Disable access control (allow connections)
    # -br: Black root window background
    # -screen: Screen dimensions
    # -resizeable: Allow window resize
    # -host-cursor: Use host cursor (smoother)
    Xephyr "$DISPLAY_NUM" \
        -ac \
        -br \
        -screen "$SCREEN_SIZE" \
        -resizeable \
        -host-cursor \
        -title "Area Desktop Environment (Dev)" \
        &
    
    XEPHYR_PID=$!
    
    # Wait for Xephyr to start
    echo "   Waiting for Xephyr to initialize..."
    
    # Give Xephyr time to initialize (it needs to process xkbcomp, etc.)
    sleep 1
    
    # Check if process is still alive
    if ! kill -0 "$XEPHYR_PID" 2>/dev/null; then
        echo "❌ Xephyr process died during startup"
        exit 1
    fi
    
    # Try to connect to the display
    for i in {1..50}; do
        # Check if process is still running
        if ! kill -0 "$XEPHYR_PID" 2>/dev/null; then
            echo "❌ Xephyr process died"
            exit 1
        fi
        
        # Check if we can connect to the display
        if command -v xdpyinfo &>/dev/null; then
            if DISPLAY="$DISPLAY_NUM" xdpyinfo &>/dev/null; then
                echo "✅ Xephyr started (PID: $XEPHYR_PID)"
                return 0
            fi
        else
            # Fallback if xdpyinfo is missing
            # Check for X socket
            DISPLAY_NUM_ONLY="${DISPLAY_NUM#:}"
            if [[ -S "/tmp/.X11-unix/X$DISPLAY_NUM_ONLY" ]]; then
                echo "✅ Xephyr started (PID: $XEPHYR_PID) [Socket detected, xdpyinfo missing]"
                return 0
            fi
        fi
        
        # Also check for X socket explicitly if xdpyinfo check failed for some reason
        DISPLAY_NUM_ONLY="${DISPLAY_NUM#:}"
        if [[ -S "/tmp/.X11-unix/X$DISPLAY_NUM_ONLY" ]]; then
            # Socket exists, give it one more moment
            sleep 0.5
            if command -v xdpyinfo &>/dev/null; then
                if DISPLAY="$DISPLAY_NUM" xdpyinfo &>/dev/null; then
                    echo "✅ Xephyr started (PID: $XEPHYR_PID)"
                    return 0
                fi
            else
                 # Trust the socket
                 echo "✅ Xephyr started (PID: $XEPHYR_PID) [Socket detected]"
                 return 0
            fi
        fi
        
        sleep 0.2
    done
    
    # Final check - maybe it's running but xdpyinfo is having issues
    if kill -0 "$XEPHYR_PID" 2>/dev/null; then
        echo "⚠️  Xephyr process is running but xdpyinfo can't connect"
        echo "   Proceeding anyway... (PID: $XEPHYR_PID)"
        return 0
    fi
    
    echo "❌ Xephyr failed to start within timeout"
    exit 1
}

# -----------------------------------------------------------------------------
# Run area
# -----------------------------------------------------------------------------
run_area() {
    echo ""
    echo "🚀 Launching area on $DISPLAY_NUM..."
    echo "   Screen size: $SCREEN_SIZE"
    echo ""
    
    # Set environment variables
    export DISPLAY="$DISPLAY_NUM"
    export RUST_BACKTRACE=1
    
    # Force unbuffered output for real-time logs
    export RUST_LOG_STYLE=always
    
    if [[ "$DEBUG_MODE" == "true" ]]; then
        export RUST_LOG="${RUST_LOG:-${LOG_LEVEL:-debug}}"
        BINARY="$PROJECT_ROOT/target/debug/area"
    else
        # Default to debug level for development script to see WM acquisition details
        export RUST_LOG="${RUST_LOG:-debug}"
        BINARY="$PROJECT_ROOT/target/release/area"
    fi
    
    # Create logs directory
    LOGS_DIR="$PROJECT_ROOT/logs"
    mkdir -p "$LOGS_DIR"
    
    # Generate timestamped log filenames
    TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
    FULL_LOG="$LOGS_DIR/area_full_${TIMESTAMP}.log"
    WARN_ERROR_LOG="$LOGS_DIR/area_warnings_errors_${TIMESTAMP}.log"
    
    echo "   DISPLAY=$DISPLAY"
    echo "   RUST_LOG=$RUST_LOG"
    echo "   RUST_BACKTRACE=$RUST_BACKTRACE"
    echo "   Binary: $BINARY"
    echo "   📝 Full log: $FULL_LOG"
    echo "   ⚠️  Warnings/Errors: $WARN_ERROR_LOG"
    echo ""
    echo "─────────────────────────────────────────────────────────────────────"
    echo ""
    
    # Run area with output captured to both terminal and log files
    # Use tee to duplicate output: one stream to full log, another filtered to warnings/errors log
    # Process substitution allows us to filter in real-time without hiding output from terminal
    if command -v stdbuf &>/dev/null; then
        # Use tee with process substitution to duplicate stream:
        # - All output goes to terminal (via stdout)
        # - All output goes to full log file
        # - Filtered WARN/ERROR output goes to warnings/errors log file
        # Pattern matches: "WARN", "ERROR" (Rust tracing levels), "warning", "error", and emoji indicators
        stdbuf -oL -eL "$BINARY" 2>&1 | \
            tee "$FULL_LOG" | \
            tee >(grep --line-buffered -iE "( WARN | ERROR |warning|error|⚠️|❌)" >> "$WARN_ERROR_LOG")
        AREA_EXIT_CODE=${PIPESTATUS[0]}
    else
        # Fallback: run directly and capture output
        "$BINARY" 2>&1 | \
            tee "$FULL_LOG" | \
            tee >(grep --line-buffered -iE "( WARN | ERROR |warning|error|⚠️|❌)" >> "$WARN_ERROR_LOG")
        AREA_EXIT_CODE=${PIPESTATUS[0]}
    fi
    
    echo ""
    echo "─────────────────────────────────────────────────────────────────────"
    
    if [[ "$AREA_EXIT_CODE" -eq 0 ]]; then
        echo "✅ Area exited normally"
    else
        echo "⚠️  Area exited with code: $AREA_EXIT_CODE"
    fi
    
    # Show summary of warnings/errors if any were captured
    if [[ -f "$WARN_ERROR_LOG" ]] && [[ -s "$WARN_ERROR_LOG" ]]; then
        WARN_ERROR_COUNT=$(wc -l < "$WARN_ERROR_LOG")
        echo "   📊 Captured $WARN_ERROR_COUNT warning/error lines in: $WARN_ERROR_LOG"
    fi
    
    return "$AREA_EXIT_CODE"
}

# -----------------------------------------------------------------------------
# Watch mode (auto-reload on file changes)
# -----------------------------------------------------------------------------
watch_and_reload() {
    echo ""
    echo "👀 Watch mode enabled - monitoring src/ for changes"
    echo "   Press Ctrl+C to stop"
    echo ""
    
    # Check for inotifywait
    if command -v inotifywait &>/dev/null; then
        USE_INOTIFY=true
    else
        echo "⚠️  inotifywait not found, using polling mode (slower)"
        echo "   Install inotify-tools for better performance"
        USE_INOTIFY=false
    fi
    
    while true; do
        # Build and run
        if build_area; then
            # Run area in background for watch mode, but capture output
            (
                export DISPLAY="$DISPLAY_NUM"
                export RUST_BACKTRACE=1
                export RUST_LOG_STYLE=always
                
                if [[ "$DEBUG_MODE" == "true" ]]; then
                    export RUST_LOG="${RUST_LOG:-${LOG_LEVEL:-debug}}"
                    BINARY="$PROJECT_ROOT/target/debug/area"
                else
                    export RUST_LOG="${RUST_LOG:-debug}"
                    BINARY="$PROJECT_ROOT/target/release/area"
                fi
                
                # Use stdbuf for unbuffered output if available
                if command -v stdbuf &>/dev/null; then
                    stdbuf -oL -eL "$BINARY"
                else
                    "$BINARY"
                fi
            ) &
            AREA_PID=$!
            
            # Wait for file changes
            if [[ "$USE_INOTIFY" == "true" ]]; then
                inotifywait -q -r -e modify,create,delete --include '\.rs$' src/ 2>/dev/null || true
            else
                # Polling fallback - check every 2 seconds
                sleep 2
                BINARY_PATH="$PROJECT_ROOT/target/release/area"
                if [[ ! -f "$BINARY_PATH" ]]; then
                    BINARY_PATH="$PROJECT_ROOT/target/debug/area"
                fi
                while ! find src -type f -name "*.rs" -newer "$BINARY_PATH" 2>/dev/null | grep -q .; do
                    sleep 2
                done
            fi
            
            echo ""
            echo "📝 Source files changed, restarting..."
            
            # Kill current area instance
            if [[ -n "$AREA_PID" ]] && kill -0 "$AREA_PID" 2>/dev/null; then
                kill "$AREA_PID" 2>/dev/null || true
                wait "$AREA_PID" 2>/dev/null || true
            fi
        else
            echo ""
            echo "⚠️  Build failed, waiting for next change..."
            
            if [[ "$USE_INOTIFY" == "true" ]]; then
                inotifywait -q -r -e modify,create,delete --include '\.rs$' src/ 2>/dev/null || true
            else
                sleep 2
            fi
        fi
    done
}

# -----------------------------------------------------------------------------
# Main execution
# -----------------------------------------------------------------------------
echo ""
echo "╔════════════════════════════════════════════════════════════════════╗"
echo "║           Area Desktop Environment - Xephyr Development            ║"
echo "╚════════════════════════════════════════════════════════════════════╝"
echo ""

# Build first
if ! build_area; then
    exit 1
fi

# Start Xephyr
start_xephyr

# Run in watch mode or single run
if [[ "$WATCH_MODE" == "true" ]]; then
    watch_and_reload
else
    run_area
fi

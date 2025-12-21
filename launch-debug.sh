#!/bin/bash
# Launch Area Desktop components in separate terminals with logging
# Each component runs in its own terminal window for easy debugging

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 Area Desktop Debug Launcher${NC}"
echo ""

# Create logs directory
LOG_DIR="${XDG_RUNTIME_DIR:-/tmp}/area-debug-logs"
mkdir -p "$LOG_DIR"
echo -e "${GREEN}📁 Log directory: $LOG_DIR${NC}"
echo ""

# Check for required binaries
check_binary() {
    local name=$1
    local path=""
    
    if command -v "$name" >/dev/null 2>&1; then
        path=$(command -v "$name")
    elif [ -f "$HOME/.local/bin/$name" ]; then
        path="$HOME/.local/bin/$name"
    elif [ -f "/usr/local/bin/$name" ]; then
        path="/usr/local/bin/$name"
    elif [ -f "target/release/$name" ]; then
        path="$(pwd)/target/release/$name"
    elif [ -f "crates/$name/target/release/$name" ]; then
        path="$(pwd)/crates/$name/target/release/$name"
    elif [ -f "target/release/$name" ]; then
        path="$(pwd)/target/release/$name"
    fi
    
    if [ -z "$path" ]; then
        echo -e "${RED}❌ $name not found${NC}"
        return 1
    fi
    
    echo "$path"
    return 0
}

# Find binaries
echo -e "${YELLOW}🔍 Finding binaries...${NC}"
AREA_FOCUS=$(check_binary "area-focus") || exit 1
AREA_PANEL=$(check_binary "area-panel") || exit 1
AREA_NAVIGATOR=$(check_binary "area-navigator") || {
    echo -e "${YELLOW}⚠️  area-navigator not found, trying to build...${NC}"
    if [ -f "crates/area-navigator/Cargo.toml" ]; then
        cargo build --release --manifest-path crates/area-navigator/Cargo.toml
        if [ -f "crates/area-navigator/target/release/area-navigator" ]; then
            AREA_NAVIGATOR="$(pwd)/crates/area-navigator/target/release/area-navigator"
            echo -e "${GREEN}✓ Built area-navigator${NC}"
        else
            exit 1
        fi
    else
        exit 1
    fi
}

echo -e "${GREEN}✓ area-focus: $AREA_FOCUS${NC}"
echo -e "${GREEN}✓ area-panel: $AREA_PANEL${NC}"
echo -e "${GREEN}✓ area-navigator: $AREA_NAVIGATOR${NC}"
echo ""

# Check for terminal emulator
TERMINAL=""
if command -v alacritty >/dev/null 2>&1; then
    TERMINAL="alacritty"
elif command -v xterm >/dev/null 2>&1; then
    TERMINAL="xterm"
elif command -v gnome-terminal >/dev/null 2>&1; then
    TERMINAL="gnome-terminal"
elif command -v konsole >/dev/null 2>&1; then
    TERMINAL="konsole"
else
    echo -e "${RED}❌ No terminal emulator found (alacritty, xterm, gnome-terminal, konsole)${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Using terminal: $TERMINAL${NC}"
echo ""

# Set up environment
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export XDG_CURRENT_DESKTOP=Area
export XDG_SESSION_TYPE=x11
export RUST_LOG="${RUST_LOG:-debug}"

# Ensure DISPLAY is set
if [ -z "$DISPLAY" ]; then
    echo -e "${RED}❌ DISPLAY not set${NC}"
    exit 1
fi

echo -e "${BLUE}📋 Environment:${NC}"
echo "  DISPLAY=$DISPLAY"
echo "  XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
echo "  RUST_LOG=$RUST_LOG"
echo ""

# Function to launch component in terminal
launch_in_terminal() {
    local name=$1
    local binary=$2
    local log_file="$LOG_DIR/${name}.log"
    local title="Area Desktop - $name"
    
    echo -e "${YELLOW}🚀 Launching $name...${NC}"
    
    case "$TERMINAL" in
        alacritty)
            alacritty \
                --title "$title" \
                --command bash -c "
                    echo '=== $name Log ===';
                    echo 'Log file: $log_file';
                    echo 'Binary: $binary';
                    echo 'Press Ctrl+C to stop';
                    echo '';
                    $binary 2>&1 | tee '$log_file'
                " &
            ;;
        xterm)
            xterm -T "$title" -e bash -c "
                echo '=== $name Log ===';
                echo 'Log file: $log_file';
                echo 'Binary: $binary';
                echo 'Press Ctrl+C to stop';
                echo '';
                $binary 2>&1 | tee '$log_file'
            " &
            ;;
        gnome-terminal)
            gnome-terminal --title="$title" -- bash -c "
                echo '=== $name Log ===';
                echo 'Log file: $log_file';
                echo 'Binary: $binary';
                echo 'Press Ctrl+C to stop';
                echo '';
                $binary 2>&1 | tee '$log_file'
            " &
            ;;
        konsole)
            konsole --title "$title" -e bash -c "
                echo '=== $name Log ===';
                echo 'Log file: $log_file';
                echo 'Binary: $binary';
                echo 'Press Ctrl+C to stop';
                echo '';
                $binary 2>&1 | tee '$log_file'
            " &
            ;;
    esac
    
    sleep 0.5  # Small delay between launches
    echo -e "${GREEN}✓ $name launched (PID: $!)${NC}"
    echo "  Log: $log_file"
    echo ""
}

# Launch components
launch_in_terminal "area-focus" "$AREA_FOCUS"
launch_in_terminal "area-panel" "$AREA_PANEL"
launch_in_terminal "area-navigator" "$AREA_NAVIGATOR"

# Wait a moment for everything to start
sleep 2

echo -e "${GREEN}✅ All components launched!${NC}"
echo ""
echo -e "${BLUE}📊 Summary:${NC}"
echo "  Log directory: $LOG_DIR"
echo "  Components:"
echo "    - area-focus: $LOG_DIR/area-focus.log"
echo "    - area-panel: $LOG_DIR/area-panel.log"
echo "    - area-navigator: $LOG_DIR/area-navigator.log"
echo ""
echo -e "${YELLOW}💡 Tips:${NC}"
echo "  - Each component runs in its own terminal window"
echo "  - Logs are also written to files in $LOG_DIR"
echo "  - Press Ctrl+C in each terminal to stop that component"
echo "  - Check logs for detailed debugging information"
echo ""
echo -e "${BLUE}Press Enter to view live logs (or Ctrl+C to exit)...${NC}"
read

# Show live log viewer
tail -f "$LOG_DIR"/*.log


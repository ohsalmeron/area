#!/bin/bash
# Installation script for Area Desktop session
# Builds binaries and installs systemd services and LightDM session

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Area Desktop Session Installer${NC}"
echo "=================================="

# Check if running as root for system-wide install
if [ "$EUID" -eq 0 ]; then
    INSTALL_PREFIX="/usr/local"
    XSESSION_DIR="/usr/share/xsessions"
    SYSTEMD_USER_DIR="/etc/systemd/user"
    INSTALL_TYPE="system"
else
    INSTALL_PREFIX="$HOME/.local"
    # LightDM typically only looks in /usr/share/xsessions, so we need sudo for session file
    XSESSION_DIR="/usr/share/xsessions"
    SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
    INSTALL_TYPE="user"
fi

echo "Install type: $INSTALL_TYPE"
echo "Install prefix: $INSTALL_PREFIX"
echo ""

# Build focus manager, panel, and navigator
echo -e "${YELLOW}Building focus manager...${NC}"
cd "$PROJECT_ROOT"
if ! cargo build --release --bin area-focus; then
    echo -e "${RED}Error: area-focus build failed${NC}" >&2
    exit 1
fi

if [ ! -f "$PROJECT_ROOT/target/release/area-focus" ]; then
    echo -e "${RED}Error: area-focus binary not found after build${NC}" >&2
    exit 1
fi
echo -e "${GREEN}✓ Focus manager built successfully${NC}"

echo -e "${YELLOW}Building panel...${NC}"
if ! cargo build --release --bin area-panel; then
    echo -e "${RED}Error: area-panel build failed${NC}" >&2
    exit 1
fi

if [ ! -f "$PROJECT_ROOT/target/release/area-panel" ]; then
    echo -e "${RED}Error: area-panel binary not found after build${NC}" >&2
    exit 1
fi
echo -e "${GREEN}✓ Panel built successfully${NC}"

echo -e "${YELLOW}Building navigator...${NC}"
if [ ! -d "$PROJECT_ROOT/xfce-rs" ]; then
    echo -e "${RED}Error: xfce-rs directory not found${NC}" >&2
    echo "Navigator is required for this session." >&2
    exit 1
fi

cd "$PROJECT_ROOT/xfce-rs"
if ! cargo build --release --bin navigator; then
    echo -e "${RED}Error: Navigator build failed${NC}" >&2
    exit 1
fi

if [ ! -f "$PROJECT_ROOT/xfce-rs/target/release/navigator" ]; then
    echo -e "${RED}Error: Navigator binary not found after build${NC}" >&2
    exit 1
fi

cd "$PROJECT_ROOT"
echo -e "${GREEN}✓ Navigator built successfully${NC}"

# Create directories
echo -e "${YELLOW}Creating directories...${NC}"
mkdir -p "$INSTALL_PREFIX/bin"
mkdir -p "$XSESSION_DIR"
mkdir -p "$SYSTEMD_USER_DIR"

# Install binaries
echo -e "${YELLOW}Installing binaries...${NC}"
install -m 755 "$PROJECT_ROOT/target/release/area-focus" "$INSTALL_PREFIX/bin/area-focus"
install -m 755 "$PROJECT_ROOT/target/release/area-panel" "$INSTALL_PREFIX/bin/area-panel"
install -m 755 "$PROJECT_ROOT/session/area-navigator-session" "$INSTALL_PREFIX/bin/area-navigator-session"
install -m 755 "$PROJECT_ROOT/xfce-rs/target/release/navigator" "$INSTALL_PREFIX/bin/navigator"
echo -e "${GREEN}✓ Binaries installed${NC}"

# No systemd services needed for simple Navigator-only session
echo -e "${GREEN}✓ Skipping systemd services (not needed for Navigator-only session)${NC}"

# Install LightDM session file
echo -e "${YELLOW}Installing LightDM session file...${NC}"
# LightDM session file must be in /usr/share/xsessions (requires sudo)
if [ "$INSTALL_TYPE" = "user" ]; then
    echo -e "${YELLOW}Note: Installing session file to $XSESSION_DIR requires sudo${NC}"
    # Update Exec path in desktop file and install with sudo
    sed "s|Exec=.*|Exec=$INSTALL_PREFIX/bin/area-navigator-session|" "$PROJECT_ROOT/session/area.desktop" | sudo tee "$XSESSION_DIR/area.desktop" > /dev/null
    sudo chmod 644 "$XSESSION_DIR/area.desktop"
else
    # Update Exec path in desktop file
    sed "s|Exec=.*|Exec=$INSTALL_PREFIX/bin/area-navigator-session|" "$PROJECT_ROOT/session/area.desktop" > "$XSESSION_DIR/area.desktop"
    chmod 644 "$XSESSION_DIR/area.desktop"
fi
echo -e "${GREEN}✓ LightDM session file installed${NC}"

# Summary
echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "Installed files:"
echo "  - Focus manager: $INSTALL_PREFIX/bin/area-focus"
echo "  - Panel: $INSTALL_PREFIX/bin/area-panel"
echo "  - Session script: $INSTALL_PREFIX/bin/area-navigator-session"
if [ -f "$INSTALL_PREFIX/bin/navigator" ]; then
    echo "  - Navigator: $INSTALL_PREFIX/bin/navigator"
fi
echo "  - LightDM session: $XSESSION_DIR/area.desktop"
echo ""
echo "Components:"
echo "  - area-focus: Handles keyboard input routing, focus management, and global hotkeys"
echo "                (Alt+Tab, F11, Alt+F4, Super key for Navigator toggle)"
echo "  - area-panel: Bottom panel showing open windows (placeholder for future features)"
echo "  - navigator: Main desktop application"
echo ""
echo "Crates in use:"
echo "  ✓ area-focus (active) - Window focus manager and keyboard shortcuts"
echo "  ✓ area-panel (active) - Bottom panel UI"
echo "  ✗ area-comp (not used) - Compositor (for future use)"
echo "  ✗ area-wm (not used) - Full window manager (for future use)"
echo "  ✗ area-ipc (not used) - IPC protocol (for future use)"
echo "  ✗ area-shell (not used) - Shell components (for future use)"
echo ""
echo "Next steps:"
if [ "$INSTALL_TYPE" = "user" ]; then
    echo "  1. Log out from your current session"
    echo "  2. Select 'Area Navigator' from the LightDM session menu"
    echo "  3. Log in"
else
    echo "  1. All users can now select 'Area Navigator' from LightDM"
fi
echo ""
echo "Note: This is a minimal session that only runs Navigator."
echo "area-focus provides essential window management and keyboard input handling."



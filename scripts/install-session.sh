#!/bin/bash
# Installation script for Area Desktop session
# Builds unified area binary and installs systemd service and LightDM session

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

# Build unified area binary (WM + Compositor)
echo -e "${YELLOW}Building unified area binary (WM + Compositor)...${NC}"
cd "$PROJECT_ROOT"
if ! cargo build --release --bin area; then
    echo -e "${RED}Error: area build failed${NC}" >&2
    exit 1
fi

if [ ! -f "$PROJECT_ROOT/target/release/area" ]; then
    echo -e "${RED}Error: area binary not found after build${NC}" >&2
    exit 1
fi
echo -e "${GREEN}✓ Unified area binary built successfully${NC}"

# Create directories
echo -e "${YELLOW}Creating directories...${NC}"
mkdir -p "$INSTALL_PREFIX/bin"
mkdir -p "$XSESSION_DIR"
mkdir -p "$SYSTEMD_USER_DIR"

# Install binary
echo -e "${YELLOW}Installing binary...${NC}"
install -m 755 "$PROJECT_ROOT/target/release/area" "$INSTALL_PREFIX/bin/area"
echo -e "${GREEN}✓ Binary installed${NC}"

# Install systemd service
echo -e "${YELLOW}Installing systemd service...${NC}"
install -m 644 "$PROJECT_ROOT/session/area.service" "$SYSTEMD_USER_DIR/area.service"
install -m 644 "$PROJECT_ROOT/session/area-desktop.target" "$SYSTEMD_USER_DIR/area-desktop.target"

# Update ExecStart path in service file
sed -i "s|ExecStart=/usr/local/bin/area|ExecStart=$INSTALL_PREFIX/bin/area|" "$SYSTEMD_USER_DIR/area.service"

echo -e "${GREEN}✓ Systemd services installed${NC}"

# Install session script
echo -e "${YELLOW}Installing session script...${NC}"
install -m 755 "$PROJECT_ROOT/session/area-session" "$INSTALL_PREFIX/bin/area-session"
echo -e "${GREEN}✓ Session script installed${NC}"

# Install LightDM session file
echo -e "${YELLOW}Installing LightDM session file...${NC}"
# LightDM session file must be in /usr/share/xsessions (requires sudo)
if [ "$INSTALL_TYPE" = "user" ]; then
    echo -e "${YELLOW}Note: Installing session file to $XSESSION_DIR requires sudo${NC}"
    # Create temporary desktop file with correct Exec path
    TEMP_DESKTOP=$(mktemp)
    cat > "$TEMP_DESKTOP" << EOF
[Desktop Entry]
Name=Area
Comment=Area Window Manager + Compositor
Exec=$INSTALL_PREFIX/bin/area-session
Type=Application
DesktopNames=Area
EOF
    sudo install -m 644 "$TEMP_DESKTOP" "$XSESSION_DIR/area.desktop"
    rm "$TEMP_DESKTOP"
else
    # Create desktop file with correct Exec path
    cat > "$XSESSION_DIR/area.desktop" << EOF
[Desktop Entry]
Name=Area
Comment=Area Window Manager + Compositor
Exec=$INSTALL_PREFIX/bin/area-session
Type=Application
DesktopNames=Area
EOF
    chmod 644 "$XSESSION_DIR/area.desktop"
fi
echo -e "${GREEN}✓ LightDM session file installed${NC}"

# Reload systemd user daemon
echo -e "${YELLOW}Reloading systemd user daemon...${NC}"
systemctl --user daemon-reload
echo -e "${GREEN}✓ Systemd daemon reloaded${NC}"

# Summary
echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "Installed files:"
echo "  - Unified binary: $INSTALL_PREFIX/bin/area"
echo "  - Session script: $INSTALL_PREFIX/bin/area-session"
echo "  - Systemd service: $SYSTEMD_USER_DIR/area.service"
echo "  - Desktop target: $SYSTEMD_USER_DIR/area-desktop.target"
echo "  - LightDM session: $XSESSION_DIR/area.desktop"
echo ""
echo "Architecture:"
echo "  ✓ Unified WM+Compositor (single process, no IPC)"
echo "  ✓ OpenGL hardware-accelerated rendering"
echo "  ✓ Damage-based rendering (0% CPU when idle)"
echo "  ✓ VSync-enabled compositor"
echo "  ✓ Window management (move, resize, close, maximize, minimize)"
echo "  ✓ Shell with panel and logout dialog"
echo ""
echo "Next steps:"
if [ "$INSTALL_TYPE" = "user" ]; then
    echo "  1. Log out from your current session"
    echo "  2. Select 'Area' from the LightDM session menu"
    echo "  3. Log in"
else
    echo "  1. All users can now select 'Area' from LightDM"
fi
echo ""
echo "Debugging:"
echo "  - View logs: journalctl --user -u area.service -f"
echo "  - Check status: systemctl --user status area.service"
echo "  - Manual test: DISPLAY=:0 $INSTALL_PREFIX/bin/area"
echo ""

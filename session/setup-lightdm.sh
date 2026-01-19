#!/bin/bash
# Setup script for Area Wayland LightDM session
# Builds the compositor and installs session files

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
COMPOSITOR_DIR="$PROJECT_ROOT/area-wayland"

echo "========================================="
echo "Area Wayland LightDM Setup"
echo "========================================="
echo ""

# Check if we're in the right directory
if [ ! -d "$COMPOSITOR_DIR" ]; then
    echo "Error: area-wayland directory not found at $COMPOSITOR_DIR"
    exit 1
fi

# Step 1: Build the compositor
echo "[1/3] Building compositor (release mode)..."
cd "$COMPOSITOR_DIR"
if ! cargo build --release; then
    echo "Error: Failed to build compositor"
    exit 1
fi
echo "✓ Build complete"
echo ""

# Step 2: Check if binary exists
BINARY_RELEASE="$COMPOSITOR_DIR/target/release/area-wayland"
BINARY_DEBUG="$COMPOSITOR_DIR/target/debug/area-wayland"

if [ -f "$BINARY_RELEASE" ]; then
    BINARY="$BINARY_RELEASE"
elif [ -f "$BINARY_DEBUG" ]; then
    BINARY="$BINARY_DEBUG"
    echo "Warning: Using debug binary (release build may have failed)"
else
    echo "Error: Compositor binary not found"
    exit 1
fi

echo "[2/3] Installing session files..."
echo ""

# Step 3: Copy desktop file
DESKTOP_FILE="$SCRIPT_DIR/area-wayland.desktop"
if [ ! -f "$DESKTOP_FILE" ]; then
    echo "Error: Desktop file not found at $DESKTOP_FILE"
    exit 1
fi

# Try user-specific installation first (no sudo needed)
USER_WAYLAND_SESSIONS="$HOME/.local/share/wayland-sessions"
if mkdir -p "$USER_WAYLAND_SESSIONS" 2>/dev/null; then
    cp "$DESKTOP_FILE" "$USER_WAYLAND_SESSIONS/"
    echo "✓ Installed desktop file to: $USER_WAYLAND_SESSIONS/area-wayland.desktop"
    INSTALLED_USER=true
else
    INSTALLED_USER=false
fi

# CRITICAL: System-wide installation is REQUIRED for LightDM
# LightDM runs as user 'lightdm', not the logged-in user, so it cannot
# read files from ~/.local/share/wayland-sessions/
SYSTEM_WAYLAND_SESSIONS="/usr/share/wayland-sessions"
INSTALLED_SYSTEM=false

if [ -w "$SYSTEM_WAYLAND_SESSIONS" ] 2>/dev/null; then
    # We have write access, install directly
    cp "$DESKTOP_FILE" "$SYSTEM_WAYLAND_SESSIONS/"
    echo "✓ Installed desktop file to: $SYSTEM_WAYLAND_SESSIONS/area-wayland.desktop"
    INSTALLED_SYSTEM=true
elif sudo -n true 2>/dev/null; then
    # Sudo available without password prompt
    if sudo cp "$DESKTOP_FILE" "$SYSTEM_WAYLAND_SESSIONS/" 2>/dev/null; then
        echo "✓ Installed desktop file to: $SYSTEM_WAYLAND_SESSIONS/area-wayland.desktop"
        INSTALLED_SYSTEM=true
    else
        echo "✗ Failed to install to system directory"
        INSTALLED_SYSTEM=false
    fi
else
    # Sudo requires password - can't do it automatically
    echo "✗ System-wide install requires sudo (password needed)"
    INSTALLED_SYSTEM=false
fi

# User installation is optional (for testing with gtk-launch, etc.)
# But system-wide is mandatory for LightDM
if [ "$INSTALLED_SYSTEM" = false ]; then
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "ERROR: System-wide installation failed!"
    echo "═══════════════════════════════════════════════════════════"
    echo ""
    echo "LightDM will NOT show the session without system-wide installation."
    echo "LightDM runs as user 'lightdm' and cannot read ~/.local/share/"
    echo ""
    echo "Please run one of these commands:"
    echo ""
    echo "  Option 1: Run the helper script (recommended):"
    echo "    sudo bash $SCRIPT_DIR/install-desktop-file.sh"
    echo ""
    echo "  Option 2: Manual installation:"
    echo "    sudo cp $DESKTOP_FILE $SYSTEM_WAYLAND_SESSIONS/"
    echo ""
    echo "  Option 3: Re-run this script with sudo:"
    echo "    sudo bash $SCRIPT_DIR/setup-lightdm.sh"
    echo ""
    exit 1
fi

# Verify LightDM can read it
echo ""
echo "Verifying LightDM can access the file..."
if sudo -u lightdm test -r "$SYSTEM_WAYLAND_SESSIONS/area-wayland.desktop" 2>/dev/null; then
    echo "✓ LightDM user can read the desktop file"
else
    echo "⚠ WARNING: Could not verify LightDM access (may need password)"
    echo "  File is installed, but please verify manually:"
    echo "  sudo -u lightdm test -r $SYSTEM_WAYLAND_SESSIONS/area-wayland.desktop"
fi

# Step 4: Update session script with correct binary path (if it has the default)
SESSION_SCRIPT="$SCRIPT_DIR/area-wayland-session"
if [ -f "$SESSION_SCRIPT" ]; then
    # Only update if the script still has a hardcoded path (not if it's already using $HOME)
    if grep -q "COMPOSITOR_BIN=\"/home/" "$SESSION_SCRIPT" 2>/dev/null; then
        sed -i "s|COMPOSITOR_BIN=\"/home/[^\"]*\"|COMPOSITOR_BIN=\"$BINARY\"|" "$SESSION_SCRIPT" 2>/dev/null || \
        sed -i '' "s|COMPOSITOR_BIN=\"/home/[^\"]*\"|COMPOSITOR_BIN=\"$BINARY\"|" "$SESSION_SCRIPT" 2>/dev/null || true
        echo "✓ Updated session script with binary path"
    else
        echo "✓ Session script already uses dynamic paths"
    fi
else
    echo "Warning: Session script not found at $SESSION_SCRIPT"
fi

echo ""
echo "[3/3] Verifying installation..."
echo ""

# Verify binary exists and is executable
if [ -f "$BINARY" ]; then
    chmod +x "$BINARY" 2>/dev/null || true
    echo "✓ Compositor binary: $BINARY"
    echo "  Size: $(du -h "$BINARY" | cut -f1)"
else
    echo "✗ Compositor binary not found"
    exit 1
fi

# Check if session file is in place
if [ -f "$USER_WAYLAND_SESSIONS/area-wayland.desktop" ] || [ -f "$SYSTEM_WAYLAND_SESSIONS/area-wayland.desktop" ]; then
    echo "✓ Session file installed"
else
    echo "✗ Session file not found"
    exit 1
fi

echo ""
echo "========================================="
echo "Setup Complete!"
echo "========================================="
echo ""
echo "Next steps:"
echo "  1. Log out from your current session"
echo "  2. Select 'Area Wayland' from the LightDM session menu"
echo "  3. Log in - the compositor will start and launch alacritty"
echo ""
echo "Note: The compositor is still in development."
echo "      Windows may not display until rendering is fully implemented."
echo ""

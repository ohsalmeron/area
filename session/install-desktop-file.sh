#!/bin/bash
# Quick script to install desktop file system-wide for LightDM
# This is REQUIRED because LightDM runs as user 'lightdm' and cannot
# read files from ~/.local/share/wayland-sessions/

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_FILE="$SCRIPT_DIR/area-wayland.desktop"
TARGET="/usr/share/wayland-sessions/area-wayland.desktop"
LOG_FILE="/home/bizkit/Documents/GitHub/area/.cursor/debug.log"

# Ensure log directory exists
mkdir -p "$(dirname "$LOG_FILE")" 2>/dev/null || true

# Logging function
log_debug() {
    local msg="$1"
    local data="$2"
    local timestamp=$(date +%s)000
    local id="install_${timestamp}_$$"
    local log_entry="{\"id\":\"$id\",\"timestamp\":$timestamp,\"location\":\"install-desktop-file.sh\",\"message\":\"$msg\",\"data\":$data,\"sessionId\":\"debug-session\",\"runId\":\"install-run\",\"hypothesisId\":\"A\"}"
    echo "$log_entry" >> "$LOG_FILE" 2>/dev/null || true
    # Also echo to stderr so user sees it
    echo "[LOG] $msg" >&2
}

log_debug "Installation script started" "{\"source\":\"$DESKTOP_FILE\",\"target\":\"$TARGET\",\"user\":\"$(whoami)\",\"script_dir\":\"$SCRIPT_DIR\"}"

# Verify source file exists
if [ ! -f "$DESKTOP_FILE" ]; then
    log_debug "Source file not found" "{\"path\":\"$DESKTOP_FILE\",\"error\":\"file_missing\",\"script_dir\":\"$SCRIPT_DIR\",\"ls_output\":\"$(ls -la \"$SCRIPT_DIR\" 2>&1 | head -5)\"}"
    echo "ERROR: Desktop file not found at $DESKTOP_FILE" >&2
    echo "Script directory contents:" >&2
    ls -la "$SCRIPT_DIR" >&2 || true
    exit 1
fi

log_debug "Source file verified" "{\"path\":\"$DESKTOP_FILE\",\"exists\":true,\"size\":$(stat -c%s "$DESKTOP_FILE" 2>/dev/null || echo 0)}"

echo "Installing desktop file system-wide..."
echo "Source: $DESKTOP_FILE"
echo "Target: $TARGET"
echo ""

# Check if already installed
if [ -f "$TARGET" ]; then
    log_debug "Target already exists" "{\"target\":\"$TARGET\",\"action\":\"will_overwrite\"}"
    echo "⚠ Target file already exists, will overwrite"
fi

# Install with sudo
log_debug "Attempting installation" "{\"command\":\"sudo cp\",\"source\":\"$DESKTOP_FILE\",\"target\":\"$TARGET\",\"target_dir\":\"$(dirname "$TARGET")\"}"
echo "Installing desktop file..." >&2
echo "  Source: $DESKTOP_FILE" >&2
echo "  Target: $TARGET" >&2
echo "  (This requires sudo - you may be prompted for your password)" >&2

if sudo cp "$DESKTOP_FILE" "$TARGET" 2>&1; then
    log_debug "Installation successful" "{\"target\":\"$TARGET\",\"status\":\"installed\"}"
    echo "✓ Installed to $TARGET" >&2
else
    local exit_code=$?
    log_debug "Installation failed" "{\"target\":\"$TARGET\",\"error\":\"sudo_failed\",\"exit_code\":$exit_code}"
    echo "✗ Installation failed (exit code: $exit_code)" >&2
    echo "  Make sure you have sudo access and entered the correct password." >&2
    exit 1
fi

# Verify permissions
echo ""
echo "Verifying installation..."
if [ -f "$TARGET" ]; then
    FILE_PERMS=$(stat -c "%a %U:%G" "$TARGET" 2>/dev/null || echo "unknown")
    log_debug "File exists after install" "{\"target\":\"$TARGET\",\"perms\":\"$FILE_PERMS\"}"
    echo "✓ File exists"
    ls -la "$TARGET"
else
    log_debug "File missing after install" "{\"target\":\"$TARGET\",\"error\":\"not_found\"}"
    echo "✗ File not found after installation"
    exit 1
fi

# Test if LightDM user can read it
echo ""
echo "Testing LightDM access..."
if sudo -u lightdm test -r "$TARGET" 2>/dev/null; then
    log_debug "LightDM access verified" "{\"target\":\"$TARGET\",\"lightdm_readable\":true,\"status\":\"success\"}"
    echo "✓ LightDM user can read the file"
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "SUCCESS! The session should now appear in LightDM."
    echo "═══════════════════════════════════════════════════════════"
    echo ""
    echo "Next steps:"
    echo "  1. Log out from your current session"
    echo "  2. Select 'Area Wayland' from the LightDM session menu"
    echo "  3. Log in - the compositor will start"
    echo ""
else
    log_debug "LightDM access failed" "{\"target\":\"$TARGET\",\"lightdm_readable\":false,\"error\":\"permission_denied\"}"
    echo "✗ WARNING: LightDM user cannot read the file"
    echo "  This may indicate a permission issue."
    echo "  File permissions: $(stat -c "%a %U:%G" "$TARGET" 2>/dev/null || echo "unknown")"
    exit 1
fi

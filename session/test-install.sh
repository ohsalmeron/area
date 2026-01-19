#!/bin/bash
# Test installation script with detailed logging
# This will help us understand why installation might be failing

set -e

LOG_FILE="/home/bizkit/Documents/GitHub/area/.cursor/debug.log"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_FILE="$SCRIPT_DIR/area-wayland.desktop"
TARGET="/usr/share/wayland-sessions/area-wayland.desktop"

# Logging function
log_debug() {
    local msg="$1"
    local data="$2"
    local timestamp=$(date +%s)000
    local id="test_${timestamp}_$$"
    echo "{\"id\":\"$id\",\"timestamp\":$timestamp,\"location\":\"test-install.sh\",\"message\":\"$msg\",\"data\":$data,\"sessionId\":\"debug-session\",\"runId\":\"test-install\"}" >> "$LOG_FILE" 2>/dev/null || true
    echo "[LOG] $msg" >&2
}

log_debug "Test installation started" "{\"source\":\"$DESKTOP_FILE\",\"target\":\"$TARGET\"}"

# Check source file
if [ ! -f "$DESKTOP_FILE" ]; then
    log_debug "Source file missing" "{\"path\":\"$DESKTOP_FILE\",\"error\":\"file_not_found\"}"
    echo "ERROR: Source file not found: $DESKTOP_FILE" >&2
    exit 1
fi
log_debug "Source file found" "{\"path\":\"$DESKTOP_FILE\",\"size\":$(stat -c%s "$DESKTOP_FILE" 2>/dev/null || echo 0)}"

# Check if we can write to target (requires sudo)
log_debug "Checking target directory" "{\"target_dir\":\"$(dirname "$TARGET")\"}"
if [ ! -d "$(dirname "$TARGET")" ]; then
    log_debug "Target directory missing" "{\"target_dir\":\"$(dirname "$TARGET")\",\"error\":\"directory_not_found\"}"
    echo "ERROR: Target directory does not exist: $(dirname "$TARGET")" >&2
    exit 1
fi
log_debug "Target directory exists" "{\"target_dir\":\"$(dirname "$TARGET")\"}"

# Check if file already exists
if [ -f "$TARGET" ]; then
    log_debug "Target file already exists" "{\"target\":\"$TARGET\",\"action\":\"will_overwrite\"}"
    echo "WARNING: Target file already exists, will overwrite" >&2
fi

# Attempt installation
log_debug "Attempting installation with sudo" "{\"command\":\"sudo cp\",\"source\":\"$DESKTOP_FILE\",\"target\":\"$TARGET\"}"
echo "Attempting to install with sudo..." >&2
echo "You may be prompted for your password." >&2

if sudo cp "$DESKTOP_FILE" "$TARGET"; then
    log_debug "Installation successful" "{\"target\":\"$TARGET\",\"status\":\"installed\"}"
    echo "SUCCESS: File installed" >&2
else
    log_debug "Installation failed" "{\"target\":\"$TARGET\",\"error\":\"sudo_failed\",\"exit_code\":$?}"
    echo "ERROR: Installation failed (exit code: $?)" >&2
    exit 1
fi

# Verify installation
if [ -f "$TARGET" ]; then
    FILE_PERMS=$(stat -c "%a %U:%G" "$TARGET" 2>/dev/null || echo "unknown")
    FILE_SIZE=$(stat -c%s "$TARGET" 2>/dev/null || echo 0)
    log_debug "File verified after install" "{\"target\":\"$TARGET\",\"perms\":\"$FILE_PERMS\",\"size\":$FILE_SIZE}"
    echo "VERIFIED: File exists with permissions: $FILE_PERMS" >&2
else
    log_debug "File missing after install" "{\"target\":\"$TARGET\",\"error\":\"verification_failed\"}"
    echo "ERROR: File not found after installation" >&2
    exit 1
fi

# Test LightDM access
log_debug "Testing LightDM user access" "{\"target\":\"$TARGET\"}"
if sudo -u lightdm test -r "$TARGET" 2>/dev/null; then
    log_debug "LightDM access verified" "{\"target\":\"$TARGET\",\"lightdm_readable\":true,\"status\":\"success\"}"
    echo "SUCCESS: LightDM user can read the file" >&2
else
    log_debug "LightDM access failed" "{\"target\":\"$TARGET\",\"lightdm_readable\":false,\"error\":\"permission_denied\"}"
    echo "WARNING: LightDM user cannot read the file" >&2
    echo "  File permissions: $FILE_PERMS" >&2
fi

log_debug "Test installation completed" "{\"target\":\"$TARGET\",\"status\":\"complete\"}"
echo "Installation test completed. Check $LOG_FILE for detailed logs." >&2

#!/bin/bash
# Minimal installation script - just installs the desktop file
# This is a simplified version for debugging

set -e

LOG_FILE="/home/bizkit/Documents/GitHub/area/.cursor/debug.log"
SRC="/home/bizkit/Documents/GitHub/area/session/area-wayland.desktop"
DST="/usr/share/wayland-sessions/area-wayland.desktop"

# Ensure log directory exists
mkdir -p "$(dirname "$LOG_FILE")" 2>/dev/null || true

# Logging function
log() {
    local msg="$1"
    local data="$2"
    local ts=$(date +%s)000
    local id="install_${ts}_$$"
    local entry="{\"id\":\"$id\",\"timestamp\":$ts,\"location\":\"install-now.sh\",\"message\":\"$msg\",\"data\":$data,\"sessionId\":\"debug-session\",\"runId\":\"install-now\",\"hypothesisId\":\"A\"}"
    echo "$entry" >> "$LOG_FILE" 2>/dev/null || true
    echo "[LOG] $msg" >&2
}

log "Script started" "{\"source\":\"$SRC\",\"target\":\"$DST\",\"user\":\"$(whoami)\"}"

# Check source
if [ ! -f "$SRC" ]; then
    log "Source file missing" "{\"path\":\"$SRC\",\"error\":\"file_not_found\"}"
    echo "ERROR: Source file not found: $SRC" >&2
    exit 1
fi
log "Source file found" "{\"path\":\"$SRC\",\"size\":$(stat -c%s "$SRC" 2>/dev/null || echo 0)}"

# Check target directory
if [ ! -d "$(dirname "$DST")" ]; then
    log "Target directory missing" "{\"target_dir\":\"$(dirname "$DST")\",\"error\":\"directory_not_found\"}"
    echo "ERROR: Target directory does not exist: $(dirname "$DST")" >&2
    exit 1
fi
log "Target directory exists" "{\"target_dir\":\"$(dirname "$DST")\"}"

# Install
echo "Installing desktop file..." >&2
echo "  Source: $SRC" >&2
echo "  Target: $DST" >&2
log "Attempting installation" "{\"command\":\"sudo cp\"}"

if sudo cp "$SRC" "$DST" 2>&1; then
    log "Installation successful" "{\"target\":\"$DST\"}"
    echo "✓ Installed successfully" >&2
else
    local exit_code=$?
    log "Installation failed" "{\"target\":\"$DST\",\"error\":\"sudo_failed\",\"exit_code\":$exit_code}"
    echo "✗ Installation failed (exit code: $exit_code)" >&2
    exit 1
fi

# Verify
if [ -f "$DST" ]; then
    PERMS=$(stat -c "%a %U:%G" "$DST" 2>/dev/null || echo "unknown")
    SIZE=$(stat -c%s "$DST" 2>/dev/null || echo 0)
    log "File verified" "{\"target\":\"$DST\",\"perms\":\"$PERMS\",\"size\":$SIZE}"
    echo "✓ File verified: $DST" >&2
    echo "  Permissions: $PERMS" >&2
else
    log "Verification failed" "{\"target\":\"$DST\",\"error\":\"not_found\"}"
    echo "✗ Verification failed - file not found" >&2
    exit 1
fi

# Test LightDM access
if sudo -u lightdm test -r "$DST" 2>/dev/null; then
    log "LightDM access OK" "{\"target\":\"$DST\",\"lightdm_readable\":true}"
    echo "✓ LightDM can read the file" >&2
else
    log "LightDM access failed" "{\"target\":\"$DST\",\"lightdm_readable\":false}"
    echo "⚠ WARNING: LightDM cannot read the file" >&2
fi

log "Installation complete" "{\"status\":\"success\",\"target\":\"$DST\"}"
echo "" >&2
echo "═══════════════════════════════════════════════════════════" >&2
echo "SUCCESS! Desktop file installed." >&2
echo "═══════════════════════════════════════════════════════════" >&2
echo "" >&2
echo "Next steps:" >&2
echo "  1. Restart LightDM: sudo systemctl restart lightdm.service" >&2
echo "  2. Log out and check the session menu" >&2
echo "  3. Select 'Area Wayland' from the menu" >&2

#!/bin/bash
# Installation script for Area Desktop session
# Builds navigator, builds unified area binary, and installs systemd service and LightDM session
# Tracks all errors and prints comprehensive success summary

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Error and success tracking
ERRORS=()
SUCCESSES=()
WARNINGS=()

# Function to log errors
log_error() {
    ERRORS+=("$1")
    echo -e "${RED}✗ ERROR: $1${NC}" >&2
}

# Function to log successes
log_success() {
    SUCCESSES+=("$1")
    echo -e "${GREEN}✓ $1${NC}"
}

# Function to log warnings
log_warning() {
    WARNINGS+=("$1")
    echo -e "${YELLOW}⚠ WARNING: $1${NC}"
}

# Function to run command and track errors
run_cmd() {
    local description="$1"
    shift
    echo -e "${BLUE}→ $description${NC}"
    if "$@" 2>&1; then
        log_success "$description"
        return 0
    else
        log_error "$description failed"
        return 1
    fi
}

echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}     Area Desktop Session Installer${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo ""

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

# ============================================================================
# Step 1: Build Navigator from xfce-rs
# ============================================================================
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}Step 1: Building Navigator (Application Launcher)${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"

NAVIGATOR_BUILD_DIR="$HOME/GitHub/xfce-rs"
NAVIGATOR_BINARY=""
NAVIGATOR_BUILT=false

# First, check if navigator already exists in standard locations
echo "Checking for existing navigator installation..."
if command -v navigator >/dev/null 2>&1; then
    NAVIGATOR_BINARY="$(command -v navigator)"
    log_success "Found existing navigator at: $NAVIGATOR_BINARY"
    NAVIGATOR_BUILT=true
elif [ -f "$HOME/.local/bin/navigator" ] && [ -x "$HOME/.local/bin/navigator" ]; then
    NAVIGATOR_BINARY="$HOME/.local/bin/navigator"
    log_success "Found existing navigator at: $NAVIGATOR_BINARY"
    NAVIGATOR_BUILT=true
elif [ -f "/usr/local/bin/navigator" ] && [ -x "/usr/local/bin/navigator" ]; then
    NAVIGATOR_BINARY="/usr/local/bin/navigator"
    log_success "Found existing navigator at: $NAVIGATOR_BINARY"
    NAVIGATOR_BUILT=true
fi

# If not found, try to find in various build locations
if [ "$NAVIGATOR_BUILT" = false ]; then
    echo "Searching for navigator in build directories..."
    POSSIBLE_PATHS=(
        "$HOME/GitHub/xfce-rs/target/release/navigator"
        "$HOME/GitHub/xfce-rs/navigator/target/release/navigator"
        "$HOME/GitHub/xfce-rs/area/xfce-rs/target/release/navigator"
        "$HOME/GitHub/xfce-rs/target/debug/navigator"
        "$HOME/GitHub/xfce-rs/navigator/target/debug/navigator"
    )
    
    for path in "${POSSIBLE_PATHS[@]}"; do
        if [ -f "$path" ] && [ -x "$path" ]; then
            NAVIGATOR_BINARY="$path"
            log_success "Found navigator binary at: $NAVIGATOR_BINARY"
            NAVIGATOR_BUILT=true
            break
        fi
    done
fi

# If still not found, try to build it
if [ "$NAVIGATOR_BUILT" = true ]; then
    echo "Using existing navigator: $NAVIGATOR_BINARY"
elif [ ! -d "$NAVIGATOR_BUILD_DIR" ]; then
    log_warning "Navigator build directory not found: $NAVIGATOR_BUILD_DIR"
    log_warning "Skipping navigator build - session will use existing navigator if available"
else
    echo "Navigator not found, attempting to build..."
    echo "Navigator build directory: $NAVIGATOR_BUILD_DIR"
    
    # Check if it's a workspace or single crate
    if [ -f "$NAVIGATOR_BUILD_DIR/Cargo.toml" ] && grep -q "^\[workspace\]" "$NAVIGATOR_BUILD_DIR/Cargo.toml" 2>/dev/null; then
        # Workspace build
        echo "Detected workspace structure"
        cd "$NAVIGATOR_BUILD_DIR"
        # Capture output to check for specific errors
        if cargo build --release --bin navigator 2>&1 | tee /tmp/navigator-build.log; then
            NAVIGATOR_BUILT=true
            # Check multiple possible locations after build
            POSSIBLE_BUILD_PATHS=(
                "$NAVIGATOR_BUILD_DIR/target/release/navigator"
                "$NAVIGATOR_BUILD_DIR/navigator/target/release/navigator"
                "$NAVIGATOR_BUILD_DIR/area/xfce-rs/target/release/navigator"
            )
            
            NAVIGATOR_FOUND=false
            for path in "${POSSIBLE_BUILD_PATHS[@]}"; do
                if [ -f "$path" ] && [ -x "$path" ]; then
                    NAVIGATOR_BINARY="$path"
                    log_success "Building navigator (workspace) - found at: $NAVIGATOR_BINARY"
                    NAVIGATOR_FOUND=true
                    break
                fi
            done
            
            if [ "$NAVIGATOR_FOUND" = false ]; then
                log_warning "Navigator build succeeded but binary not found in expected locations"
                log_warning "Searched: ${POSSIBLE_BUILD_PATHS[*]}"
                NAVIGATOR_BUILT=false
            fi
        else
            # Check if it's a dependency error (non-critical)
            if grep -q "failed to load manifest\|No such file or directory" /tmp/navigator-build.log 2>/dev/null; then
                log_warning "Navigator build skipped - missing workspace dependencies (xfce-rs-config, xfce-rs-utils, etc.)"
                log_warning "This is optional - Area session will work without navigator"
                log_warning "You can build navigator later or use an existing navigator installation"
            else
                log_warning "Navigator build failed - check /tmp/navigator-build.log for details"
            fi
            NAVIGATOR_BUILT=false
            rm -f /tmp/navigator-build.log
        fi
    elif [ -d "$NAVIGATOR_BUILD_DIR/navigator" ] && [ -f "$NAVIGATOR_BUILD_DIR/navigator/Cargo.toml" ]; then
        # Single crate build
        echo "Detected single crate structure"
        cd "$NAVIGATOR_BUILD_DIR/navigator"
        if cargo build --release 2>&1 | tee /tmp/navigator-build.log; then
            NAVIGATOR_BUILT=true
            POSSIBLE_SINGLE_PATHS=(
                "$NAVIGATOR_BUILD_DIR/navigator/target/release/navigator"
                "$NAVIGATOR_BUILD_DIR/target/release/navigator"
            )
            
            NAVIGATOR_FOUND=false
            for path in "${POSSIBLE_SINGLE_PATHS[@]}"; do
                if [ -f "$path" ] && [ -x "$path" ]; then
                    NAVIGATOR_BINARY="$path"
                    log_success "Building navigator (single crate) - found at: $NAVIGATOR_BINARY"
                    NAVIGATOR_FOUND=true
                    break
                fi
            done
            
            if [ "$NAVIGATOR_FOUND" = false ]; then
                log_warning "Navigator build succeeded but binary not found in expected locations"
                NAVIGATOR_BUILT=false
            fi
        else
            if grep -q "failed to load manifest\|No such file or directory" /tmp/navigator-build.log 2>/dev/null; then
                log_warning "Navigator build skipped - missing dependencies"
                log_warning "This is optional - Area session will work without navigator"
            else
                log_warning "Navigator build failed - check /tmp/navigator-build.log for details"
            fi
            NAVIGATOR_BUILT=false
            rm -f /tmp/navigator-build.log
        fi
    else
        log_warning "Could not determine navigator build structure in $NAVIGATOR_BUILD_DIR"
    fi
    
    if [ "$NAVIGATOR_BUILT" = true ] && [ -n "$NAVIGATOR_BINARY" ] && [ -f "$NAVIGATOR_BINARY" ]; then
        log_success "Navigator binary found at: $NAVIGATOR_BINARY"
        # Optionally install navigator to a standard location
        if [ "$INSTALL_TYPE" = "user" ]; then
            mkdir -p "$INSTALL_PREFIX/bin"
            if cp "$NAVIGATOR_BINARY" "$INSTALL_PREFIX/bin/navigator" 2>/dev/null; then
                chmod +x "$INSTALL_PREFIX/bin/navigator"
                log_success "Navigator installed to $INSTALL_PREFIX/bin/navigator"
            else
                log_warning "Could not install navigator to $INSTALL_PREFIX/bin (will use development path)"
            fi
        fi
    elif [ "$NAVIGATOR_BUILT" = true ]; then
        log_warning "Navigator build succeeded but binary not found in expected locations"
        NAVIGATOR_BUILT=false
    fi
fi

echo ""

# ============================================================================
# Step 2: Build Area WM+Compositor
# ============================================================================
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}Step 2: Building Area WM+Compositor${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"

cd "$PROJECT_ROOT"
AREA_BUILT=false

if run_cmd "Building unified area binary (WM + Compositor)" cargo build --release --bin area; then
    if [ -f "$PROJECT_ROOT/target/release/area" ]; then
        AREA_BUILT=true
        log_success "Area binary found at: $PROJECT_ROOT/target/release/area"
    else
        log_error "Area build succeeded but binary not found at expected location"
    fi
fi

if [ "$AREA_BUILT" = false ]; then
    log_error "Area binary build failed - cannot continue installation"
    echo ""
    echo -e "${RED}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${RED}Installation failed: Area binary is required${NC}"
    echo -e "${RED}═══════════════════════════════════════════════════════════${NC}"
    exit 1
fi

echo ""

# ============================================================================
# Step 3: Create Directories
# ============================================================================
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}Step 3: Creating Installation Directories${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"

run_cmd "Creating bin directory" mkdir -p "$INSTALL_PREFIX/bin"
run_cmd "Creating systemd user directory" mkdir -p "$SYSTEMD_USER_DIR"
# XSESSION_DIR creation handled by install command

echo ""

# ============================================================================
# Step 4: Install Area Binary
# ============================================================================
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}Step 4: Installing Area Binary${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"

if run_cmd "Installing area binary" install -m 755 "$PROJECT_ROOT/target/release/area" "$INSTALL_PREFIX/bin/area"; then
    # Verify installation
    if [ -x "$INSTALL_PREFIX/bin/area" ]; then
        log_success "Area binary verified at: $INSTALL_PREFIX/bin/area"
    else
        log_error "Area binary installed but not executable"
    fi
fi

echo ""

# ============================================================================
# Step 5: Install Systemd Services
# ============================================================================
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}Step 5: Installing Systemd Services${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"

if [ ! -f "$PROJECT_ROOT/session/area.service" ]; then
    log_error "area.service not found at $PROJECT_ROOT/session/area.service"
else
    if run_cmd "Installing area.service" install -m 644 "$PROJECT_ROOT/session/area.service" "$SYSTEMD_USER_DIR/area.service"; then
        # Update ExecStart path in service file
        if sed -i "s|ExecStart=/usr/local/bin/area|ExecStart=$INSTALL_PREFIX/bin/area|g" "$SYSTEMD_USER_DIR/area.service" 2>/dev/null; then
            log_success "Updated ExecStart path in area.service"
        else
            log_warning "Could not update ExecStart path in area.service (may need manual edit)"
        fi
    fi
fi

if [ ! -f "$PROJECT_ROOT/session/area-desktop.target" ]; then
    log_error "area-desktop.target not found at $PROJECT_ROOT/session/area-desktop.target"
else
    run_cmd "Installing area-desktop.target" install -m 644 "$PROJECT_ROOT/session/area-desktop.target" "$SYSTEMD_USER_DIR/area-desktop.target"
fi

echo ""

# ============================================================================
# Step 6: Install Session Script
# ============================================================================
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}Step 6: Installing Session Script${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"

if [ ! -f "$PROJECT_ROOT/session/area-session" ]; then
    log_error "area-session not found at $PROJECT_ROOT/session/area-session"
else
    run_cmd "Installing area-session script" install -m 755 "$PROJECT_ROOT/session/area-session" "$INSTALL_PREFIX/bin/area-session"
    # Verify it's executable
    if [ -x "$INSTALL_PREFIX/bin/area-session" ]; then
        log_success "area-session script verified and executable"
        log_success "Startup applications are configured in: $INSTALL_PREFIX/bin/area-session"
        log_success "  Edit the 'Launch Applications' section to add/remove apps"
    fi
fi

if [ ! -f "$PROJECT_ROOT/session/area-launch-app" ]; then
    log_warning "area-launch-app helper not found (optional)"
else
    run_cmd "Installing area-launch-app helper" install -m 755 "$PROJECT_ROOT/session/area-launch-app" "$INSTALL_PREFIX/bin/area-launch-app"
    if [ -x "$INSTALL_PREFIX/bin/area-launch-app" ]; then
        log_success "area-launch-app helper installed (for proper desktop file launching)"
    fi
fi

echo ""

# ============================================================================
# Step 7: Install LightDM Session File
# ============================================================================
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}Step 7: Installing LightDM Session File${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"

# LightDM session file must be in /usr/share/xsessions (requires sudo for user installs)
TEMP_DESKTOP=$(mktemp)
cat > "$TEMP_DESKTOP" << EOF
[Desktop Entry]
Name=Area
Comment=Area Window Manager + Compositor
Exec=$INSTALL_PREFIX/bin/area-session
Type=Application
DesktopNames=Area
EOF

if [ "$INSTALL_TYPE" = "user" ]; then
    echo "Note: Installing session file to $XSESSION_DIR requires sudo"
    if sudo install -m 644 "$TEMP_DESKTOP" "$XSESSION_DIR/area.desktop" 2>/dev/null; then
        log_success "LightDM session file installed (with sudo)"
    else
        log_error "Failed to install LightDM session file (sudo required)"
    fi
else
    if run_cmd "Installing LightDM session file" install -m 644 "$TEMP_DESKTOP" "$XSESSION_DIR/area.desktop"; then
        log_success "LightDM session file installed"
    fi
fi
rm -f "$TEMP_DESKTOP"

echo ""

# ============================================================================
# Step 8: Reload Systemd
# ============================================================================
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}Step 8: Reloading Systemd User Daemon${NC}"
echo -e "${YELLOW}═══════════════════════════════════════════════════════════${NC}"

if systemctl --user daemon-reload 2>/dev/null; then
    log_success "Systemd user daemon reloaded"
else
    log_warning "Could not reload systemd user daemon (may need to run manually)"
fi

echo ""

# ============================================================================
# Final Summary
# ============================================================================
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}                    INSTALLATION SUMMARY${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
echo ""

# Print Successes
if [ ${#SUCCESSES[@]} -gt 0 ]; then
    echo -e "${GREEN}✓ SUCCESSES (${#SUCCESSES[@]}):${NC}"
    for success in "${SUCCESSES[@]}"; do
        echo -e "  ${GREEN}✓${NC} $success"
    done
    echo ""
fi

# Print Warnings
if [ ${#WARNINGS[@]} -gt 0 ]; then
    echo -e "${YELLOW}⚠ WARNINGS (${#WARNINGS[@]}):${NC}"
    for warning in "${WARNINGS[@]}"; do
        echo -e "  ${YELLOW}⚠${NC} $warning"
    done
    echo ""
fi

# Print Errors
if [ ${#ERRORS[@]} -gt 0 ]; then
    echo -e "${RED}✗ ERRORS (${#ERRORS[@]}):${NC}"
    for error in "${ERRORS[@]}"; do
        echo -e "  ${RED}✗${NC} $error"
    done
    echo ""
fi

# Installation Status
# Navigator build failure is not critical - only fail if Area installation failed
AREA_CRITICAL_ERRORS=0
for error in "${ERRORS[@]}"; do
    # Skip navigator-related errors (case-insensitive)
    if [[ ! "$error" =~ [Nn]avigator ]]; then
        AREA_CRITICAL_ERRORS=$((AREA_CRITICAL_ERRORS + 1))
    fi
done

if [ $AREA_CRITICAL_ERRORS -eq 0 ]; then
    echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  ✓ INSTALLATION COMPLETE - ALL STEPS SUCCEEDED${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════${NC}"
    echo ""
    
    echo "Installed Components:"
    echo "  ✓ Area WM+Compositor binary: $INSTALL_PREFIX/bin/area"
    echo "  ✓ Session script: $INSTALL_PREFIX/bin/area-session"
    echo "  ✓ Systemd service: $SYSTEMD_USER_DIR/area.service"
    echo "  ✓ Desktop target: $SYSTEMD_USER_DIR/area-desktop.target"
    echo "  ✓ LightDM session: $XSESSION_DIR/area.desktop"
    if [ -n "$NAVIGATOR_BINARY" ] && [ "$NAVIGATOR_BUILT" = true ]; then
        echo "  ✓ Navigator launcher: $NAVIGATOR_BINARY"
        if [ -f "$INSTALL_PREFIX/bin/navigator" ]; then
            echo "    (also installed to: $INSTALL_PREFIX/bin/navigator)"
        fi
    else
        echo "  ⚠ Navigator: Not built (will use existing if available)"
    fi
    echo ""
    
    echo "Architecture:"
    echo "  ✓ Unified WM+Compositor (single process, no IPC)"
    echo "  ✓ OpenGL hardware-accelerated rendering"
    echo "  ✓ Damage-based rendering (0% CPU when idle)"
    echo "  ✓ VSync-enabled compositor"
    echo "  ✓ Window management (move, resize, close, maximize, minimize)"
    echo "  ✓ Shell with panel and logout dialog"
    echo "  ✓ Launcher: Super key launches navigator (configurable in ~/.config/area/config.toml)"
    echo ""
    
    echo "Next Steps:"
    if [ "$INSTALL_TYPE" = "user" ]; then
        echo "  1. Log out from your current session"
        echo "  2. Select 'Area' from the LightDM session menu"
        echo "  3. Log in"
    else
        echo "  1. All users can now select 'Area' from LightDM"
    fi
    echo ""
    
    echo "Debugging Commands:"
    echo "  - View logs: journalctl --user -u area.service -f"
    echo "  - Check status: systemctl --user status area.service"
    echo "  - Manual test: DISPLAY=:0 $INSTALL_PREFIX/bin/area"
    echo "  - Test navigator: $([ -n "$NAVIGATOR_BINARY" ] && echo "$NAVIGATOR_BINARY" || echo "navigator")"
    echo ""
    
    exit 0
else
    echo -e "${RED}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${RED}  ✗ INSTALLATION INCOMPLETE - CRITICAL ERRORS OCCURRED${NC}"
    echo -e "${RED}═══════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Critical errors (non-navigator):"
    for error in "${ERRORS[@]}"; do
        if [[ ! "$error" =~ [Nn]avigator ]]; then
            echo "  ✗ $error"
        fi
    done
    echo ""
    echo "Please review the errors above and fix them before using Area session."
    echo ""
    exit 1
fi

# Reinstalling Area Desktop for Testing

## Quick Reinstall (Full)

Rebuilds binaries and reinstalls everything:

```bash
./scripts/install-session.sh
```

This will:
1. Build release binaries (`cargo build --release`)
2. Install binaries to `~/.local/bin`
3. Update service files with correct paths
4. Install LightDM session file (requires sudo password)
5. Reload systemd user daemon

## Quick Service File Update Only

If you only changed service files and don't need to rebuild binaries:

```bash
# Update service files manually
cd /home/bizkit/GitHub/area
INSTALL_PREFIX="$HOME/.local"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"

# Update WM service
sed -e "s|ExecStart=/usr/local/bin/|ExecStart=$INSTALL_PREFIX/bin/|g" \
    -e "s|ExecStartPre=.*xdpyinfo.*|ExecStartPre=/bin/bash -c 'until xdpyinfo -display \${DISPLAY:-:0} >/dev/null 2>&1; do sleep 0.1; done'|" \
    session/area-wm.service > "$SYSTEMD_USER_DIR/area-wm.service"

# Update Shell service
sed "s|ExecStart=/usr/local/bin/|ExecStart=$INSTALL_PREFIX/bin/|g" \
    session/area-shell.service > "$SYSTEMD_USER_DIR/area-shell.service"

# Update session script
install -m 755 session/area-session "$INSTALL_PREFIX/bin/area-session"

# Reload systemd
systemctl --user daemon-reload
```

## Rebuild Binaries Only

If you only changed code and don't need to update service files:

```bash
# Build release binaries
cargo build --release

# Install binaries
install -m 755 target/release/area-wm ~/.local/bin/area-wm
install -m 755 target/release/area-shell ~/.local/bin/area-shell
```

## After Reinstalling

1. **Test the session files are correct:**
   ```bash
   # Check service files
   cat ~/.config/systemd/user/area-wm.service
   cat ~/.config/systemd/user/area-shell.service
   
   # Check session script
   cat ~/.local/bin/area-session
   ```

2. **Verify systemd can see them:**
   ```bash
   systemctl --user daemon-reload
   systemctl --user list-unit-files | grep area
   ```

3. **Test starting services manually (optional):**
   ```bash
   # Stop any running services first
   systemctl --user stop area-desktop.target
   
   # Start and check status
   systemctl --user start area-wm
   systemctl --user status area-wm
   ```

4. **Log out and test from LightDM:**
   - Log out from your current session
   - Select "Area" from LightDM session menu
   - Log in

## Troubleshooting

If services fail to start:

```bash
# Check logs
journalctl --user -u area-wm -n 50
journalctl --user -u area-shell -n 50

# Check service status
systemctl --user status area-wm
systemctl --user status area-shell

# Reset failed services
systemctl --user reset-failed area-wm area-shell
```



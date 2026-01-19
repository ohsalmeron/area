# Area Wayland Session Files

This directory contains the files needed to run Area Wayland as a LightDM session.

## Quick Start

Run the setup script:

```bash
./setup-lightdm.sh
```

That's it! Then log out and select "Area Wayland" from LightDM.

## Files

- `setup-lightdm.sh` - Automated setup script (run this!)
- `area-wayland.desktop` - LightDM session definition
- `area-wayland-session` - Session startup script

## What the setup script does

1. Builds the compositor (`cargo build --release`)
2. Copies the desktop file to `~/.local/share/xsessions/` (or `/usr/share/xsessions/` if you have sudo)
3. Updates the session script with the correct binary path
4. Verifies everything is in place

## Manual setup

See `../area-wayland/SESSION_SETUP.md` for manual installation instructions.

#!/bin/bash
# Quick reload script for area-focus (if it exists)
# This is a placeholder - area-focus functionality is now integrated into area

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo "ℹ️  Note: area-focus functionality is now integrated into the unified area binary"
echo "   Use scripts/reload-area.sh instead to reload the entire WM+Compositor"
echo ""
echo "   If you have a separate area-focus binary, you can reload it with:"
echo "   systemctl --user restart area-focus.service"
echo ""

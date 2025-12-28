# EWMH Implementation Status Report

## Overview
This document compares the EWMH specification checklist against the current implementation in the `area` window manager.

## ✅ IMPLEMENTED

### Client Message Handlers

#### 1. `_NET_CLOSE_WINDOW` ✅
- **Status**: Fully implemented
- **Location**: `src/main.rs:655-669`
- **Implementation**: 
  - Verifies window is managed
  - Uses `wm.close_window()` which sends `WM_DELETE_WINDOW` if supported, else `XKillClient`
  - Matches spec behavior

#### 2. `_NET_WM_STATE` ✅ (Partially)
- **Status**: Core states implemented, some advanced states are property-only
- **Location**: `src/main.rs:672-973`
- **Implemented States**:
  - ✅ `FULLSCREEN` - fully functional with compositor integration
  - ✅ `MAXIMIZED_VERT/HORZ` - fully functional
  - ✅ `HIDDEN` (minimize) - fully functional
  - ✅ `ABOVE/BELOW` - property updates work, visual stacking needs verification
  - ⚠️ `SHADED` - property-only (no visual "roll-up" yet)
  - ⚠️ `STICKY` - property-only (no workspace persistence yet)
  - ⚠️ `MODAL` - property-only (no special dialog handling yet)
  - ⚠️ `SKIP_PAGER` - property-only (no pager integration yet)
  - ⚠️ `SKIP_TASKBAR` - property-only (no taskbar integration yet)
  - ⚠️ `DEMANDS_ATTENTION` - property-only (no urgency hint handling yet)
- **Notes**: 
  - Maximized and fullscreen are mutually exclusive (spec requirement met)
  - Above and below are mutually exclusive (spec requirement met)

#### 3. `_NET_ACTIVE_WINDOW` ✅
- **Status**: Implemented
- **Location**: `src/main.rs:975-994`
- **Implementation**: 
  - Handles source indication and timestamp
  - Calls `wm.set_focus()` which updates `_NET_ACTIVE_WINDOW` root property
  - **Missing**: Desktop switching if window is on another desktop (see `_NET_CURRENT_DESKTOP` below)

#### 4. `_NET_REQUEST_FRAME_EXTENTS` ✅
- **Status**: Implemented
- **Location**: `src/main.rs:997-1024`
- **Implementation**: 
  - Calculates frame extents (top: 32, sides/bottom: 2)
  - Sets `_NET_FRAME_EXTENTS` property immediately
  - Works for both managed and unmanaged windows

### Root Window Properties

#### 5. `_NET_CLIENT_LIST` ✅
- **Status**: Implemented
- **Location**: `src/main.rs:1807-1809`, `src/wm/ewmh.rs:255-270`
- **Implementation**: 
  - Updated on window map/unmap
  - Uses `update_client_list()` helper
  - **Missing**: `_NET_CLIENT_LIST_STACKING` (reverse stacking order)

#### 6. `_NET_ACTIVE_WINDOW` (root property) ✅
- **Status**: Implemented
- **Location**: `src/wm/ewmh.rs:237-253`, `src/wm/mod.rs:1058`
- **Implementation**: Updated when focus changes

## ❌ MISSING / INCOMPLETE

### Client Message Handlers

#### 7. `_NET_MOVERESIZE_WINDOW` ❌
- **Status**: Not implemented
- **Purpose**: Absolute move/resize with gravity
- **Data Format**: flags + x + y + w + h
- **xfwm4 twist**: Refuses to move maximized windows unless flag 1<<12 (USER_POS) is set
- **Priority**: High (used by wmctrl, pagers, wine/steam games)

#### 8. `_NET_WM_MOVERESIZE` ❌
- **Status**: Not implemented
- **Purpose**: Interactive move/resize (drag-to-move initiated by app)
- **Data Format**: root-x, root-y, direction (move, size-*, keyboard)
- **xfwm4 twist**: Honors `_NET_WM_SYNC_REQUEST` for smooth resizing
- **Priority**: Medium (used by some apps for custom drag behavior)

#### 9. `_NET_WM_FULLSCREEN_MONITORS` ❌
- **Status**: Not implemented
- **Purpose**: Fullscreen spanning specific monitors
- **Data Format**: top, bottom, left, right monitor indices
- **xfwm4 twist**: Falls back to primary monitor if requested set is invalid
- **Priority**: Medium (used by SDL, wine, mpv, games)

### Root Window Properties

#### 10. `_NET_CLIENT_LIST_STACKING` ❌
- **Status**: Not implemented
- **Purpose**: Client list in reverse stacking order
- **Priority**: Medium (used by pagers, taskbars)

#### 11. `_NET_NUMBER_OF_DESKTOPS` ❌
- **Status**: Atom exists but property not updated
- **Location**: Atom defined in `src/wm/ewmh.rs:97`
- **Purpose**: Total number of virtual workspaces
- **Priority**: Low (workspaces not yet implemented)

#### 12. `_NET_CURRENT_DESKTOP` ❌
- **Status**: Atom exists but property not updated
- **Location**: Atom defined in `src/wm/ewmh.rs:98`
- **Purpose**: Active workspace index
- **Priority**: Low (workspaces not yet implemented)
- **Note**: Required for `_NET_ACTIVE_WINDOW` to switch desktops

#### 13. `_NET_WM_DESKTOP` (per-window) ❌
- **Status**: Atom exists but property not updated
- **Location**: Atom defined in `src/wm/ewmh.rs:101`
- **Purpose**: Window's workspace assignment
- **Special Value**: 0xFFFFFFFF = sticky (all workspaces)
- **Priority**: Low (workspaces not yet implemented)

### PropertyNotify Handlers

#### 14. `_NET_WM_NAME` ⚠️
- **Status**: Atom exists, handler missing
- **Location**: Atom defined in `src/wm/ewmh.rs:100`
- **Purpose**: Update title-bar/taskbar text when window title changes
- **Priority**: Medium (taskbars need this)

#### 15. `_NET_WM_DESKTOP` ⚠️
- **Status**: Atom exists, handler missing
- **Location**: Atom defined in `src/wm/ewmh.rs:101`
- **Purpose**: Pager needs to move window icon when desktop changes
- **Priority**: Low (workspaces not yet implemented)

#### 16. `_NET_WM_WINDOW_TYPE` ⚠️
- **Status**: Atom exists, handler missing
- **Location**: Atom defined in `src/wm/ewmh.rs:102`
- **Purpose**: Handle window type changes on the fly (rare but possible)
- **Priority**: Low

#### 17. `_NET_WM_STRUT` / `_NET_WM_STRUT_PARTIAL` ⚠️
- **Status**: Atoms exist, handlers missing
- **Location**: Atoms defined in `src/wm/ewmh.rs:150-151`
- **Purpose**: Recompute work-area when panel moves
- **Priority**: Medium (panels need this for proper window placement)

## 📋 IMPLEMENTATION PRIORITY

### High Priority (Core Functionality)
1. ✅ `_NET_CLOSE_WINDOW` - DONE
2. ✅ `_NET_WM_STATE` (core states) - DONE
3. ✅ `_NET_ACTIVE_WINDOW` - DONE
4. ❌ `_NET_MOVERESIZE_WINDOW` - **TODO** (used by wmctrl, pagers, games)

### Medium Priority (Better Compatibility)
5. ✅ `_NET_REQUEST_FRAME_EXTENTS` - DONE
6. ❌ `_NET_CLIENT_LIST_STACKING` - **TODO** (pagers need this)
7. ⚠️ `_NET_WM_NAME` PropertyNotify - **TODO** (taskbars need this)
8. ⚠️ `_NET_WM_STRUT` PropertyNotify - **TODO** (panels need this)
9. ❌ `_NET_WM_MOVERESIZE` - **TODO** (some apps use this)

### Low Priority (Advanced Features)
10. ❌ `_NET_WM_FULLSCREEN_MONITORS` - **TODO** (multi-monitor fullscreen)
11. ❌ Workspace-related properties - **TODO** (when workspaces are implemented)
12. ⚠️ Advanced `_NET_WM_STATE` handlers (SHADED, STICKY, etc.) - **TODO** (visual implementation)

## 🔍 SPEC ACCURACY CHECK

The specification provided is **still accurate and relevant**. The EWMH standard hasn't changed significantly, and the behaviors described (especially xfwm4-specific twists) are still correct.

### Notes on Spec Accuracy:
- ✅ All described client messages are still valid
- ✅ All described root properties are still valid
- ✅ xfwm4-specific behaviors are still accurate
- ✅ PropertyNotify requirements are still valid

## 🎯 RECOMMENDED NEXT STEPS

1. **Implement `_NET_MOVERESIZE_WINDOW`** - High impact, used by many tools
2. **Add `_NET_CLIENT_LIST_STACKING`** - Easy to implement, improves pager compatibility
3. **Add PropertyNotify handlers** for `_NET_WM_NAME` and `_NET_WM_STRUT` - Improves panel/taskbar integration
4. **Implement `_NET_WM_MOVERESIZE`** - For better app compatibility
5. **Add workspace support** - Then implement workspace-related properties

## 📝 CODE LOCATIONS

- **Client Message Handlers**: `src/main.rs:654-1044`
- **EWMH Atoms**: `src/wm/ewmh.rs:14-84`
- **EWMH Helpers**: `src/wm/ewmh.rs:86-522`
- **PropertyNotify**: `src/main.rs:1445-1478`
- **Root Property Updates**: `src/main.rs:1800-1809`, `src/wm/mod.rs:1058`




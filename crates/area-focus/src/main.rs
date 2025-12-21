// Focus manager with keyboard shortcuts
// Handles click-to-focus, Alt+Tab switching, F11 fullscreen, Alt+F4 close, Super for Navigator
// Based on Compiz's focus handling approach

use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::properties::WmHints;
use x11rb::rust_connection::RustConnection;
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::CURRENT_TIME;

fn find_toplevel_window(conn: &RustConnection, mut window: u32, root: u32) -> Result<u32> {
    loop {
        // If this window is the root, we're done
        if window == root {
            return Ok(root);
        }
        
        // Get parent
        let tree = conn.query_tree(window)?.reply()?;
        
        // If parent is root, this is a toplevel window
        if tree.parent == root {
            return Ok(window);
        }
        
        window = tree.parent;
    }
}

fn window_has_input_hint(conn: &RustConnection, window: u32) -> Result<bool> {
    // Get WM_HINTS property using x11rb's helper
    match WmHints::get(conn, window)?.reply()? {
        Some(hints) => {
            // If input hint is set, use its value; otherwise default to true
            Ok(hints.input.unwrap_or(true))
        }
        None => {
            // No WM_HINTS means default to accepting input
            Ok(true)
        }
    }
}

fn window_supports_take_focus(conn: &RustConnection, window: u32) -> Result<bool> {
    // Get WM_PROTOCOLS property
    let wm_protocols_atom = conn.intern_atom(false, b"WM_PROTOCOLS")?.reply()?;
    let wm_take_focus_atom = conn.intern_atom(false, b"WM_TAKE_FOCUS")?.reply()?;
    
    let reply = conn.get_property(
        false,
        window,
        wm_protocols_atom.atom,
        AtomEnum::ATOM,
        0,
        32,
    )?.reply()?;
    
    if reply.value.is_empty() {
        return Ok(false);
    }
    
    // Check if WM_TAKE_FOCUS is in the list
    for chunk in reply.value.chunks(4) {
        if chunk.len() == 4 {
            let atom = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if atom == wm_take_focus_atom.atom {
                return Ok(true);
            }
        }
    }
    
    Ok(false)
}

fn send_take_focus(conn: &RustConnection, window: u32) -> Result<()> {
    let wm_protocols_atom = conn.intern_atom(false, b"WM_PROTOCOLS")?.reply()?;
    let wm_take_focus_atom = conn.intern_atom(false, b"WM_TAKE_FOCUS")?.reply()?;
    
    // Send ClientMessage with WM_TAKE_FOCUS
    let event = ClientMessageEvent::new(
        32, // format
        window,
        wm_protocols_atom.atom,
        [
            wm_take_focus_atom.atom,
            CURRENT_TIME,
            0,
            0,
            0,
        ],
    );
    
    conn.send_event(
        false,
        window,
        EventMask::NO_EVENT,
        &event,
    )?;
    conn.flush()?;
    
    Ok(())
}

fn is_terminal_window(conn: &RustConnection, window: u32) -> bool {
    // Check WM_CLASS for terminal applications
    match conn.get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => {
                if let Ok(class) = String::from_utf8(reply.value) {
                    let class_lower = class.to_lowercase();
                    // Check for common terminal applications
                    class_lower.contains("alacritty") || 
                    class_lower.contains("xterm") ||
                    class_lower.contains("urxvt") ||
                    class_lower.contains("gnome-terminal") ||
                    class_lower.contains("xfce4-terminal") ||
                    class_lower.contains("konsole") ||
                    class_lower.contains("kitty")
                } else {
                    false
                }
            }
            Err(_) => false,
        },
        Err(_) => false,
    }
}

fn set_focus_to_window(conn: &RustConnection, window: u32) -> Result<()> {
    // Check if window accepts input
    let attrs = conn.get_window_attributes(window)?.reply()?;
    
    // Skip unmapped windows
    if attrs.map_state != MapState::VIEWABLE {
        tracing::debug!("Window {} is not viewable (map_state: {:?})", window, attrs.map_state);
        return Ok(());
    }
    
    // Ensure window is mapped
    if attrs.map_state == MapState::UNMAPPED {
        tracing::info!("Mapping window {}", window);
        conn.map_window(window)?;
        conn.flush()?;
    }
    
    // Raise the window to top
    conn.configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
    conn.flush()?;
    
    // Ensure the window can receive keyboard events by selecting KeyPress events
    // This is important for some applications
    conn.change_window_attributes(
        window,
        &ChangeWindowAttributesAux::new().event_mask(
            EventMask::KEY_PRESS | EventMask::KEY_RELEASE | EventMask::FOCUS_CHANGE
        ),
    )?;
    conn.flush()?;
    
    // Check if window supports WM_TAKE_FOCUS protocol
    let supports_take_focus = window_supports_take_focus(conn, window)?;
    
    // Check input hint
    let has_input_hint = window_has_input_hint(conn, window)?;
    
    tracing::info!("Focusing window {}: input_hint={}, take_focus={}", window, has_input_hint, supports_take_focus);
    
    let mut set_focus = false;
    
    // If window has input hint, set focus directly (like Compiz does)
    if has_input_hint {
        conn.set_input_focus(InputFocus::POINTER_ROOT, window, CURRENT_TIME)?;
        conn.flush()?;
        set_focus = true;
        tracing::info!("✓ Set input focus to window {} (has input hint)", window);
    }
    
    // If window supports WM_TAKE_FOCUS, send the message (can be done in addition to direct focus)
    if supports_take_focus {
        if let Err(e) = send_take_focus(conn, window) {
            tracing::warn!("Failed to send WM_TAKE_FOCUS to window {}: {}", window, e);
        } else {
            tracing::info!("✓ Sent WM_TAKE_FOCUS to window {}", window);
            set_focus = true;
        }
    }
    
    // If neither worked, try setting focus anyway (some apps might accept it)
    if !set_focus {
        conn.set_input_focus(InputFocus::POINTER_ROOT, window, CURRENT_TIME)?;
        conn.flush()?;
        tracing::info!("✓ Set input focus to window {} (fallback)", window);
    }
    
    // Update _NET_ACTIVE_WINDOW (important for EWMH compliance)
    let root = conn.setup().roots[0].root;
    let net_active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?;
    let event = ClientMessageEvent::new(
        32,
        root,
        net_active_window.atom,
        [2, CURRENT_TIME, 0, 0, 0], // source = 2 (application), timestamp
    );
    conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, &event)?;
    conn.flush()?;
    
    Ok(())
}

fn get_client_list(conn: &RustConnection, root: u32) -> Result<Vec<u32>> {
    // Query the window tree directly (like Compiz does)
    // This is more reliable than _NET_CLIENT_LIST which we'd need to maintain ourselves
    let tree = conn.query_tree(root)?.reply()?;
    
    let mut windows = Vec::new();
    
    // Filter to only toplevel windows (children of root)
    for &child in &tree.children {
        // Skip our own WM window and other special windows
        let attrs = match conn.get_window_attributes(child) {
            Ok(cookie) => match cookie.reply() {
                Ok(attrs) => attrs,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        
        // Skip override_redirect windows (they're usually special like panels)
        // But include them if they're normal application windows
        if attrs.override_redirect {
            // Check if it's a normal window by looking for WM_CLASS
            match conn.get_property(false, child, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256) {
                Ok(cookie) => match cookie.reply() {
                    Ok(_) => {
                        // Has WM_CLASS, might be a normal window
                        windows.push(child);
                    }
                    Err(_) => {
                        // No WM_CLASS, probably a special window, skip it
                        continue;
                    }
                },
                Err(_) => continue,
            }
        } else {
            // Normal managed window
            windows.push(child);
        }
    }
    
    Ok(windows)
}

fn get_active_window(conn: &RustConnection, root: u32) -> Result<Option<u32>> {
    let net_active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?;
    
    let reply = conn.get_property(
        false,
        root,
        net_active_window.atom,
        AtomEnum::WINDOW,
        0,
        1,
    )?.reply()?;
    
    if reply.value.len() >= 4 {
        let window = u32::from_ne_bytes([reply.value[0], reply.value[1], reply.value[2], reply.value[3]]);
        if window != 0 {
            return Ok(Some(window));
        }
    }
    
    Ok(None)
}

fn get_window_title(conn: &RustConnection, window: u32) -> String {
    // Try _NET_WM_NAME first
    let net_wm_name = match conn.intern_atom(false, b"_NET_WM_NAME") {
        Ok(cookie) => match cookie.reply() {
            Ok(atom) => atom.atom,
            Err(_) => return format!("Window {}", window),
        },
        Err(_) => return format!("Window {}", window),
    };
    
    let utf8_string = match conn.intern_atom(false, b"UTF8_STRING") {
        Ok(cookie) => match cookie.reply() {
            Ok(atom) => atom.atom,
            Err(_) => return format!("Window {}", window),
        },
        Err(_) => return format!("Window {}", window),
    };
    
    match conn.get_property(false, window, net_wm_name, utf8_string, 0, 256) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => {
                if let Ok(title) = String::from_utf8(reply.value) {
                    if !title.is_empty() {
                        return title;
                    }
                }
            }
            Err(_) => {}
        },
        Err(_) => {}
    }
    
    // Fallback to WM_NAME
    match conn.get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => {
                if let Ok(title) = String::from_utf8(reply.value) {
                    if !title.is_empty() {
                        return title;
                    }
                }
            }
            Err(_) => {}
        },
        Err(_) => {}
    }
    
    format!("Window {}", window)
}

fn is_window_mapped(conn: &RustConnection, window: u32) -> bool {
    match conn.get_window_attributes(window) {
        Ok(cookie) => match cookie.reply() {
            Ok(attrs) => attrs.map_state == MapState::VIEWABLE,
            Err(_) => false,
        },
        Err(_) => false,
    }
}

fn get_switchable_windows(conn: &RustConnection, root: u32) -> Result<Vec<u32>> {
    let all_windows = get_client_list(conn, root)?;
    
    // Filter to only mapped, normal windows
    let mut switchable = Vec::new();
    for window in all_windows {
        if is_window_mapped(conn, window) {
            // Skip desktop and dock windows
            let net_wm_window_type = match conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE") {
                Ok(cookie) => match cookie.reply() {
                    Ok(atom) => atom.atom,
                    Err(_) => {
                        switchable.push(window);
                        continue;
                    }
                },
                Err(_) => {
                    switchable.push(window);
                    continue;
                }
            };
            
            let dock_atom = match conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE_DOCK") {
                Ok(cookie) => match cookie.reply() {
                    Ok(atom) => atom.atom,
                    Err(_) => {
                        switchable.push(window);
                        continue;
                    }
                },
                Err(_) => {
                    switchable.push(window);
                    continue;
                }
            };
            
            let desktop_atom = match conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE_DESKTOP") {
                Ok(cookie) => match cookie.reply() {
                    Ok(atom) => atom.atom,
                    Err(_) => {
                        switchable.push(window);
                        continue;
                    }
                },
                Err(_) => {
                    switchable.push(window);
                    continue;
                }
            };
            
            match conn.get_property(false, window, net_wm_window_type, AtomEnum::ATOM, 0, 1) {
                Ok(cookie) => match cookie.reply() {
                    Ok(reply) => {
                        if reply.value.len() >= 4 {
                            let window_type = u32::from_ne_bytes([
                                reply.value[0], reply.value[1], reply.value[2], reply.value[3]
                            ]);
                            if window_type != dock_atom && window_type != desktop_atom {
                                switchable.push(window);
                            }
                        } else {
                            switchable.push(window);
                        }
                    }
                    Err(_) => {
                        switchable.push(window);
                    }
                },
                Err(_) => {
                    switchable.push(window);
                }
            }
        }
    }
    
    Ok(switchable)
}

fn switch_to_next_window(conn: &RustConnection, root: u32) -> Result<()> {
    let windows = get_switchable_windows(conn, root)?;
    if windows.is_empty() {
        return Ok(());
    }
    
    let current = get_active_window(conn, root)?.unwrap_or(0);
    
    // Find current window index
    let current_idx = windows.iter().position(|&w| w == current);
    
    let next_idx = if let Some(idx) = current_idx {
        (idx + 1) % windows.len()
    } else {
        0
    };
    
    let next_window = windows[next_idx];
    set_focus_to_window(conn, next_window)?;
    
    // Update _NET_ACTIVE_WINDOW
    let net_active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?;
    let event = ClientMessageEvent::new(
        32,
        root,
        net_active_window.atom,
        [2, CURRENT_TIME, 0, 0, 0], // source = 2 (application)
    );
    conn.send_event(false, next_window, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, &event)?;
    conn.flush()?;
    
    tracing::info!("Switched to window {}: {}", next_window, get_window_title(conn, next_window));
    Ok(())
}

fn toggle_fullscreen(conn: &RustConnection, root: u32) -> Result<()> {
    let window = get_active_window(conn, root)?.ok_or_else(|| anyhow::anyhow!("No active window"))?;
    
    let net_wm_state = conn.intern_atom(false, b"_NET_WM_STATE")?.reply()?;
    let net_wm_state_fullscreen = conn.intern_atom(false, b"_NET_WM_STATE_FULLSCREEN")?.reply()?;
    
    // Get current state
    let reply = conn.get_property(
        false,
        window,
        net_wm_state.atom,
        AtomEnum::ATOM,
        0,
        32,
    )?.reply()?;
    
    let mut states = Vec::new();
    for chunk in reply.value.chunks(4) {
        if chunk.len() == 4 {
            let atom = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            states.push(atom);
        }
    }
    
    let is_fullscreen = states.contains(&net_wm_state_fullscreen.atom);
    
    // Toggle fullscreen
    let action = if is_fullscreen { 0 } else { 1 }; // 0 = remove, 1 = add
    let event = ClientMessageEvent::new(
        32,
        root,
        net_wm_state.atom,
        [
            action as u32,
            net_wm_state_fullscreen.atom,
            0,
            0,
            0,
        ],
    );
    conn.send_event(false, window, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, &event)?;
    conn.flush()?;
    
    tracing::info!("Toggled fullscreen for window {}", window);
    Ok(())
}

fn close_window(conn: &RustConnection, root: u32) -> Result<()> {
    let window = get_active_window(conn, root)?.ok_or_else(|| anyhow::anyhow!("No active window"))?;
    
    // Try WM_DELETE_WINDOW first (graceful close)
    let wm_protocols = conn.intern_atom(false, b"WM_PROTOCOLS")?.reply()?;
    let wm_delete_window = conn.intern_atom(false, b"WM_DELETE_WINDOW")?.reply()?;
    
    // Check if window supports WM_DELETE_WINDOW
    let reply = conn.get_property(
        false,
        window,
        wm_protocols.atom,
        AtomEnum::ATOM,
        0,
        32,
    )?.reply()?;
    
    let mut supports_delete = false;
    for chunk in reply.value.chunks(4) {
        if chunk.len() == 4 {
            let atom = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if atom == wm_delete_window.atom {
                supports_delete = true;
                break;
            }
        }
    }
    
    if supports_delete {
        // Send WM_DELETE_WINDOW message
        let event = ClientMessageEvent::new(
            32,
            window,
            wm_protocols.atom,
            [wm_delete_window.atom, CURRENT_TIME, 0, 0, 0],
        );
        conn.send_event(false, window, EventMask::NO_EVENT, &event)?;
        conn.flush()?;
        tracing::info!("Sent WM_DELETE_WINDOW to window {}", window);
    } else {
        // Fallback: use _NET_CLOSE_WINDOW
        let net_close_window = conn.intern_atom(false, b"_NET_CLOSE_WINDOW")?.reply()?;
        let event = ClientMessageEvent::new(
            32,
            root,
            net_close_window.atom,
            [CURRENT_TIME, 2, 0, 0, 0], // source = 2 (application)
        );
        conn.send_event(false, window, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, &event)?;
        conn.flush()?;
        tracing::info!("Sent _NET_CLOSE_WINDOW to window {}", window);
    }
    
    Ok(())
}

fn find_navigator_window(conn: &RustConnection, root: u32) -> Result<Option<u32>> {
    let windows = get_client_list(conn, root)?;
    
    for window in windows {
        let title = get_window_title(conn, window);
        let title_lower = title.to_lowercase();
        if title_lower.contains("navigator") {
            return Ok(Some(window));
        }
        
        // Also check WM_CLASS
        match conn.get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256) {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => {
                    if let Ok(class) = String::from_utf8(reply.value) {
                        let class_lower = class.to_lowercase();
                        if class_lower.contains("navigator") {
                            return Ok(Some(window));
                        }
                    }
                }
                Err(_) => {}
            },
            Err(_) => {}
        }
    }
    
    Ok(None)
}

fn toggle_navigator(conn: &RustConnection, root: u32) -> Result<()> {
    tracing::info!("Looking for Navigator window...");
    let windows = get_client_list(conn, root)?;
    tracing::info!("Found {} windows in client list", windows.len());
    
    if let Some(window) = find_navigator_window(conn, root)? {
        tracing::info!("Found Navigator window: {}", window);
        
        // Check if window is mapped
        let attrs = conn.get_window_attributes(window)?.reply()?;
        tracing::info!("Window {} map_state: {:?}", window, attrs.map_state);
        
        if attrs.map_state == MapState::VIEWABLE {
            // Window is visible, hide it
            tracing::info!("Hiding Navigator window {}", window);
            conn.unmap_window(window)?;
            conn.flush()?;
            tracing::info!("✓ Navigator window {} hidden", window);
        } else {
            // Window is hidden, show it
            tracing::info!("Showing Navigator window {}", window);
            conn.map_window(window)?;
            conn.flush()?;
            
            tracing::info!("Raising Navigator window {}", window);
            conn.configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
            
            tracing::info!("Setting focus to Navigator window {}", window);
            set_focus_to_window(conn, window)?;
            
            // Update _NET_ACTIVE_WINDOW
            let net_active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?;
            let event = ClientMessageEvent::new(
                32,
                root,
                net_active_window.atom,
                [2, CURRENT_TIME, 0, 0, 0],
            );
            conn.send_event(false, window, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, &event)?;
            conn.flush()?;
            
            tracing::info!("✓ Navigator window {} shown and focused", window);
        }
        Ok(())
    } else {
        tracing::warn!("✗ Navigator window not found in {} windows", windows.len());
        // List all windows for debugging
        for window in windows {
            let title = get_window_title(conn, window);
            tracing::debug!("  Window {}: '{}'", window, title);
        }
        Ok(())
    }
}

// Keysym constants from X11/keysymdef.h
const XK_TAB: u32 = 0xFF09;
const XK_F4: u32 = 0xFFC3;
const XK_F11: u32 = 0xFFC8;
const XK_SUPER_L: u32 = 0xFFEB;
const XK_SUPER_R: u32 = 0xFFEC;

fn keysym_to_keycode(conn: &RustConnection, keysym: u32) -> Result<Vec<u8>> {
    // Get keyboard mapping
    let setup = conn.setup();
    let min_keycode = setup.min_keycode;
    let max_keycode = setup.max_keycode;
    
    let reply = conn.get_keyboard_mapping(min_keycode, (max_keycode - min_keycode + 1) as u8)?.reply()?;
    
    let mut keycodes = Vec::new();
    let keysyms_per_keycode = reply.keysyms_per_keycode as usize;
    
    for keycode in min_keycode..=max_keycode {
        let idx = ((keycode - min_keycode) as usize) * keysyms_per_keycode;
        if idx < reply.keysyms.len() {
            // Check all keysyms for this keycode (considering shift states)
            for i in 0..keysyms_per_keycode {
                if idx + i < reply.keysyms.len() {
                    if reply.keysyms[idx + i] == keysym {
                        keycodes.push(keycode);
                        break;
                    }
                }
            }
        }
    }
    
    Ok(keycodes)
}

fn keycode_to_keysym(conn: &RustConnection, keycode: u8) -> Option<u32> {
    let setup = conn.setup();
    let min_keycode = setup.min_keycode;
    
    let cookie = match conn.get_keyboard_mapping(min_keycode, 1) {
        Ok(c) => c,
        Err(_) => return None,
    };
    
    let reply = match cookie.reply() {
        Ok(r) => r,
        Err(_) => return None,
    };
    
    let keysyms_per_keycode = reply.keysyms_per_keycode as usize;
    let idx = ((keycode - min_keycode) as usize) * keysyms_per_keycode;
    
    if idx < reply.keysyms.len() {
        Some(reply.keysyms[idx])
    } else {
        None
    }
}

fn setup_keyboard_grabs(conn: &RustConnection, root: u32) -> Result<()> {
    tracing::info!("Setting up keyboard grabs...");
    
    // Get keycodes dynamically using keysym values
    let tab_keycodes = keysym_to_keycode(conn, XK_TAB)?;
    let f11_keycodes = keysym_to_keycode(conn, XK_F11)?;
    let f4_keycodes = keysym_to_keycode(conn, XK_F4)?;
    let super_l_keycodes = keysym_to_keycode(conn, XK_SUPER_L)?;
    let super_r_keycodes = keysym_to_keycode(conn, XK_SUPER_R)?;
    
    tracing::info!("Found keycodes - Tab: {:?}, F11: {:?}, F4: {:?}, Super_L: {:?}, Super_R: {:?}", 
                   tab_keycodes, f11_keycodes, f4_keycodes, super_l_keycodes, super_r_keycodes);
    
    let mut grabbed = 0;
    
    // Alt+Tab (Mod1 + Tab)
    for &keycode in &tab_keycodes {
        match conn.grab_key(
            false,
            root,
            ModMask::M1, // Alt
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        ) {
            Ok(_) => {
                tracing::info!("✓ Grabbed Alt+Tab (keycode {})", keycode);
                grabbed += 1;
            }
            Err(e) => {
                tracing::warn!("✗ Failed to grab Alt+Tab (keycode {}): {}", keycode, e);
            }
        }
    }
    
    // F11 (any modifiers)
    for &keycode in &f11_keycodes {
        match conn.grab_key(
            false,
            root,
            ModMask::ANY, // Any modifiers
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        ) {
            Ok(_) => {
                tracing::info!("✓ Grabbed F11 (keycode {})", keycode);
                grabbed += 1;
            }
            Err(e) => {
                tracing::warn!("✗ Failed to grab F11 (keycode {}): {}", keycode, e);
            }
        }
    }
    
    // Alt+F4 (Mod1 + F4)
    for &keycode in &f4_keycodes {
        match conn.grab_key(
            false,
            root,
            ModMask::M1, // Alt
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        ) {
            Ok(_) => {
                tracing::info!("✓ Grabbed Alt+F4 (keycode {})", keycode);
                grabbed += 1;
            }
            Err(e) => {
                tracing::warn!("✗ Failed to grab Alt+F4 (keycode {}): {}", keycode, e);
            }
        }
    }
    
    // Super key (no modifiers)
    for &keycode in &super_l_keycodes {
        match conn.grab_key(
            false,
            root,
            ModMask::from(0u16), // No modifiers
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        ) {
            Ok(_) => {
                tracing::info!("✓ Grabbed Super_L (keycode {})", keycode);
                grabbed += 1;
            }
            Err(e) => {
                tracing::warn!("✗ Failed to grab Super_L (keycode {}): {}", keycode, e);
            }
        }
    }
    
    for &keycode in &super_r_keycodes {
        match conn.grab_key(
            false,
            root,
            ModMask::from(0u16),
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        ) {
            Ok(_) => {
                tracing::info!("✓ Grabbed Super_R (keycode {})", keycode);
                grabbed += 1;
            }
            Err(e) => {
                tracing::warn!("✗ Failed to grab Super_R (keycode {}): {}", keycode, e);
            }
        }
    }
    
    tracing::info!("Keyboard grabs complete: {} successful grabs", grabbed);
    Ok(())
}

fn register_as_wm(conn: &RustConnection, screen_num: usize) -> Result<()> {
    let root = conn.setup().roots[screen_num].root;
    
    // Create a window to own the WM_Sn selection
    let wm_window = conn.generate_id()?;
    conn.create_window(
        0, // depth: COPY_FROM_PARENT
        wm_window,
        root,
        -100,
        -100,
        1,
        1,
        0,
        WindowClass::COPY_FROM_PARENT,
        0, // visual: COPY_FROM_PARENT
        &CreateWindowAux::new()
            .override_redirect(1) // Bool32
            .event_mask(EventMask::PROPERTY_CHANGE),
    )?;
    
    // Set WM name
    let wm_name_atom = conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?;
    let utf8_string_atom = conn.intern_atom(false, b"UTF8_STRING")?.reply()?;
    let name = b"area-focus";
    conn.change_property(
        PropMode::REPLACE,
        wm_window,
        wm_name_atom.atom,
        utf8_string_atom.atom,
        8,
        name.len() as u32,
        name,
    )?;
    
    // Get timestamp from PropertyNotify
    conn.map_window(wm_window)?;
    conn.flush()?;
    
    // Wait for PropertyNotify to get timestamp
    let timestamp = loop {
        let event = conn.wait_for_event()?;
        if let x11rb::protocol::Event::PropertyNotify(e) = event {
            if e.window == wm_window {
                break e.time;
            }
        }
    };
    
    // Acquire WM_Sn selection
    let wm_sn = format!("WM_S{}", screen_num);
    let wm_sn_atom = conn.intern_atom(false, wm_sn.as_bytes())?.reply()?;
    
    conn.set_selection_owner(wm_window, wm_sn_atom.atom, timestamp)?;
    conn.flush()?;
    
    // Verify we got it
    let owner = conn.get_selection_owner(wm_sn_atom.atom)?.reply()?;
    if owner.owner != wm_window {
        tracing::warn!("Failed to acquire WM_S{} selection", screen_num);
        return Err(anyhow::anyhow!("Failed to register as window manager"));
    }
    
    tracing::info!("✓ Registered as window manager (WM_S{})", screen_num);
    
    // Select SubstructureRedirectMask on root (required for WM)
    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(
            EventMask::SUBSTRUCTURE_REDIRECT
                | EventMask::SUBSTRUCTURE_NOTIFY
                | EventMask::BUTTON_PRESS
                | EventMask::KEY_PRESS
                | EventMask::FOCUS_CHANGE
                | EventMask::ENTER_WINDOW
                | EventMask::LEAVE_WINDOW,
        ),
    )?;
    
    tracing::info!("✓ Selected SubstructureRedirectMask on root");
    
    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let (conn, screen_num) = RustConnection::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    
    tracing::info!("Focus manager started on display {}", screen_num);
    
    // Register as window manager (required for proper keyboard routing)
    register_as_wm(&conn, screen_num)?;
    
    // Setup keyboard grabs
    setup_keyboard_grabs(&conn, root)?;
    conn.flush()?;
    
    // Track the currently focused window
    let mut focused_window: Option<u32> = None;
    
    // Event loop
    loop {
        let event = conn.wait_for_event()?;
        
        match event {
            x11rb::protocol::Event::FocusIn(e) => {
                // Track which window received focus
                if e.mode == x11rb::protocol::xproto::NotifyMode::NORMAL {
                    if let Ok(toplevel) = find_toplevel_window(&conn, e.event, root) {
                        if toplevel != root {
                            focused_window = Some(toplevel);
                            let title = get_window_title(&conn, toplevel);
                            tracing::info!("FocusIn: window {} '{}' received focus", toplevel, title);
                        }
                    }
                }
            }
            x11rb::protocol::Event::FocusOut(e) => {
                if e.mode == x11rb::protocol::xproto::NotifyMode::NORMAL {
                    if let Ok(toplevel) = find_toplevel_window(&conn, e.event, root) {
                        if toplevel != root && focused_window == Some(toplevel) {
                            tracing::info!("FocusOut: window {} lost focus", toplevel);
                            focused_window = None;
                        }
                    }
                }
            }
            x11rb::protocol::Event::ButtonPress(e) => {
                tracing::info!("ButtonPress: button={}, window={}", e.detail, e.event);
                
                // Find the window under the pointer
                let pointer = conn.query_pointer(root)?.reply()?;
                tracing::info!("Pointer at window: {} (child of root)", pointer.child);
                
                if pointer.child != x11rb::NONE {
                    // Find the toplevel window
                    match find_toplevel_window(&conn, pointer.child, root) {
                        Ok(toplevel) => {
                            if toplevel != root {
                                let title = get_window_title(&conn, toplevel);
                                tracing::info!("Click detected on toplevel window {}: '{}'", toplevel, title);
                                
                                match set_focus_to_window(&conn, toplevel) {
                                    Ok(_) => {
                                        tracing::info!("✓ Successfully focused window {}: '{}'", toplevel, title);
                                    }
                                    Err(e) => {
                                        tracing::warn!("✗ Failed to set focus to window {}: {}", toplevel, e);
                                    }
                                }
                            } else {
                                tracing::debug!("Click on root window, ignoring");
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to find toplevel window for {}: {}", pointer.child, e);
                        }
                    }
                } else {
                    tracing::debug!("Click on root window (no child)");
                }
            }
            x11rb::protocol::Event::KeyPress(e) => {
                let keycode = e.detail;
                let state = u16::from(e.state);
                let mod1 = ModMask::M1.bits();
                let mod4 = ModMask::M4.bits();
                
                // Get keysym for logging
                let keysym = keycode_to_keysym(&conn, keycode);
                let keysym_str = keysym.map(|k| format!("0x{:X}", k)).unwrap_or_else(|| "Unknown".to_string());
                
                tracing::info!("KeyPress: keycode={}, keysym={}, state=0x{:x} (Mod1={}, Mod4={})", 
                              keycode, keysym_str, state, (state & mod1) != 0, (state & mod4) != 0);
                
                // Check for Alt+Tab
                let tab_keycodes = keysym_to_keycode(&conn, XK_TAB).unwrap_or_default();
                if tab_keycodes.contains(&keycode) && (state & mod1) != 0 {
                    tracing::info!("→ Handling Alt+Tab (keycode {}, keysym 0x{:X})", keycode, XK_TAB);
                    if let Err(e) = switch_to_next_window(&conn, root) {
                        tracing::warn!("Failed to switch window: {}", e);
                    } else {
                        tracing::info!("✓ Window switched");
                    }
                    continue;
                }
                
                // Check for F11
                let f11_keycodes = keysym_to_keycode(&conn, XK_F11).unwrap_or_default();
                if f11_keycodes.contains(&keycode) {
                    tracing::info!("→ Handling F11 (keycode {}, keysym 0x{:X})", keycode, XK_F11);
                    if let Err(e) = toggle_fullscreen(&conn, root) {
                        tracing::warn!("Failed to toggle fullscreen: {}", e);
                    } else {
                        tracing::info!("✓ Fullscreen toggled");
                    }
                    continue;
                }
                
                // Check for Alt+F4
                let f4_keycodes = keysym_to_keycode(&conn, XK_F4).unwrap_or_default();
                if f4_keycodes.contains(&keycode) && (state & mod1) != 0 {
                    tracing::info!("→ Handling Alt+F4 (keycode {}, keysym 0x{:X})", keycode, XK_F4);
                    match close_window(&conn, root) {
                        Ok(_) => {
                            tracing::info!("✓ Close window message sent");
                        }
                        Err(e) => {
                            tracing::warn!("✗ Failed to close window: {}", e);
                        }
                    }
                    continue;
                }
                
                // Check for Super key
                let super_l_keycodes = keysym_to_keycode(&conn, XK_SUPER_L).unwrap_or_default();
                let super_r_keycodes = keysym_to_keycode(&conn, XK_SUPER_R).unwrap_or_default();
                if super_l_keycodes.contains(&keycode) || super_r_keycodes.contains(&keycode) {
                    // Super key pressed - it might have Mod4 set, but if it's just Super (no other modifiers), handle it
                    // Check if only Mod4 is set (or no modifiers)
                    let other_mods = state & !mod4;
                    if other_mods == 0 {
                        tracing::info!("→ Handling Super key (keycode {}, keysym 0x{:X} or 0x{:X}, state=0x{:x})", 
                                      keycode, XK_SUPER_L, XK_SUPER_R, state);
                        if let Err(e) = toggle_navigator(&conn, root) {
                            tracing::warn!("Failed to toggle Navigator: {}", e);
                        } else {
                            tracing::info!("✓ Navigator toggled");
                        }
                        continue;
                    } else {
                        tracing::debug!("Super key pressed but with other modifiers: state=0x{:x}", state);
                    }
                }
                
                tracing::debug!("KeyPress not handled: keycode={}, keysym={}, state=0x{:x}", keycode, keysym_str, state);
            }
            x11rb::protocol::Event::MapNotify(e) => {
                // Center windows when they're mapped
                if e.event != root && !e.override_redirect {
                    let is_terminal = is_terminal_window(&conn, e.event);
                    
                    // Get window geometry
                    if let Ok(geom_reply) = conn.get_geometry(e.event) {
                        if let Ok(geom) = geom_reply.reply() {
                            let screen_width = screen.width_in_pixels as i32;
                            let screen_height = screen.height_in_pixels as i32;
                            
                            // Calculate centered position
                            let x = (screen_width - geom.width as i32) / 2;
                            let y = (screen_height - geom.height as i32) / 2;
                            
                            // Center the window
                            if let Err(err) = conn.configure_window(
                                e.event,
                                &ConfigureWindowAux::new().x(x).y(y),
                            ) {
                                tracing::warn!("Failed to center window {}: {}", e.event, err);
                            } else {
                                let title = get_window_title(&conn, e.event);
                                tracing::info!("✓ Centered window {} '{}' at ({}, {})", e.event, title, x, y);
                            }
                            
                            // Focus app windows, but not terminal windows
                            // Terminal windows stay in background, app windows come to foreground
                            if !is_terminal {
                                let window_id = e.event;
                                if let Err(err) = set_focus_to_window(&conn, window_id) {
                                    tracing::warn!("Failed to focus window {}: {}", window_id, err);
                                } else {
                                    tracing::info!("✓ Focused app window {} '{}'", window_id, get_window_title(&conn, window_id));
                                }
                            } else {
                                tracing::debug!("Terminal window {} mapped, leaving in background", e.event);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        
        conn.flush()?;
    }
}


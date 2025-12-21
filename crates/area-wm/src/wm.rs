//! Core window manager logic

use crate::ewmh::Atoms;
use crate::ipc::{IpcHandle, IpcServer};
use crate::window::{Window, WindowManager};
use crate::decorations::{WindowFrame, ButtonType};

use anyhow::{Context, Result};
use area_ipc::{ShellCommand, WmEvent};
use std::process::Command;
use tracing::{debug, info};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::COPY_FROM_PARENT;

/// Modifier key for window operations (Alt = Mod1)
const MOD_MASK: u16 = 0x08; // Mod1Mask

/// Super key mask
const SUPER_MASK: u16 = 0x40; // Mod4Mask

/// Run the window manager
pub async fn run() -> Result<()> {
    // Connect to X server
    let (conn, screen_num) = RustConnection::connect(None)
        .context("Failed to connect to X server")?;

    info!("Connected to X server, screen {}", screen_num);

    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let screen_width = screen.width_in_pixels;
    let screen_height = screen.height_in_pixels;

    info!("Screen size: {}x{}", screen_width, screen_height);

    // Become the window manager by selecting SubstructureRedirect on root
    let mask = EventMask::SUBSTRUCTURE_REDIRECT
        | EventMask::SUBSTRUCTURE_NOTIFY
        | EventMask::STRUCTURE_NOTIFY
        | EventMask::PROPERTY_CHANGE
        | EventMask::BUTTON_PRESS
        | EventMask::BUTTON_RELEASE
        | EventMask::POINTER_MOTION;

    conn.change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(mask))?
        .check()
        .context("Another window manager is already running")?;

    info!("Registered as window manager");

    // Intern EWMH atoms
    let atoms = Atoms::new(&conn)?;
    atoms.setup_supported(&conn, root)?;

    // Create a window for EWMH check (required by some apps like wezterm)
    let wm_check_win = conn.generate_id()?;
    conn.create_window(
        COPY_FROM_PARENT as u8,
        wm_check_win,
        root,
        -1, -1, 1, 1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &Default::default(),
    )?;
    atoms.setup_supporting_wm_check(&conn, root, wm_check_win, "area-wm")?;

    // Initialize window manager state
    let mut wm = WindowManager::new();

    // Initialize Composite extension (redirects all windows to offscreen)
    // This enables efficient window capture in the compositor
    let _composite_manager = crate::composite::CompositeManager::new(&conn, root)?;

    // Start IPC server
    let ipc_server = IpcServer::new();
    let mut ipc = ipc_server.start().await?;

    // Scan for existing windows
    let existing = conn.query_tree(root)?.reply()?;
    for &child in &existing.children {
        if let Ok(attrs) = conn.get_window_attributes(child)?.reply() {
            if attrs.map_state != MapState::UNMAPPED && !attrs.override_redirect {
                manage_window(&conn, &atoms, &mut wm, &mut ipc, child)?;
            }
        }
    }

    // Grab keys for window management
    setup_keybinds(&conn, root)?;

    conn.flush()?;

    // Drag state
    let mut drag_state: Option<DragState> = None;

    // Event loop
    info!("Entering event loop");
    loop {
        // Process IPC commands
        while let Some(cmd) = ipc.try_recv_command() {
            handle_shell_command(&conn, &atoms, &mut wm, &mut ipc, cmd)?;
        }

        // Process X events
        if let Some(event) = conn.poll_for_event()? {
            match event {
                Event::MapRequest(e) => {
                    debug!("MapRequest for window {}", e.window);
                    manage_window(&conn, &atoms, &mut wm, &mut ipc, e.window)?;
                }

                Event::ConfigureRequest(e) => {
                    debug!("ConfigureRequest for window {}", e.window);
                    // Honor the request
                    let aux = ConfigureWindowAux::from_configure_request(&e);
                    conn.configure_window(e.window, &aux)?;

                    // Update our state if we're tracking this window
                    if let Some(win) = wm.get_window_mut(e.window) {
                        if u16::from(e.value_mask) & u16::from(ConfigWindow::X) != 0 {
                            win.x = e.x as i32;
                        }
                        if u16::from(e.value_mask) & u16::from(ConfigWindow::Y) != 0 {
                            win.y = e.y as i32;
                        }
                        if u16::from(e.value_mask) & u16::from(ConfigWindow::WIDTH) != 0 {
                            win.width = e.width as u32;
                        }
                        if u16::from(e.value_mask) & u16::from(ConfigWindow::HEIGHT) != 0 {
                            win.height = e.height as u32;
                        }
                    }
                }

                Event::UnmapNotify(e) => {
                    debug!("UnmapNotify for window {}", e.window);
                    if let Some(win) = wm.get_window_mut(e.window) {
                        win.mapped = false;
                    }
                }

                Event::DestroyNotify(e) => {
                    debug!("DestroyNotify for window {}", e.window);
                    if wm.remove_window(e.window).is_some() {
                        ipc.broadcast(WmEvent::WindowClosed { id: e.window });
                        update_client_list(&conn, &atoms, &wm, root)?;
                    }
                }

                Event::PropertyNotify(e) => {
                    if let Some(win) = wm.get_window_mut(e.window) {
                        if e.atom == atoms.net_wm_name || e.atom == atoms.wm_name {
                            let title = atoms.get_window_title(&conn, e.window)?;
                            if win.title != title {
                                win.title = title.clone();
                                ipc.broadcast(WmEvent::WindowTitleChanged {
                                    id: e.window,
                                    title,
                                });
                            }
                        }
                    }
                }

                Event::ButtonPress(e) => {
                    let state_bits = u16::from(e.state);
                    debug!("ButtonPress: window={}, button={}, state={:x}", e.event, e.detail, state_bits);

                    let mut handled = false;

                    // Check if Alt is held for window operations (Alt+Drag anywhere)
                    if state_bits & MOD_MASK != 0 {
                        if e.detail == 1 {
                            // Alt+Left click = start move
                            drag_state = Some(DragState {
                                window: e.child,
                                start_x: e.root_x,
                                start_y: e.root_y,
                                mode: DragMode::Move,
                            });
                            // Notify compositor that drag started
                            ipc.broadcast(WmEvent::WindowDragStarted { id: e.child });
                            handled = true;
                        } else if e.detail == 3 {
                            // Alt+Right click = start resize
                            drag_state = Some(DragState {
                                window: e.child,
                                start_x: e.root_x,
                                start_y: e.root_y,
                                mode: DragMode::Resize,
                            });
                            // Notify compositor that drag started (resize also triggers wobble)
                            ipc.broadcast(WmEvent::WindowDragStarted { id: e.child });
                            handled = true;
                        }
                    } 
                    
                    if !handled {
                        // Check if clicked ON A FRAME (Titlebar/Buttons)
                        // Iterate windows to find if e.event (screen window) belongs to a frame
                        let mut action = None;
                        
                        for win in wm.all_windows() {
                            if let Some(frame) = &win.frame {
                                if frame.contains(e.event) {
                                    // Found the window!
                                    // Check if it's a button
                                    if let Some(btn) = frame.get_button_type(e.event) {
                                        action = Some((win.id, "button", Some(btn)));
                                    } else if e.event == frame.titlebar {
                                        action = Some((win.id, "titlebar", None));
                                    }
                                    break;
                                }
                            }
                        }

                        if let Some((win_id, kind, btn_type)) = action {
                             if kind == "titlebar" && e.detail == 1 {
                                 // Dragging titlebar
                                 drag_state = Some(DragState {
                                    window: win_id,
                                    start_x: e.root_x,
                                    start_y: e.root_y,
                                    mode: DragMode::Move,
                                });
                                // Notify compositor that drag started
                                ipc.broadcast(WmEvent::WindowDragStarted { id: win_id });
                                handled = true;
                             } else if kind == "button" && e.detail == 1 {
                                 // Handle button click
                                 if let Some(btn) = btn_type {
                                     match btn {
                                         ButtonType::Close => {
                                             // Helper to close window
                                             // Send delete message
                                             atoms.send_delete_window(&conn, win_id)?;
                                         }
                                         ButtonType::Maximize => {
                                            toggle_maximize(&conn, &atoms, &mut wm, &mut ipc, win_id, screen.root)?;
                                         }
                                         ButtonType::Minimize => {
                                             // Minimize (Unmap)
                                             conn.unmap_window(if let Some(w) = wm.get_window(win_id) {
                                                 // Unmap frame if framed
                                                 w.frame.as_ref().map(|f| f.frame).unwrap_or(win_id)
                                             } else { win_id })?;
                                             // Update state
                                             if let Some(w) = wm.get_window_mut(win_id) {
                                                 w.mapped = false;
                                             }
                                             ipc.broadcast(WmEvent::WindowClosed { id: win_id }); // Or minimized event?
                                         }
                                     }
                                 }
                                 handled = true;
                             }
                        }
                    }

                    if handled {
                         conn.grab_pointer(
                            false,
                            root,
                            EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                            GrabMode::ASYNC,
                            GrabMode::ASYNC,
                            x11rb::NONE,
                            x11rb::NONE,
                            x11rb::CURRENT_TIME,
                        )?;
                    } else if e.child != 0 && e.child != root {
                        // Click on client window = focus it
                        focus_window(&conn, &atoms, &mut wm, &mut ipc, e.child, root)?;
                        // Replay event so app sees it? 
                        // If we grabbed ButtonPress on root, we intercept it.
                        // But we grabbed on Window... so checking e.child != 0 implies click on managed window
                        conn.allow_events(Allow::REPLAY_POINTER, x11rb::CURRENT_TIME)?;
                    }
                }

                Event::ButtonRelease(_) => {
                    if let Some(drag) = drag_state.take() {
                        // Notify compositor that drag ended
                        ipc.broadcast(WmEvent::WindowDragEnded { id: drag.window });
                        conn.ungrab_pointer(x11rb::CURRENT_TIME)?;
                    }
                }

                Event::MotionNotify(e) => {
                    if let Some(ref drag) = drag_state {
                        if drag.window != 0 {
                            let dx = e.root_x - drag.start_x;
                            let dy = e.root_y - drag.start_y;

                            // Get window info before mutable borrow
                            let (old_x, old_y, old_width, old_height) = if let Some(win) = wm.get_window(drag.window) {
                                (win.x, win.y, win.width, win.height)
                            } else {
                                continue;
                            };
                            
                                match drag.mode {
                                    DragMode::Move => {
                                    let new_x = old_x + dx as i32;
                                    let new_y = old_y + dy as i32;
                                        
                                    // Get frame reference before mutable borrow
                                    let has_frame = wm.get_window(drag.window).and_then(|w| w.frame.as_ref()).is_some();
                                    
                                    if has_frame {
                                        if let Some(win) = wm.get_window(drag.window) {
                                        if let Some(frame) = &win.frame {
                                                frame.move_to(&conn, new_x as i16, new_y as i16)?;
                                            }
                                        }
                                        } else {
                                            conn.configure_window(
                                                drag.window,
                                                &ConfigureWindowAux::new().x(new_x).y(new_y),
                                            )?;
                                        }

                                        if let Some(win) = wm.get_window_mut(drag.window) {
                                            win.x = new_x;
                                            win.y = new_y;
                                        }
                                        
                                    // Send geometry update for smooth compositor updates during drag
                                    ipc.broadcast(WmEvent::WindowGeometryChanged {
                                        id: drag.window,
                                        x: new_x,
                                        y: new_y,
                                        width: old_width,
                                        height: old_height,
                                    });
                                    }
                                    DragMode::Resize => {
                                    let new_w = (old_width as i32 + dx as i32).max(100) as u32;
                                    let new_h = (old_height as i32 + dy as i32).max(100) as u32;
                                        
                                    // Get frame reference before mutable borrow
                                    let has_frame = wm.get_window(drag.window).and_then(|w| w.frame.as_ref()).is_some();
                                    
                                    if has_frame {
                                        if let Some(win) = wm.get_window(drag.window) {
                                        if let Some(frame) = &win.frame {
                                                frame.resize(&conn, new_w as u16, new_h as u16)?;
                                            }
                                        }
                                        } else {
                                            conn.configure_window(
                                                drag.window,
                                                &ConfigureWindowAux::new().width(new_w).height(new_h),
                                            )?;
                                        }

                                        if let Some(win) = wm.get_window_mut(drag.window) {
                                            win.width = new_w;
                                            win.height = new_h;
                                        }
                                    
                                    // Send geometry update for smooth compositor updates during resize
                                    ipc.broadcast(WmEvent::WindowGeometryChanged {
                                        id: drag.window,
                                        x: old_x,
                                        y: old_y,
                                        width: new_w,
                                        height: new_h,
                                    });
                                }
                            }

                            // Update drag start for next motion
                            drag_state = Some(DragState {
                                window: drag.window,
                                start_x: e.root_x,
                                start_y: e.root_y,
                                mode: drag.mode,
                            });
                        }
                    }
                }

                Event::KeyPress(e) => {
                    let state_bits = u16::from(e.state);
                    debug!("KeyPress: keycode={}, state={:x}", e.detail, state_bits);
                    handle_keypress(&conn, &atoms, &mut wm, &mut ipc, e, root)?;
                }

                _ => {}
            }

            conn.flush()?;
        } else {
            // No X events, yield to async runtime
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DragMode {
    Move,
    Resize,
}

#[derive(Debug)]
struct DragState {
    window: u32,
    start_x: i16,
    start_y: i16,
    mode: DragMode,
}

fn manage_window(
    conn: &RustConnection,
    atoms: &Atoms,
    wm: &mut WindowManager,
    ipc: &mut IpcHandle,
    window: u32,
) -> Result<()> {
    // Get window geometry
    let geom = conn.get_geometry(window)?.reply()?;
    let screen = &conn.setup().roots[0];

    // Get window attributes
    let title = atoms.get_window_title(conn, window)?;
    let class = atoms.get_window_class(conn, window)?;

    debug!("Managing window {}: '{}' ({})", window, title, class);

    // Center the window on screen (unless it's the compositor)
    let (x, y, width, height) = if class == "area-comp" {
        (geom.x as i32, geom.y as i32, geom.width as u32, geom.height as u32)
    } else {
        // Clamp window size to screen size to prevent overflow
        let width = (geom.width as u32).min(screen.width_in_pixels as u32);
        let height = (geom.height as u32).min(screen.height_in_pixels as u32);
        let x = (screen.width_in_pixels.saturating_sub(width as u16) / 2) as i32;
        let y = (screen.height_in_pixels.saturating_sub(height as u16) / 2) as i32;
        (x, y, width, height)
    };

    // Create frame if not compositor
    let (_final_x, _final_y, _final_w, _final_h, frame) = if class != "area-comp" {
        let frame = WindowFrame::new(conn, screen, window, x as i16, y as i16, width as u16, height as u16)?;
        
        // When framed, the window ID we track is still the client, 
        // but its x/y should be the frame's x/y
        (x, y, width, height, Some(frame))
    } else {
        // Just map compositor directly
        conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(x)
                .y(y)
                .width(width)
                .height(height)
                .border_width(0),
        )?;
        
        conn.map_window(window)?;
        (x, y, width, height, None)
    };
    
    // Grab Alt+Button1 for moving and Alt+Button3 for resizing
    conn.grab_button(
        false,
        window,
        EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
        x11rb::NONE,
        x11rb::NONE,
        ButtonIndex::M1,
        ModMask::M1,
    )?;
    
    conn.grab_button(
        false,
        window,
        EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
        x11rb::NONE,
        x11rb::NONE,
        ButtonIndex::M3,
        ModMask::M1,
    )?;
    
    conn.map_window(window)?;

    // Create window entry
    let mut win = Window::new(window);
    win.x = x;
    win.y = y;
    win.width = width;
    win.height = height;
    win.title = title.clone();
    win.class = class.clone();
    win.sticky = class == "area-comp";
    win.mapped = true;
    win.workspace = wm.current_workspace();
    win.frame = frame.clone(); // Store frame info
    
    // Send Frame ID if present to shell so it can capture the decoration!
    let notify_id = frame.as_ref().map(|f| f.frame).unwrap_or(window);

    wm.add_window(win);

    // Notify compositor
    ipc.broadcast(WmEvent::WindowOpened {
        id: notify_id, // Send FRAME id so shell captures the frame!
        title,
        class,
        x,
        y,
        width,
        height,
    });

    // Update EWMH
    update_client_list(conn, atoms, wm, screen.root)?;

    Ok(())
}

fn toggle_maximize(
    conn: &RustConnection,
    atoms: &Atoms,
    wm: &mut WindowManager,
    _ipc: &mut IpcHandle,
    window: u32,
    _root: u32,
) -> Result<()> {
    let screen = &conn.setup().roots[0];
    let win = wm.get_window_mut(window).context("Window not found")?;
    
    if win.maximized {
        // Unmaximize: restore previous geometry
        let target_x = win.restore_x;
        let target_y = win.restore_y;
        let target_w = win.restore_width;
        let target_h = win.restore_height;
        
        if let Some(frame) = &win.frame {
            frame.move_to(conn, target_x as i16, target_y as i16)?;
            frame.resize(conn, target_w as u16, target_h as u16)?;
        } else {
            conn.configure_window(
                window,
                &ConfigureWindowAux::new()
                    .x(target_x)
                    .y(target_y)
                    .width(target_w)
                    .height(target_h),
            )?;
        }
        
        win.x = target_x;
        win.y = target_y;
        win.width = target_w;
        win.height = target_h;
        win.maximized = false;
        
        // Remove EWMH maximize state
        atoms.set_window_state(conn, window, &[], &[atoms._net_wm_state_maximized_vert, atoms._net_wm_state_maximized_horz])?;
    } else {
        // Maximize: save current geometry and maximize
        win.restore_x = win.x;
        win.restore_y = win.y;
        win.restore_width = win.width;
        win.restore_height = win.height;
        
        let screen_w = screen.width_in_pixels as u32;
        let screen_h = screen.height_in_pixels as u32;
        
        if let Some(frame) = &win.frame {
            frame.move_to(conn, 0, 0)?;
            frame.resize(conn, screen_w as u16, screen_h as u16)?;
        } else {
            conn.configure_window(
                window,
                &ConfigureWindowAux::new()
                    .x(0)
                    .y(0)
                    .width(screen_w)
                    .height(screen_h),
            )?;
        }
        
        win.x = 0;
        win.y = 0;
        win.width = screen_w;
        win.height = screen_h;
        win.maximized = true;
        
        // Set EWMH maximize state
        atoms.set_window_state(conn, window, &[atoms._net_wm_state_maximized_vert, atoms._net_wm_state_maximized_horz], &[])?;
    }
    
    conn.flush()?;
    Ok(())
}

fn focus_window(
    conn: &RustConnection,
    atoms: &Atoms,
    wm: &mut WindowManager,
    ipc: &mut IpcHandle,
    window: u32,
    root: u32,
) -> Result<()> {
    // Set input focus
    conn.set_input_focus(InputFocus::POINTER_ROOT, window, x11rb::CURRENT_TIME)?;

    // Raise to top
    conn.configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;

    // Update state
    wm.set_focus(Some(window));

    // Update EWMH
    atoms.update_active_window(conn, root, Some(window))?;

    // Notify compositor
    ipc.broadcast(WmEvent::WindowFocused { id: window });

    Ok(())
}

fn update_client_list(
    conn: &RustConnection,
    atoms: &Atoms,
    wm: &WindowManager,
    root: u32,
) -> Result<()> {
    let windows: Vec<u32> = wm.all_windows().map(|w| w.id).collect();
    atoms.update_client_list(conn, root, &windows)?;
    Ok(())
}

fn setup_keybinds(conn: &RustConnection, root: u32) -> Result<()> {
    // Grab Alt+Button1 (move) and Alt+Button3 (resize)
    conn.grab_button(
        false,
        root,
        EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
        x11rb::NONE,
        x11rb::NONE,
        ButtonIndex::M1,
        ModMask::M1,
    )?;

    conn.grab_button(
        false,
        root,
        EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
        x11rb::NONE,
        x11rb::NONE,
        ButtonIndex::M3,
        ModMask::M1,
    )?;

    // Grab Super+keys for workspaces (keycodes 10-13 are 1-4, 36 is Return)
    for keycode in [10u8, 11, 12, 13, 36] {
        conn.grab_key(
            false,
            root,
            ModMask::M4, // Super
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )?;
    }

    // Grab Super key alone (keycode 133) for Navigator
    conn.grab_key(
        false,
        root,
        ModMask::from(0u16), // No modifiers
        133, // Super_L
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?;

    info!("Keybinds set up: Alt+Drag to move, Alt+RightDrag to resize, Super for Navigator, Super+1-4 for workspaces");

    Ok(())
}

fn handle_keypress(
    conn: &RustConnection,
    atoms: &Atoms,
    wm: &mut WindowManager,
    ipc: &mut IpcHandle,
    event: KeyPressEvent,
    root: u32,
) -> Result<()> {
    let keycode = event.detail;
    let state_bits = u16::from(event.state);
    
    debug!("KeyPress: keycode={}, state={:x}", keycode, state_bits);

    // Super alone (keycode 133) = toggle Navigator
    if keycode == 133 {
        info!("Super key pressed, toggling Navigator");
        toggle_navigator(wm)?;
        return Ok(());
    }

    // Super + number = switch workspace
    if state_bits & SUPER_MASK != 0 {
        match keycode {
            10 => switch_workspace(conn, atoms, wm, ipc, 0, root)?, // 1
            11 => switch_workspace(conn, atoms, wm, ipc, 1, root)?, // 2
            12 => switch_workspace(conn, atoms, wm, ipc, 2, root)?, // 3
            13 => switch_workspace(conn, atoms, wm, ipc, 3, root)?, // 4
            36 => {
                // Super + Return = launch terminal
                spawn_app("xfce4-terminal")?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn toggle_navigator(wm: &WindowManager) -> Result<()> {
    // Find Navigator window by class
    for win in wm.all_windows() {
        if win.title == "Navigator" {
            // Toggle visibility by sending a message to restore/minimize
            // For now, just spawn a new instance if not found
            return Ok(());
        }
    }
    
    // Navigator not found or minimized, spawn it
    spawn_app("navigator")?;
    Ok(())
}

fn switch_workspace(
    conn: &RustConnection,
    atoms: &Atoms,
    wm: &mut WindowManager,
    ipc: &mut IpcHandle,
    workspace: u8,
    root: u32,
) -> Result<()> {
    let old_ws = wm.current_workspace();
    if workspace == old_ws {
        return Ok(());
    }

    info!("Switching from workspace {} to {}", old_ws, workspace);

    // Hide windows on old workspace
    for win in wm.all_windows() {
        if win.workspace == old_ws && win.mapped && !win.sticky {
            conn.unmap_window(win.id)?;
        }
    }

    wm.switch_workspace(workspace);

    // Show windows on new workspace
    for win in wm.all_windows() {
        if win.workspace == workspace && win.mapped {
            conn.map_window(win.id)?;
        }
    }

    // Update EWMH
    atoms.update_current_desktop(conn, root, workspace as u32)?;

    // Notify compositor
    ipc.broadcast(WmEvent::WorkspaceChanged {
        current: workspace,
        total: wm.num_workspaces(),
    });

    Ok(())
}

fn handle_shell_command(
    conn: &RustConnection,
    atoms: &Atoms,
    wm: &mut WindowManager,
    ipc: &mut IpcHandle,
    cmd: ShellCommand,
) -> Result<()> {
    let screen = &conn.setup().roots[0];
    let root = screen.root;

    match cmd {
        ShellCommand::FocusWindow { id } => {
            focus_window(conn, atoms, wm, ipc, id, root)?;
        }
        ShellCommand::CloseWindow { id } => {
            // Send WM_DELETE_WINDOW if supported, otherwise destroy
            conn.destroy_window(id)?;
        }
        ShellCommand::SwitchWorkspace { index } => {
            switch_workspace(conn, atoms, wm, ipc, index, root)?;
        }
        ShellCommand::LaunchApp { command } => {
            spawn_app(&command)?;
        }
        ShellCommand::MoveWindow { id, x, y } => {
            conn.configure_window(id, &ConfigureWindowAux::new().x(x).y(y))?;
            if let Some(win) = wm.get_window_mut(id) {
                win.x = x;
                win.y = y;
            }
        }
        ShellCommand::ResizeWindow { id, width, height } => {
            conn.configure_window(id, &ConfigureWindowAux::new().width(width).height(height))?;
            if let Some(win) = wm.get_window_mut(id) {
                win.width = width;
                win.height = height;
            }
        }
        _ => {
            debug!("Unhandled shell command: {:?}", cmd);
        }
    }

    conn.flush()?;
    Ok(())
}

fn spawn_app(command: &str) -> Result<()> {
    info!("Launching: {}", command);
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .env("WAYLAND_DISPLAY", "") // Force X11 for Xephyr
        .spawn()
        .context("Failed to spawn application")?;
    Ok(())
}

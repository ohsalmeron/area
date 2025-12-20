//! Area Shell
//!
//! A Bevy-powered desktop shell for the Area window manager.
//! Provides panels, overview mode, launcher, and agentic overlays
//! with Compiz Fusion-style animations.

mod agent;
mod animation;
mod bar;
mod grab;
mod ipc;
mod launcher;
mod overview;
mod state;
mod wallpaper;
mod compositor;
mod wobbly;

use anyhow::Result;
use bevy::prelude::*;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<()> {
    // Initialize logging - silence noisy crates
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "area_shell=debug,info,wgpu_core=warn,wgpu_hal=warn,naga=warn".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Area Shell");

    App::new()
        // Bevy defaults with custom window settings
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Area Shell".into(),
                // Start as a dock-type window
                decorations: false,
                transparent: true,
                // Postion at top of screen
                position: WindowPosition::At(IVec2::new(0, 0)),
                resolution: (1280u32, 32u32).into(), // Start as bar-only
                present_mode: bevy::window::PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }).disable::<bevy::log::LogPlugin>())
        // Our plugins
        .insert_resource(ClearColor(Color::srgba(0.05, 0.05, 0.1, 1.0)))
        .add_plugins((
            state::StatePlugin,
            animation::AnimationPlugin,
            grab::GrabPlugin,
            ipc::IpcPlugin,
            bar::BarPlugin,
            wallpaper::WallpaperPlugin,
            launcher::LauncherPlugin,
            overview::OverviewPlugin,
            agent::AgentPlugin,
            wobbly::WobblyPlugin,
            compositor::CompositorPlugin, // Enabled for testing
        ))
        .add_systems(Update, sync_window_size)
        .run();

    Ok(())
}

/// Sync Bevy window size based on current shell mode
fn sync_window_size(
    mode: Res<state::ShellMode>,
    mut windows: Query<&mut Window>,
    mut resize_reader: MessageReader<bevy::window::WindowResized>,
) {
    // Process any window resize messages first (debouncing)
    let _ = resize_reader.read().count();
    
    if !mode.is_changed() {
        return;
    }

    if let Ok(mut window) = windows.single_mut() {
        let target_resolution = match *mode {
            state::ShellMode::Normal => {
                // Only the bar: height 32
                (1280.0, 32.0)
            }
            state::ShellMode::Overview | state::ShellMode::Launcher => {
                // Fullscreen for compositor and overlays: height 720
                (1280.0, 720.0)
            }
        };
        
        // Only resize if different to avoid unnecessary swap chain reconfigurations
        let current = window.resolution.size();
        if current.x != target_resolution.0 || current.y != target_resolution.1 {
            window.resolution.set(target_resolution.0, target_resolution.1);
        }
    }
}

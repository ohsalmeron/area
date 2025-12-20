//! Area Terminal - Custom terminal emulator library
//!
//! Provides terminal emulation using alacritty_terminal with
//! grid extraction for custom rendering in Bevy.

pub mod pty;
pub mod term;

pub use term::{GridCell, Terminal};
pub use pty::Pty;

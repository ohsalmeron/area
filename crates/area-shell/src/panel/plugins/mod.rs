//! Panel plugins
//!
//! Built-in plugins for the panel system.

pub mod clock;
pub mod workspace;
pub mod taskbar;
pub mod separator;

pub use clock::ClockPlugin;
pub use workspace::WorkspacePlugin;
pub use taskbar::TaskbarPlugin;
pub use separator::SeparatorPlugin;









//! Windows input backend: capture lives in `hlidskjalf-motion`; injection lets Windows be controlled.

pub mod edge;
pub mod keymap;

#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod injector;

#[cfg(windows)]
pub use clipboard::{WindowsClipboard, MAX_TEXT_BYTES};
#[cfg(windows)]
pub use injector::WindowsInjector;

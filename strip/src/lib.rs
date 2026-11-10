//! The strip: one pixel-art creature per Claude Code session, on the edge of the screen.
//!
//! Everything here is plain data in, pixels out, so it runs and is tested on any system. The
//! Windows window, the socket to the agent and the menus live in the binary (`main.rs`).
//! Ported from the Swift overlay (`Sources/PerchCore`, `Sources/PerchOverlay`); the agent's
//! protocol is `shared/protocol.ts`.

pub mod appearance;
pub mod config;
pub mod hotkey;
pub mod lang;
pub mod layout;
pub mod protocol;
pub mod raster;
pub mod screens;
pub mod sessions;
pub mod sprite;
pub mod view;

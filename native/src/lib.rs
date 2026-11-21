//! CenAI's Rust-native application runtime.

pub mod core;
pub mod ecosystem;
pub mod render;
pub mod ui;

#[cfg(target_os = "android")]
mod android;

/// Stable runtime identity used by diagnostics and capability registration.
pub const RUNTIME_ID: &str = "cenai.native";
/// Protocol version shared by the app and its private first-party ecosystem.
pub const ECOSYSTEM_PROTOCOL_VERSION: u16 = 1;

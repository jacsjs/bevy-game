//! Library entry point.
//!
//! Integration tests in `tests/` are compiled as separate crates.
//! A `lib.rs` gives them a stable public API surface to import.

pub mod game;
pub mod common;
pub mod plugins;

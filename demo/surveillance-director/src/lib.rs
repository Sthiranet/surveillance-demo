//! surveillance-director: everything that exists only for the presentation. It runs every party
//! in one process, in memory, and exposes the actions the UI calls. Reset rebuilds the network.
//!
//! This is the only crate that names concrete types from every layer: it wires a profile's
//! `Camera` to a generic `ChainNode` and a `dyn AnchorLedger`.

pub mod api;
pub mod views;
pub mod world;

pub use world::{DemoError, Shared, World, set_auto};

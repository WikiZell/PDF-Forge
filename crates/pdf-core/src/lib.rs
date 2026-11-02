//! Pure-Rust PDF core: no UI and no OS-specific dependencies.
//!
//! Milestone 1 contains only the safety primitives and a light structural
//! inspector. Page-tree operations, merge/split, annotations and the bulk-job
//! engine arrive in later milestones.

pub mod fs_safety;
pub mod inspect;

pub use fs_safety::{sanitize_file_name, AllowedPaths, Limits, PathError};
pub use inspect::{inspect, DocInfo, InspectError};

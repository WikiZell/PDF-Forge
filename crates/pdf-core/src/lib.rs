//! Pure-Rust PDF core: no UI and no OS-specific dependencies.
//!
//! Milestone 1 contains only the safety primitives and a light structural
//! inspector. Page-tree operations, merge/split, annotations and the bulk-job
//! engine arrive in later milestones.

pub mod build;
pub mod bulk;
pub mod fs_safety;
pub mod inspect;
pub mod load;
pub mod model;
pub mod sigimage;
pub mod sign;
pub mod split;

pub use fs_safety::{
    atomic_write, sanitize_file_name, unique_output_path, write_new_atomic, AllowedPaths, Limits, PathError,
};
pub use inspect::{inspect, DocInfo, InspectError};

pub use build::{build, BuildError, BuildReport, FieldRename};
pub use load::{load_bytes, load_file, LoadError};
pub use lopdf;
pub use model::{parse_ranges, BlankSize, EditError, PageEntry, PageKind, Project};
pub use split::{split_by_bookmarks, split_every_n, split_ranges, top_level_bookmarks, SplitPart};

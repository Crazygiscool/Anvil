//! The application catalog: a data-driven list of installable apps.
//!
//! The catalog is the single source of truth for what Anvil can install. It
//! is loaded from a TOML index file merged on top of the built-in defaults,
//! and installs are executed through the regular mutation stack.

// The catalog is a public API of the crate; not every entry point is wired
// into a command or a tab yet.
#![allow(dead_code)]

pub mod index;
pub mod install;
pub mod model;

#[allow(unused_imports)]
pub use index::{
    builtin_index, default_index_path, load_index, merge_indexes, parse_index, read_index,
    standard_index_paths, validate_entry, validate_index,
};
#[allow(unused_imports)]
pub use install::{backend_for, container_spec, install_plan, install_target, uninstall_plan};
#[allow(unused_imports)]
pub use model::{CatalogEntry, CatalogIndex, CatalogPort, CatalogVolume, SUPPORTED_INDEX_VERSION};

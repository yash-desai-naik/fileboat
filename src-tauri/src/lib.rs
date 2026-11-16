//! Fileboat's core: everything that doesn't need a window. Shared by the desktop app
//! (`src/main.rs`) and, later, the `fb` command line.

pub mod fs;
pub mod git;
pub mod index;
pub mod ops;
pub mod paths;
pub mod places;
pub mod prefs;
pub mod preview;
pub mod themes;
pub mod tools;

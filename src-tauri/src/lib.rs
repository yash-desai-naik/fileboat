//! Fileboat's core: everything that doesn't need a window. Shared by the desktop app
//! (`src/main.rs`) and, later, the `fb` command line.

pub mod claude;
pub mod fs;
pub mod git;
pub mod grep;
pub mod index;
pub mod mcp;
pub mod ops;
pub mod osfs;
pub mod paths;
pub mod places;
pub mod prefs;
pub mod preview;
pub mod themes;
pub mod tools;

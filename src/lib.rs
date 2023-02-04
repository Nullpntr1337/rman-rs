// #![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(clippy::all, clippy::unwrap_used)]

pub mod entries;
mod error;
mod file;
mod parser;

mod generated {
    #![allow(clippy::all, clippy::pedantic, clippy::nursery, clippy::unwrap_used)]
    #![allow(missing_debug_implementations, unused_imports)]
    include!(concat!(env!("OUT_DIR"), "/schema_generated.rs"));
}

pub use crate::error::{ManifestError, Result};
pub use crate::file::File;
pub use crate::parser::header::Header;
pub use crate::parser::manifest::ManifestData;
pub use crate::parser::RiotManifest;

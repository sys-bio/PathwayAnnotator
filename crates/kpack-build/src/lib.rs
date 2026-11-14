//! Builds the annotator's knowledge pack.
//!
//! The pipeline (DESIGN.md section 5): `fetch` downloads each source into a
//! local cache; `build` parses the cached files, feeds them to a
//! [`PackBuilder`], and writes the pack. Each source has a module under
//! [`sources`] that parses its files into plain records and adds them to the
//! builder.

pub mod builder;
pub mod compartments;
pub mod date;
pub mod fetch;
pub mod labels;
pub mod namespaces;
pub mod sources;

pub use builder::PackBuilder;

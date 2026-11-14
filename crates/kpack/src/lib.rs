//! The annotator's knowledge pack: curated biochemical knowledge (metabolites,
//! reactions, cross-references, names) as flat, indexed Rust data.
//!
//! Everything is an index into a flat array: entities, reactions and strings
//! are referred to by integer ids. Every fact records the sources it came
//! from, so the annotator can explain its suggestions ("from BiGG").
//!
//! This crate does no I/O. [`encode`] turns a [`Pack`] into bytes and
//! [`decode`] turns bytes back into a pack; reading and writing files, or
//! fetching over the network, is the caller's business. It compiles to
//! WebAssembly.
//!
//! The pack is built by the `kpack-build` tool. See DESIGN.md section 3.

mod data;
mod io;
mod names;
mod resource;
mod sources;
mod strings;
mod xrefs;

pub use data::{
    CompId, CompartmentDef, Entity, EntityFlags, EntityId, EntityKind, Header, Namespace, NsId,
    OrgId, Organism, Pack, Participant, Pathway, PathwayId, Reaction, ReactionFlags, ReactionId,
    SourceInfo, Xref, XrefScope,
};
pub use io::{FORMAT_VERSION, PackError, decode, encode};
pub use names::{NameHit, NameIndex, NameKind, normalize_name, strip_charge_suffix};
pub use resource::split_resource;
pub use sources::Sources;
pub use strings::{StrId, StringTable};
pub use xrefs::{Target, XrefIndex, XrefKey};

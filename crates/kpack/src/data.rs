//! The pack's data: entities, reactions, and the tables that describe them.

use serde::{Deserialize, Serialize};

use crate::names::{NameHit, NameIndex};
use crate::sources::Sources;
use crate::strings::{StrId, StringTable};
use crate::xrefs::{Target, XrefIndex};

pub type EntityId = u32;
pub type ReactionId = u32;
pub type PathwayId = u32;
pub type NsId = u16;
pub type OrgId = u16;
pub type CompId = u8;

/// The whole knowledge pack.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    pub header: Header,
    pub strings: StringTable,

    pub namespaces: Vec<Namespace>,
    pub organisms: Vec<Organism>,
    pub compartments: Vec<CompartmentDef>,

    pub entities: Vec<Entity>,
    pub reactions: Vec<Reaction>,
    pub pathways: Vec<Pathway>,

    /// Normalized entity name -> entities.
    pub names: NameIndex,
    /// Normalized reaction name -> reactions ("PFK", "hexokinase").
    pub reaction_names: NameIndex,
    /// For each entity (by id), the reactions it takes part in, sorted.
    pub by_participant: Vec<Vec<ReactionId>>,
    /// (namespace, identifier) -> entity or reaction.
    pub xrefs: XrefIndex,
}

/// What the pack is and where its data came from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Header {
    /// When the pack was built, as an ISO date (YYYY-MM-DD).
    pub built: String,
    pub sources: Vec<SourceInfo>,
}

/// One data source, for attribution and the About box.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    pub name: String,
    /// The source's own version or release, or the download date.
    pub version: String,
    pub url: String,
    pub license: String,
    /// Text the license requires to travel with the data, if any.
    pub notice: Option<String>,
}

/// An identifier namespace, as registered at identifiers.org.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Namespace {
    /// The identifiers.org prefix: `chebi`.
    pub prefix: StrId,
    /// For people: `ChEBI`.
    pub label: StrId,
    /// The identifiers.org pattern for a valid id: `^CHEBI:\d+$`.
    pub pattern: StrId,
    /// Old-style MIRIAM URN prefixes that mean this namespace.
    pub legacy_urns: Vec<StrId>,
    /// What this namespace's ids start with in identifiers.org form, when
    /// sources may leave it off: `CHEBI:` (so `15422` is `CHEBI:15422`).
    /// Empty for most namespaces.
    pub id_prefix: StrId,
}

/// An organism, for preferring organism-appropriate candidates.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Organism {
    /// NCBI taxonomy id.
    pub taxon: u32,
    pub label: StrId,
    /// "yeast", "S. cerevisiae".
    pub aliases: Vec<StrId>,
    /// BiGG models for this organism: "iMM904".
    pub bigg_models: Vec<StrId>,
}

/// A compartment as BiGG codes it, and what it means.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CompartmentDef {
    /// BiGG's code: `c`, `m`, `e`.
    pub code: StrId,
    /// For people: `cytosol`.
    pub label: StrId,
    /// The GO cellular component: `GO:0005829`. `None` if not mapped yet.
    pub go: Option<StrId>,
    /// Other names modelers use: "cytoplasm", "mito".
    pub synonyms: Vec<StrId>,
}

/// What kind of thing an entity is. It decides which databases and SBO
/// terms apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityKind {
    #[default]
    SmallMolecule,
    Protein,
    Complex,
    Gene,
    Rna,
    Macromolecule,
    Process,
    Other,
}

/// Facts about an entity that change how it is matched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityFlags(pub u16);

impl EntityFlags {
    /// A cofactor or small inorganic molecule (ATP, H2O, H+, NADH, ...) that
    /// kinetic models often leave out. The matcher treats it as optional.
    pub const CURRENCY: EntityFlags = EntityFlags(1 << 0);
    /// A class of molecules rather than one ("an acyl-CoA").
    pub const GENERIC: EntityFlags = EntityFlags(1 << 1);

    pub fn contains(self, other: EntityFlags) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: EntityFlags) {
        self.0 |= other.0;
    }
}

/// A molecule, protein, complex or gene.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub kind: EntityKind,
    /// The name to show people: "D-glucose 6-phosphate".
    pub label: StrId,
    /// Every known database id for it.
    pub xrefs: Vec<Xref>,
    /// The formula of the form BiGG balances reactions with (its default
    /// charge state), for element-balance checks.
    pub formula: Option<StrId>,
    pub charge: Option<i8>,
    pub flags: EntityFlags,
    /// "Is a kind of": more general entities in the pack (from ChEBI's
    /// "is a"). Sorted.
    pub generalizes_to: Vec<EntityId>,
    /// For complexes: "has part".
    pub parts: Vec<EntityId>,
    /// For proteins and genes.
    pub organism: Option<OrgId>,
    /// Where it occurs in BiGG, in any organism.
    pub compartments: Vec<CompId>,
    pub sources: Sources,
}

/// One database id for an entity or reaction.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Xref {
    pub ns: NsId,
    /// The id in identifiers.org form: `CHEBI:15422`, `C00002`, `2.7.1.1`.
    pub id: StrId,
    /// For a molecule, the charge of the particular form this id denotes,
    /// when known. ChEBI has separate entries for ATP and ATP(4-); this tells
    /// them apart, so the annotator can offer the form a model expects.
    pub form_charge: Option<i8>,
    /// An old id the database has merged into another (a ChEBI secondary
    /// id). Recognized when reading a model's annotations; never suggested.
    pub secondary: bool,
    /// Whether the id names this very entity, or a broader or narrower one.
    pub scope: XrefScope,
    pub sources: Sources,
}

/// How an id relates to the entity it is attached to. In SBML terms, an
/// `Exact` id goes with `bqbiol:is`, a `Broader` one with
/// `bqbiol:isVersionOf`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum XrefScope {
    /// The id names this entity (or one of its charge forms).
    #[default]
    Exact,
    /// The id names a more general class this entity is a kind of:
    /// CHEBI:17634 "D-glucose" for BiGG's D-glucopyranose.
    Broader,
    /// The id names a more specific kind of this entity:
    /// CHEBI:17925 "alpha-D-glucose" for D-glucopyranose.
    Narrower,
}

/// Facts about a reaction that change how it is matched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReactionFlags(pub u16);

impl ReactionFlags {
    /// Moves a molecule between compartments.
    pub const TRANSPORT: ReactionFlags = ReactionFlags(1 << 0);
    /// A model boundary (BiGG `EX_` reactions), not chemistry.
    pub const EXCHANGE: ReactionFlags = ReactionFlags(1 << 1);
    /// Biomass, demand, sink: bookkeeping, not chemistry.
    pub const PSEUDO: ReactionFlags = ReactionFlags(1 << 2);

    pub fn contains(self, other: ReactionFlags) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: ReactionFlags) {
        self.0 |= other.0;
    }
}

/// A reaction.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    /// "hexokinase (D-glucose:ATP)".
    pub label: StrId,
    /// bigg.reaction, rhea, ec-code, kegg.reaction, metanetx.reaction, ...
    pub xrefs: Vec<Xref>,
    pub participants: Vec<Participant>,
    /// `None` when the source does not say. Matching ignores direction.
    pub reversible: Option<bool>,
    pub flags: ReactionFlags,
    /// Enzymes, per organism (from gene-protein-reaction rules).
    pub enzymes: Vec<(OrgId, EntityId)>,
    pub pathways: Vec<PathwayId>,
    pub sources: Sources,
}

/// One molecule in a reaction.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Participant {
    pub entity: EntityId,
    /// Negative is consumed, positive is produced.
    pub coef: f32,
    pub compartment: Option<CompId>,
}

/// A pathway: a set of reactions that belong together, used as context.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pathway {
    pub label: StrId,
    pub sources: Sources,
    pub reactions: Vec<ReactionId>,
    /// Its non-currency entities, for quick overlap scoring.
    pub core_entities: Vec<EntityId>,
    pub organisms: Vec<OrgId>,
}

impl Pack {
    /// The text of a string id.
    pub fn str(&self, id: StrId) -> &str {
        self.strings.get(id)
    }

    pub fn entity(&self, id: EntityId) -> &Entity {
        &self.entities[id as usize]
    }

    pub fn reaction(&self, id: ReactionId) -> &Reaction {
        &self.reactions[id as usize]
    }

    /// The namespace with this identifiers.org prefix (`chebi`).
    pub fn namespace_id(&self, prefix: &str) -> Option<NsId> {
        self.namespaces
            .iter()
            .position(|n| self.str(n.prefix) == prefix)
            .map(|i| i as NsId)
    }

    /// The identifiers.org prefix of a namespace.
    pub fn namespace_prefix(&self, ns: NsId) -> &str {
        self.str(self.namespaces[ns as usize].prefix)
    }

    /// The entities a name may mean, best first is not implied: the caller
    /// ranks. The name is normalized first.
    pub fn entities_named(&self, name: &str) -> &[NameHit] {
        self.names.lookup(&self.strings, name)
    }

    /// The reactions a name may mean ("PFK", "hexokinase").
    pub fn reactions_named(&self, name: &str) -> &[NameHit] {
        self.reaction_names.lookup(&self.strings, name)
    }

    /// The reactions an entity takes part in.
    pub fn reactions_with(&self, entity: EntityId) -> &[ReactionId] {
        self.by_participant
            .get(entity as usize)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// What a database id names exactly: `("chebi", "CHEBI:15422")` is ATP.
    /// Old (secondary) ids count; broader and narrower links do not.
    pub fn find_xref(&self, prefix: &str, id: &str) -> Vec<Target> {
        self.find_xref_related(prefix, id)
            .into_iter()
            .filter(|(_, scope)| *scope == XrefScope::Exact)
            .map(|(t, _)| t)
            .collect()
    }

    /// Everything a database id is linked to, with how: `CHEBI:17634`
    /// (D-glucose) is `Broader` than D-glucopyranose.
    pub fn find_xref_related(&self, prefix: &str, id: &str) -> Vec<(Target, XrefScope)> {
        match self.namespace_id(prefix) {
            Some(ns) => self.xrefs.lookup(&self.strings, ns, id),
            None => Vec::new(),
        }
    }

    /// The namespace and standard id an annotation URI names, in any form
    /// models write ([`crate::split_resource`]): identifiers.org paths and
    /// compact ids, MIRIAM URNs (`urn:miriam:obo.chebi:CHEBI%3A15422`), old
    /// aliases (`obo.chebi`), ChEBI and KEGG web links. `None` for a
    /// namespace the pack does not know.
    pub fn resolve_resource(&self, uri: &str) -> Option<(NsId, String)> {
        let (written, id) = crate::split_resource(uri)?;
        let urn = |ns: &Namespace| ns.legacy_urns.iter().map(|u| self.str(*u)).collect::<Vec<_>>();
        let found = if written.starts_with("urn:miriam:") {
            self.namespaces.iter().position(|n| urn(n).contains(&written.as_str()))
        } else {
            self.namespace_id(&written).map(usize::from).or_else(|| {
                self.namespaces
                    .iter()
                    .position(|n| urn(n).iter().any(|u| u.strip_prefix("urn:miriam:") == Some(written.as_str())))
            })
        }?;
        let ns = &self.namespaces[found];
        let prefix = self.str(ns.id_prefix);
        let id = if prefix.is_empty() || id.starts_with(prefix) {
            id
        } else {
            format!("{prefix}{id}")
        };
        Some((found as NsId, id))
    }

    /// Everything an annotation URI is linked to in the pack, with how.
    /// Empty if the URI is in no known form or names nothing in the pack.
    pub fn find_resource(&self, uri: &str) -> Vec<(Target, XrefScope)> {
        match self.resolve_resource(uri) {
            Some((ns, id)) => self.xrefs.lookup(&self.strings, ns, &id),
            None => Vec::new(),
        }
    }

    /// An entity's ids in one namespace.
    pub fn entity_xrefs<'a>(
        &'a self,
        entity: EntityId,
        prefix: &str,
    ) -> impl Iterator<Item = &'a Xref> + 'a {
        let ns = self.namespace_id(prefix);
        self.entity(entity)
            .xrefs
            .iter()
            .filter(move |x| Some(x.ns) == ns)
    }
}

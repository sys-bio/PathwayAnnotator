//! Searching public databases for what the knowledge pack does not know.
//!
//! When the engine has nothing for a molecule or reaction, the user can
//! search further, on request only (it sends text to outside services):
//!
//! - molecules: all of ChEBI, through EBI's Ontology Lookup Service
//!   ([`chebi_url`], [`parse_chebi`]), and reviewed UniProt proteins, for
//!   species that are enzymes or other proteins ([`uniprot_url`],
//!   [`parse_uniprot`]);
//! - compartments: the Gene Ontology's cellular components, through OLS
//!   ([`go_url`], [`parse_go`]);
//! - a model's biological process: GO's biological processes
//!   ([`go_process_url`]), and its papers: Europe PMC ([`paper_url`],
//!   [`parse_paper`]);
//! - reactions: Rhea, by the ChEBI ids of the reaction's molecules where
//!   they are known, else by their names ([`rhea_url`], [`parse_rhea`]),
//!   ranked by how well each Rhea reaction fits ([`rank_rhea`]).
//!
//! All three services allow requests from web pages and need no key. This
//! crate builds the URLs and reads the answers; the caller fetches.

mod chebi;
mod papers;
mod rhea;
mod text;
mod uniprot;

pub use chebi::{chebi_url, go_process_url, go_url, parse_chebi, parse_go};
pub use papers::{Paper, paper_url, parse_paper};
pub use rhea::{Participant, RheaQuery, ec_from_text, ecs_of_proteins, merge_ranked, parse_rhea, rank_rhea, rhea_url};
pub use text::{percent_encode, query_from_id};
pub use uniprot::{ORGANISMS, parse_uniprot, uniprot_ec_url, uniprot_url};

/// The databases searched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Db {
    Chebi,
    Uniprot,
    Rhea,
    /// The Gene Ontology's cellular components: compartments.
    Go,
}

impl Db {
    /// "ChEBI", "UniProt", "Rhea".
    pub fn label(self) -> &'static str {
        match self {
            Db::Chebi => "ChEBI",
            Db::Uniprot => "UniProt",
            Db::Rhea => "Rhea",
            Db::Go => "GO",
        }
    }

    /// The SBO term for what an entry of this database is: simple chemical,
    /// polypeptide chain, biochemical reaction, physical compartment.
    pub fn sbo(self) -> u32 {
        match self {
            Db::Chebi => 247,
            Db::Uniprot => 252,
            Db::Rhea => 176,
            Db::Go => 290,
        }
    }

    /// Who receives the search text, for telling the user before they
    /// search.
    pub fn host(self) -> &'static str {
        match self {
            Db::Chebi | Db::Go => "EBI (www.ebi.ac.uk)",
            Db::Uniprot => "UniProt (rest.uniprot.org)",
            Db::Rhea => "Rhea (www.rhea-db.org)",
        }
    }
}

/// One entry found.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub db: Db,
    /// As the database writes it: "CHEBI:57579", "P08237", "RHEA:17825".
    pub id: String,
    pub label: String,
    /// One line more: a definition, an organism and gene, an equation.
    pub detail: Option<String>,
    pub synonyms: Vec<String>,
    /// The annotation URI accepting it writes (identifiers.org).
    pub uri: String,
    /// The search text is its name or one of its synonyms.
    pub exact: bool,
    /// Higher is better; only comparable within one search.
    pub score: f32,
    /// Why it ranks where it does, in words ("matches all 3 molecules").
    pub why: Option<String>,
    /// EC numbers ("2.7.1.11"): of a protein's activity, or a reaction's.
    pub ec: Vec<String>,
}

/// Puts exact name matches first, keeping each database's own order
/// otherwise.
pub fn exact_first(hits: &mut [Hit]) {
    hits.sort_by_key(|h| !h.exact);
}

/// Whether `query` names this entry: its label or a synonym, compared as
/// the pack compares names.
pub(crate) fn names_match(query: &str, label: &str, synonyms: &[String]) -> bool {
    let q = kpack::normalize_name(query);
    !q.is_empty() && std::iter::once(label).chain(synonyms.iter().map(String::as_str)).any(|n| kpack::normalize_name(n) == q)
}

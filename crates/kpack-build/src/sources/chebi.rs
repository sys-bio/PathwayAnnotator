//! ChEBI (ebi.ac.uk/chebi): names, synonyms, charge forms and old ids for
//! the molecules already in the pack.
//!
//! ChEBI has over 200,000 compounds; the annotator needs the few thousand
//! that are metabolites. So ChEBI does not add entities. It enriches the ones
//! BiGG made:
//!
//! 1. **Seeds.** Every ChEBI id a BiGG metabolite already has.
//! 2. **Charge forms.** From each seed, follow ChEBI's "is conjugate acid
//!    of", "is conjugate base of" and "is tautomer of" links (up to
//!    [`MAX_HOPS`] steps). A model may annotate ATP with any of its
//!    protonation states; all of them should lead back to the same entity.
//!    Every id belongs to the nearest seed, so two entities never share one.
//! 3. **Ids.** Each id found is added to its entity, with its charge, and so
//!    are ChEBI's secondary (merged, old) ids for it, marked secondary.
//! 4. **Names.** Each id's curated English names become names of the entity.
//!    Brand names, unreviewed submissions and deprecated entries are left
//!    out ("Glucose injection" and "D5W" are not names for D-glucose).
//! 5. **Broader and narrower ids.** ChEBI's "is a" links, from the ids found
//!    so far. Up to [`MAX_UP`] steps up, an id becomes a `Broader` xref
//!    (CHEBI:17634 "D-glucose" for BiGG's D-glucopyranose), unless it covers
//!    more than [`MAX_FANOUT`] entities: "organophosphate oxoanion" says
//!    nothing useful about which molecule a model means. One step down, an
//!    id becomes a `Narrower` xref ("alpha-D-glucose"). Where the other end
//!    is itself an entity, the link is `Entity::generalizes_to` instead.
//!    These ids bring no names: "hexose" is not a name for glucose.
//!
//! Files: the TSV flat files from ftp.ebi.ac.uk/pub/databases/chebi/flat_files.
//! Columns are found by their header names, not their positions.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::io::{BufReader, Read};
use std::path::Path;

use flate2::read::GzDecoder;

use kpack::{EntityId, NameHit, NameKind, SourceInfo, Sources, Xref, XrefScope};

use crate::builder::{PackBuilder, merge_xref};

pub const BASE_URL: &str = "https://ftp.ebi.ac.uk/pub/databases/chebi/flat_files/";
pub const COMPOUNDS_FILE: &str = "compounds.tsv.gz";
pub const NAMES_FILE: &str = "names.tsv.gz";
pub const CHEMICAL_DATA_FILE: &str = "chemical_data.tsv.gz";
pub const SECONDARY_IDS_FILE: &str = "secondary_ids.tsv.gz";
pub const RELATION_FILE: &str = "relation.tsv.gz";
pub const README_FILE: &str = "README";
/// Everything `fetch` downloads.
pub const FILES: &[&str] = &[
    README_FILE,
    COMPOUNDS_FILE,
    NAMES_FILE,
    CHEMICAL_DATA_FILE,
    SECONDARY_IDS_FILE,
    RELATION_FILE,
];

/// How far to follow charge-form links from a BiGG id.
pub const MAX_HOPS: usize = 3;
/// How many "is a" steps up to look for broader ids.
pub const MAX_UP: usize = 2;
/// A broader id that covers more entities than this is too general to keep.
pub const MAX_FANOUT: usize = 10;

// ChEBI's status ids (status.tsv) and relation types (relation_type.tsv).
const STATUS_CHECKED: u8 = 1;
const STATUS_OK: u8 = 3;
const STATUS_DEPRECATED: u8 = 7;
const IS_A: u8 = 5;
const IS_CONJUGATE_ACID_OF: u8 = 6;
const IS_CONJUGATE_BASE_OF: u8 = 7;
const IS_TAUTOMER_OF: u8 = 11;

/// Name types kept. Brand names are left out; INNs are drug names but real
/// ones ("adenosine triphosphate" has none, many drugs do).
const NAME_TYPES: &[&str] = &["SYNONYM", "IUPAC NAME", "UNIPROT NAME", "INN"];

// --- parsed data -------------------------------------------------------------

/// One compound.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Compound {
    pub name: Option<String>,
    pub status: u8,
    pub formula: Option<String>,
    pub charge: Option<i32>,
}

/// What the pack needs from ChEBI.
#[derive(Debug, Default)]
pub struct Chebi {
    /// From README ("257"), or "unknown".
    pub release: String,
    pub compounds: HashMap<u32, Compound>,
    /// Accepted names, per compound.
    pub names: HashMap<u32, Vec<String>>,
    /// Primary id -> its secondary ids.
    pub secondaries: HashMap<u32, Vec<u32>>,
    /// Secondary id -> its primary id.
    pub primary_of: HashMap<u32, u32>,
    /// Charge-form and tautomer links, both ways.
    pub forms: HashMap<u32, Vec<u32>>,
    /// "is a": id -> the more general ids it is a kind of.
    pub parents: HashMap<u32, Vec<u32>>,
    /// The reverse: id -> the more specific ids that are kinds of it.
    pub children: HashMap<u32, Vec<u32>>,
    /// Rows read from each file, for the build report.
    pub rows: Vec<(&'static str, usize)>,
}

impl Chebi {
    /// The primary id for `id`: itself, unless it is a secondary id.
    pub fn primary(&self, id: u32) -> u32 {
        self.primary_of.get(&id).copied().unwrap_or(id)
    }
}

/// Whether a name row is worth keeping.
pub fn accept_name(kind: &str, status: u8, language: &str) -> bool {
    language == "en"
        && (status == STATUS_CHECKED || status == STATUS_OK)
        && NAME_TYPES.contains(&kind)
}

/// A ChEBI name without HTML markup. The "ASCII name" column still has
/// some: `(2<i>R</i>)-3-phosphoglyceroyl phosphate`.
pub fn strip_tags(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_tag = false;
    for c in name.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// `CHEBI:15422` or `15422` -> 15422.
pub fn chebi_number(id: &str) -> Option<u32> {
    id.strip_prefix("CHEBI:").unwrap_or(id).parse().ok()
}

// --- reading the files ---------------------------------------------------------

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// A TSV row, read by column name.
struct Row<'a> {
    fields: Vec<Cow<'a, str>>,
    columns: &'a HashMap<String, usize>,
}

impl Row<'_> {
    /// The field in `column`, or "" if the row is short.
    fn get(&self, column: &str) -> &str {
        self.columns
            .get(column)
            .and_then(|&i| self.fields.get(i))
            .map(|f| f.as_ref())
            .unwrap_or("")
    }

    fn number<T: std::str::FromStr>(&self, column: &str) -> Option<T> {
        self.get(column).trim().parse().ok()
    }

    /// The field, or `None` if empty.
    fn text(&self, column: &str) -> Option<String> {
        let s = self.get(column).trim();
        (!s.is_empty()).then(|| s.to_string())
    }
}

/// The records of a TSV text. A field that starts with `"` is quoted: it
/// runs to the next lone `"`, may contain tabs and line breaks, and `""`
/// inside it is one `"`. ChEBI quotes its definitions this way, and a few
/// run over several lines.
struct Records<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Iterator for Records<'a> {
    type Item = Vec<Cow<'a, str>>;

    fn next(&mut self) -> Option<Self::Item> {
        let s = self.text;
        if self.pos >= s.len() {
            return None;
        }
        let mut fields = Vec::new();
        loop {
            let rest = &s[self.pos..];
            if let Some(quoted) = rest.strip_prefix('"') {
                self.pos += 1;
                let mut value = String::new();
                let mut inner = quoted;
                loop {
                    match inner.find('"') {
                        None => {
                            value.push_str(inner);
                            self.pos = s.len();
                            break;
                        }
                        Some(i) => {
                            value.push_str(&inner[..i]);
                            self.pos += i + 1;
                            if s[self.pos..].starts_with('"') {
                                value.push('"');
                                self.pos += 1;
                                inner = &s[self.pos..];
                            } else {
                                break;
                            }
                        }
                    }
                }
                fields.push(Cow::Owned(value));
                // Anything between the closing quote and the separator is
                // ignored.
                self.pos = s[self.pos..].find(['\t', '\n']).map_or(s.len(), |i| self.pos + i);
            } else {
                let end = rest.find(['\t', '\n']).map_or(s.len(), |i| self.pos + i);
                fields.push(Cow::Borrowed(s[self.pos..end].trim_end_matches('\r')));
                self.pos = end;
            }
            if self.pos >= s.len() {
                break;
            }
            let sep = s.as_bytes()[self.pos];
            self.pos += 1;
            if sep == b'\n' {
                break;
            }
        }
        Some(fields)
    }
}

/// Calls `f` for every row of a TSV file. The first record is the header;
/// it must name every column in `required`. Returns the file's name and its
/// number of rows.
fn for_each_row(
    mut reader: impl Read,
    file: &'static str,
    required: &[&str],
    mut f: impl FnMut(&Row),
) -> Result<(&'static str, usize)> {
    let mut text = String::new();
    reader
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read {file}: {e}"))?;
    let mut records = Records { text: &text, pos: 0 };
    let header = records.next().ok_or_else(|| format!("{file} is empty"))?;
    let columns: HashMap<String, usize> = header
        .iter()
        .enumerate()
        .map(|(i, c)| (c.trim().to_string(), i))
        .collect();
    for c in required {
        if !columns.contains_key(*c) {
            return Err(format!("{file} has no column {c:?}; ChEBI changed its format?").into());
        }
    }
    let mut n = 0;
    for fields in records {
        if fields.len() == 1 && fields[0].is_empty() {
            continue;
        }
        n += 1;
        f(&Row {
            fields,
            columns: &columns,
        });
    }
    Ok((file, n))
}

fn open_gz(dir: &Path, file: &str) -> Result<impl Read> {
    let path = dir.join(file);
    let f = std::fs::File::open(&path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    Ok(GzDecoder::new(BufReader::new(f)))
}

/// The release number from ChEBI's README ("ChEBI Release:         257").
pub fn release_from_readme(readme: &str) -> Option<String> {
    readme.lines().find_map(|l| {
        let rest = l.trim_start_matches(['*', ' ']).strip_prefix("ChEBI Release:")?;
        Some(rest.trim().to_string())
    })
}

/// Reads the cached ChEBI files in `dir`.
pub fn parse(dir: &Path) -> Result<Chebi> {
    let mut c = Chebi {
        release: std::fs::read_to_string(dir.join(README_FILE))
            .ok()
            .and_then(|r| release_from_readme(&r))
            .unwrap_or_else(|| "unknown".into()),
        ..Chebi::default()
    };
    let mut rows = Vec::new();

    rows.push(for_each_row(
        open_gz(dir, COMPOUNDS_FILE)?,
        COMPOUNDS_FILE,
        &["id", "status_id", "ascii_name"],
        |r| {
            if let Some(id) = r.number("id") {
                c.compounds.insert(
                    id,
                    Compound {
                        name: r.text("ascii_name").map(|n| strip_tags(&n)),
                        status: r.number("status_id").unwrap_or(0),
                        ..Compound::default()
                    },
                );
            }
        },
    )?);

    rows.push(for_each_row(
        open_gz(dir, CHEMICAL_DATA_FILE)?,
        CHEMICAL_DATA_FILE,
        &["compound_id", "formula", "charge", "status_id"],
        |r| {
            let (Some(id), Some(status)) = (r.number::<u32>("compound_id"), r.number::<u8>("status_id"))
            else {
                return;
            };
            if status == STATUS_DEPRECATED {
                return;
            }
            if let Some(comp) = c.compounds.get_mut(&id) {
                // The first live row wins.
                if comp.formula.is_none() && comp.charge.is_none() {
                    comp.formula = r.text("formula");
                    comp.charge = r.number("charge");
                }
            }
        },
    )?);

    rows.push(for_each_row(
        open_gz(dir, NAMES_FILE)?,
        NAMES_FILE,
        &["compound_id", "type", "status_id", "language_code", "ascii_name"],
        |r| {
            let Some(id) = r.number::<u32>("compound_id") else {
                return;
            };
            let status = r.number("status_id").unwrap_or(0);
            if !accept_name(r.get("type"), status, r.get("language_code")) {
                return;
            }
            if let Some(name) = r.text("ascii_name").map(|n| strip_tags(&n)) {
                c.names.entry(id).or_default().push(name);
            }
        },
    )?);

    rows.push(for_each_row(
        open_gz(dir, SECONDARY_IDS_FILE)?,
        SECONDARY_IDS_FILE,
        &["compound_id", "secondary_id"],
        |r| {
            if let (Some(primary), Some(secondary)) =
                (r.number::<u32>("compound_id"), r.number::<u32>("secondary_id"))
            {
                c.secondaries.entry(primary).or_default().push(secondary);
                c.primary_of.insert(secondary, primary);
            }
        },
    )?);

    rows.push(for_each_row(
        open_gz(dir, RELATION_FILE)?,
        RELATION_FILE,
        &["relation_type_id", "init_id", "final_id", "status_id"],
        |r| {
            let kind: u8 = r.number("relation_type_id").unwrap_or(0);
            let status: u8 = r.number("status_id").unwrap_or(0);
            if status == STATUS_DEPRECATED {
                return;
            }
            // `init_id <relation> final_id`: 4167 is_a 17634 means
            // D-glucopyranose is a D-glucose.
            let (Some(a), Some(b)) = (r.number::<u32>("init_id"), r.number::<u32>("final_id")) else {
                return;
            };
            match kind {
                IS_CONJUGATE_ACID_OF | IS_CONJUGATE_BASE_OF | IS_TAUTOMER_OF => {
                    c.forms.entry(a).or_default().push(b);
                    c.forms.entry(b).or_default().push(a);
                }
                IS_A => {
                    c.parents.entry(a).or_default().push(b);
                    c.children.entry(b).or_default().push(a);
                }
                _ => {}
            }
        },
    )?);
    for list in c
        .forms
        .values_mut()
        .chain(c.parents.values_mut())
        .chain(c.children.values_mut())
    {
        list.sort_unstable();
        list.dedup();
    }
    c.rows = rows;
    Ok(c)
}

// --- into the pack -------------------------------------------------------------

/// What [`enrich`] did.
#[derive(Debug, Default)]
pub struct EnrichStats {
    /// Entities that gained anything from ChEBI.
    pub entities: usize,
    /// ChEBI ids from BiGG that ChEBI knows.
    pub seeds: usize,
    /// ChEBI ids from BiGG that ChEBI does not know (or has deleted).
    pub seeds_unknown: usize,
    /// ChEBI ids from BiGG already claimed by another entity.
    pub shared_seeds: usize,
    /// Charge forms and tautomers added beyond the seeds.
    pub forms_added: usize,
    pub secondary_ids_added: usize,
    pub names_added: usize,
    /// "is a" ids added as `Broader` / `Narrower` xrefs.
    pub broader_added: usize,
    pub narrower_added: usize,
    /// Broader ids left out for covering more than [`MAX_FANOUT`] entities.
    pub broad_classes_dropped: usize,
    /// `Entity::generalizes_to` links made.
    pub entity_links: usize,
    /// Rows read from each file.
    pub rows: Vec<(&'static str, usize)>,
}

/// Adds ChEBI's ids and names to the entities already in the builder.
pub fn enrich(b: &mut PackBuilder, c: &Chebi) -> EnrichStats {
    let mut stats = EnrichStats::default();
    let chebi_ns = b.ns("chebi");

    // 1. Seeds, in entity order, so the result does not depend on hashing.
    let mut owner: HashMap<u32, EntityId> = HashMap::new();
    let mut queue: VecDeque<(u32, usize)> = VecDeque::new();
    for (eid, e) in b.entities.iter().enumerate() {
        let eid = eid as EntityId;
        for x in e.xrefs.iter().filter(|x| x.ns == chebi_ns) {
            let Some(n) = chebi_number(b.strings.get(x.id)) else {
                continue;
            };
            let n = c.primary(n);
            if !c.compounds.contains_key(&n) {
                stats.seeds_unknown += 1;
                continue;
            }
            match owner.get(&n) {
                Some(&o) if o != eid => stats.shared_seeds += 1,
                Some(_) => {}
                None => {
                    stats.seeds += 1;
                    owner.insert(n, eid);
                    queue.push_back((n, 0));
                }
            }
        }
    }

    // 2. Charge forms: breadth first from all seeds at once, so each id goes
    //    to the nearest seed.
    while let Some((n, depth)) = queue.pop_front() {
        if depth == MAX_HOPS {
            continue;
        }
        let e = owner[&n];
        for &m in c.forms.get(&n).into_iter().flatten() {
            if owner.contains_key(&m) || !c.compounds.contains_key(&m) {
                continue;
            }
            owner.insert(m, e);
            queue.push_back((m, depth + 1));
            stats.forms_added += 1;
        }
    }

    // 3 and 4. Ids and names, in a fixed order.
    let mut claimed: Vec<(EntityId, u32)> = owner.iter().map(|(&n, &e)| (e, n)).collect();
    claimed.sort_unstable();
    let mut touched = HashSet::new();
    for (e, n) in claimed {
        let comp = &c.compounds[&n];
        if comp.status == STATUS_DEPRECATED {
            continue;
        }
        touched.insert(e);
        let primary = chebi_xref(b, c, n, n, false, XrefScope::Exact);
        merge_xref(&mut b.entities[e as usize].xrefs, primary);
        for &s in c.secondaries.get(&n).into_iter().flatten() {
            let x = chebi_xref(b, c, s, n, true, XrefScope::Exact);
            merge_xref(&mut b.entities[e as usize].xrefs, x);
            stats.secondary_ids_added += 1;
        }

        let hit = NameHit {
            target: e,
            kind: NameKind::Synonym,
            sources: Sources::CHEBI,
            count: 0,
        };
        let names = comp.name.iter().chain(c.names.get(&n).into_iter().flatten());
        for name in names {
            b.name_entity(name, hit);
            stats.names_added += 1;
        }
    }

    // 5. "is a": broader ids up, narrower ids down, entity-to-entity links.
    let live = |n: &u32| c.compounds.get(n).is_some_and(|k| k.status != STATUS_DEPRECATED);
    let mut broader: BTreeMap<u32, BTreeSet<EntityId>> = BTreeMap::new();
    let mut narrower: BTreeMap<u32, BTreeSet<EntityId>> = BTreeMap::new();
    let mut general: BTreeSet<(EntityId, EntityId)> = BTreeSet::new();
    for (&n, &e) in &owner {
        if !live(&n) {
            continue;
        }
        let mut frontier = vec![n];
        for step in 0..MAX_UP {
            let mut next = Vec::new();
            for f in &frontier {
                for &p in c.parents.get(f).into_iter().flatten().filter(|p| live(p)) {
                    match owner.get(&p) {
                        // The parent is another entity: link the entities, and
                        // stop climbing, since the parent's own broader ids
                        // are its business.
                        Some(&o) if o != e => {
                            if step == 0 {
                                general.insert((e, o));
                            }
                        }
                        // A form of this same entity.
                        Some(_) => {}
                        None => {
                            broader.entry(p).or_default().insert(e);
                            next.push(p);
                        }
                    }
                }
            }
            frontier = next;
        }
        for &k in c.children.get(&n).into_iter().flatten() {
            if live(&k) && !owner.contains_key(&k) {
                narrower.entry(k).or_default().insert(e);
            }
        }
    }
    for (p, entities) in &broader {
        if entities.len() > MAX_FANOUT {
            stats.broad_classes_dropped += 1;
            continue;
        }
        for &e in entities {
            let x = chebi_xref(b, c, *p, *p, false, XrefScope::Broader);
            merge_xref(&mut b.entities[e as usize].xrefs, x);
            touched.insert(e);
            stats.broader_added += 1;
        }
    }
    for (k, entities) in &narrower {
        for &e in entities {
            let x = chebi_xref(b, c, *k, *k, false, XrefScope::Narrower);
            merge_xref(&mut b.entities[e as usize].xrefs, x);
            touched.insert(e);
            stats.narrower_added += 1;
        }
    }
    for &(e, o) in &general {
        let list = &mut b.entities[e as usize].generalizes_to;
        if !list.contains(&o) {
            list.push(o);
            list.sort_unstable();
            stats.entity_links += 1;
        }
    }

    // A ChEBI id BiGG gave that is really a secondary id is marked so.
    for &e in &touched {
        let entity = &mut b.entities[e as usize];
        entity.sources |= Sources::CHEBI;
        for x in entity.xrefs.iter_mut().filter(|x| x.ns == chebi_ns) {
            if chebi_number(b.strings.get(x.id)).is_some_and(|n| c.primary_of.contains_key(&n)) {
                x.secondary = true;
            }
        }
    }
    stats.entities = touched.len();
    stats
}

/// The xref for ChEBI id `number`, with the charge of compound `compound`
/// (the primary id, when `number` is a secondary one).
fn chebi_xref(
    b: &mut PackBuilder,
    c: &Chebi,
    number: u32,
    compound: u32,
    secondary: bool,
    scope: XrefScope,
) -> Xref {
    Xref {
        ns: b.ns("chebi"),
        id: b.strings.intern(&format!("CHEBI:{number}")),
        form_charge: c
            .compounds
            .get(&compound)
            .and_then(|k| k.charge)
            .and_then(|q| i8::try_from(q).ok()),
        secondary,
        scope,
        sources: Sources::CHEBI,
    }
}

/// The attribution ChEBI asks for (README).
pub fn notice(release: &str) -> String {
    format!("ChEBI data is from https://www.ebi.ac.uk/chebi - the version of ChEBI is {release}.")
}

/// Parses the cached ChEBI files in `dir` and adds them to the pack.
pub fn load(b: &mut PackBuilder, dir: &Path) -> Result<EnrichStats> {
    let c = parse(dir)?;
    let mut stats = enrich(b, &c);
    stats.rows = c.rows.clone();
    b.add_source(SourceInfo {
        name: "ChEBI".into(),
        version: c.release.clone(),
        url: "https://www.ebi.ac.uk/chebi".into(),
        license: "CC BY 4.0".into(),
        notice: Some(notice(&c.release)),
    });
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_filter() {
        assert!(accept_name("SYNONYM", 1, "en"));
        assert!(accept_name("IUPAC NAME", 3, "en"));
        assert!(!accept_name("BRAND NAME", 1, "en"));
        assert!(!accept_name("SYNONYM", 9, "en"), "unreviewed submission");
        assert!(!accept_name("SYNONYM", 7, "en"), "deprecated");
        assert!(!accept_name("SYNONYM", 1, "de"));
    }

    #[test]
    fn html_is_stripped_from_names() {
        assert_eq!(strip_tags("(2<i>R</i>)-3-phosphoglyceroyl phosphate"), "(2R)-3-phosphoglyceroyl phosphate");
        assert_eq!(strip_tags("H<small><sub>4</sub></small>atp"), "H4atp");
        assert_eq!(strip_tags("plain"), "plain");
    }

    #[test]
    fn ids_and_readme() {
        assert_eq!(chebi_number("CHEBI:15422"), Some(15422));
        assert_eq!(chebi_number("15422"), Some(15422));
        assert_eq!(chebi_number("CHEBI:x"), None);
        let readme = "****\n*\n* ChEBI Release:         257\n*\n* Date of last update:   2026-11-05\n";
        assert_eq!(release_from_readme(readme).as_deref(), Some("257"));
    }

    #[test]
    fn rows_are_read_by_column_name() {
        let tsv = "b\ta\n2\t1\n\n4\t3\n";
        let mut seen = Vec::new();
        let (_, n) = for_each_row(tsv.as_bytes(), "t", &["a", "b"], |r| {
            seen.push((r.get("a").to_string(), r.number::<u32>("b")));
        })
        .unwrap();
        assert_eq!(n, 2);
        assert_eq!(seen, [("1".into(), Some(2)), ("3".into(), Some(4))]);
        assert!(for_each_row("x\n".as_bytes(), "t", &["a"], |_| {}).is_err());
    }

    /// A builder with one entity per ChEBI id in `ids`, and a ChEBI that
    /// knows those ids plus `extra`.
    fn setup(ids: &[u32], extra: &[u32]) -> (PackBuilder, Chebi) {
        let mut b = PackBuilder::new();
        let mut c = Chebi::default();
        for &n in ids {
            let x = b.xref("chebi", &n.to_string(), Sources::BIGG, None);
            b.add_entity(kpack::Entity {
                xrefs: vec![x],
                ..kpack::Entity::default()
            });
        }
        for &n in ids.iter().chain(extra) {
            c.compounds.insert(
                n,
                Compound {
                    status: STATUS_CHECKED,
                    ..Compound::default()
                },
            );
        }
        (b, c)
    }

    fn is_a(c: &mut Chebi, child: u32, parent: u32) {
        c.parents.entry(child).or_default().push(parent);
        c.children.entry(parent).or_default().push(child);
    }

    fn scopes(b: &PackBuilder, e: usize) -> Vec<(String, XrefScope)> {
        b.entities[e]
            .xrefs
            .iter()
            .map(|x| (b.strings.get(x.id).to_string(), x.scope))
            .collect()
    }

    #[test]
    fn too_broad_classes_are_dropped() {
        // Entities 1..=11 are all "is a" 100; entity 1 is also "is a" 200.
        let ids: Vec<u32> = (1..=11).collect();
        let (mut b, mut c) = setup(&ids, &[100, 200]);
        for &n in &ids {
            is_a(&mut c, n, 100);
        }
        is_a(&mut c, 1, 200);
        let stats = enrich(&mut b, &c);
        assert_eq!(stats.broad_classes_dropped, 1);
        assert_eq!(stats.broader_added, 1);
        let first = scopes(&b, 0);
        assert!(first.contains(&("CHEBI:200".into(), XrefScope::Broader)), "{first:?}");
        assert!(!first.iter().any(|(id, _)| id == "CHEBI:100"), "{first:?}");
    }

    #[test]
    fn a_parent_that_is_an_entity_becomes_a_kind_of_link() {
        // Entity 0 (CHEBI:1) is a entity 1 (CHEBI:2), which is a CHEBI:3.
        let (mut b, mut c) = setup(&[1, 2], &[3]);
        is_a(&mut c, 1, 2);
        is_a(&mut c, 2, 3);
        let stats = enrich(&mut b, &c);
        assert_eq!(stats.entity_links, 1);
        assert_eq!(b.entities[0].generalizes_to, [1]);
        assert!(b.entities[1].generalizes_to.is_empty());
        // CHEBI:2 stays entity 1's own id, not a broader id of entity 0, and
        // the climb stops there: CHEBI:3 is entity 1's broader id only.
        assert!(!scopes(&b, 0).iter().any(|(id, _)| id == "CHEBI:2" || id == "CHEBI:3"));
        assert!(scopes(&b, 1).contains(&("CHEBI:3".into(), XrefScope::Broader)));
    }

    #[test]
    fn quoted_fields_may_hold_tabs_quotes_and_line_breaks() {
        // As in ChEBI's compounds.tsv, where a few definitions run over
        // several lines.
        let tsv = "id\tdefinition\tascii_name\n\
                   1\t\"spans\ntwo lines\"\tfirst\n\
                   2\t\"\"\tsecond\n\
                   3\t\"has \"\"quotes\"\" and\ta tab\"\tthird\r\n";
        let mut seen = Vec::new();
        let (_, n) = for_each_row(tsv.as_bytes(), "t", &["id", "ascii_name"], |r| {
            seen.push((r.get("id").to_string(), r.get("definition").to_string(), r.get("ascii_name").to_string()));
        })
        .unwrap();
        assert_eq!(n, 3);
        assert_eq!(seen[0], ("1".into(), "spans\ntwo lines".into(), "first".into()));
        assert_eq!(seen[1], ("2".into(), "".into(), "second".into()));
        assert_eq!(seen[2], ("3".into(), "has \"quotes\" and\ta tab".into(), "third".into()));
    }
}

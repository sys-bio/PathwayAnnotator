# SBML Annotator - Crate Layout and Knowledge-Pack Data Model

Status: draft, 2026-11-13

## 1. Goals that shape the design

- Runs entirely in the browser (WASM on GitHub Pages). No server, no API keys.
- The intelligence is rules plus statistics over curated data (BiGG, BioModels,
  Rhea, MetaNetX, ChEBI, Reactome), shipped as an offline **knowledge pack**.
- Audience is biologists and modelers: the engine must explain every suggestion.
- The engine is UI-independent so the same code drives the web app, a CLI batch
  annotator, and an accuracy benchmark.
- Reuse `sbml-io` and `antimony-rs` for model I/O. Both are copied into this
  workspace (`crates/`) from PathwayDesigner/crates on 2026-11-13, so the
  project builds on its own (locally and in GitHub Actions). The copies will
  drift from PathwayDesigner; port fixes across by hand when needed.
- v1 scope: **metabolism only** (BiGG, Rhea, MetaNetX, ChEBI, BioModels).
  Signaling (Reactome) comes later.

## 2. Workspace layout

```
Annotator/
  Cargo.toml                 workspace (edition 2024, resolver 3)
  DESIGN.md
  crates/
    sbml-io/                 copied from PathwayDesigner (SBML read/write, MIRIAM)
    antimony/                copied from PathwayDesigner (package antimony-rs)
    kpack/                   knowledge-pack data types + load/save. No network,
                             no file system. Used by builder and app. WASM-safe.
    kpack-build/             native CLI: download sources, parse, reconcile,
                             derive indexes, write the pack. Never compiled to WASM.
    annot-model/             the model being annotated: wraps sbml-io /
                             antimony-rs, annotation state, edits, undo/redo,
                             export (SBML, Antimony, CSV, report). WASM-safe.
    annot-engine/            inference: name decoder, candidate generation,
                             evidence providers, propagation, scoring, checks.
                             Depends on kpack + annot-model. WASM-safe.
    annot-app/               the UI (WASM, also runs natively for development).
    annot-cli/               native: batch annotation + benchmark harness.
  packs/                     built packs (git-ignored; published by CI)
  bench/                     benchmark model lists and results
```

Dependency graph (arrows = "depends on"):

```
  kpack-build --> kpack
  annot-engine --> kpack
  annot-model --> annot-engine, kpack, sbml-io, antimony-rs
  annot-app --> annot-engine, annot-model, kpack
  annot-cli --> annot-engine, annot-model, kpack, kpack-build

(Changed 2026-11-14: the engine has its own SBML-free input type,
`annot_engine::Model`, and `annot-model` produces it, so `annot-model`
depends on the engine rather than the other way round.)
```

Rules:
- `kpack`, `annot-model`, `annot-engine` do no I/O. The caller hands them bytes
  or strings. This keeps them WASM-safe and easy to test.
- Only `kpack-build` and `annot-cli` touch the network or file system.
- `kpack` has minimal dependencies (serde, postcard, ruzstd).

UI framework for `annot-app`: **Leptos** (client-side rendering, built with
Trunk). Reason: this app is mostly text, tables, cards, search boxes,
structure images and links. An HTML-based UI gets text selection, copy/paste,
browser find (Ctrl+F), screen readers, translation, and CSS styling for free,
which egui would have to reimplement. Leptos is the most mature Rust option
for a web-only app. The engine crates do not depend on the UI choice.

## 3. Knowledge-pack data model (`kpack`)

### 3.1 Principles

- **Integer IDs everywhere.** Entities, reactions, strings, namespaces are
  indexes into flat arrays. Small, fast, cache-friendly, trivial to serialize.
- **Interned strings.** One string table; everything else stores `StrId`.
- **Provenance on every fact.** Each name, xref, and reaction carries the
  sources it came from, so the UI can say "from BiGG" or "seen in 37 curated
  BioModels".
- **Canonical hub IDs.** Metabolites are keyed on MetaNetX (MNXM) internally,
  which reconciles ChEBI, BiGG, KEGG, SEED, HMDB. Proteins are keyed on UniProt.
- **Store facts, derive the rest at load time.** Neighborhoods, trigram fuzzy
  indexes, and degree statistics are computed in the browser after loading
  rather than shipped.

Encoding: `postcard` (serde) compressed with zstd. The builder uses the `zstd`
crate; the app decodes with pure-Rust `ruzstd`. If load time becomes a problem,
switch to `rkyv` for zero-copy access; the type layout below is compatible.

### 3.2 Top level

```rust
pub struct Pack {
    pub header: Header,
    pub strings: StringTable,

    pub namespaces: Vec<Namespace>,     // chebi, uniprot, rhea, ec-code, go, ...
    pub organisms: Vec<Organism>,
    pub compartments: Vec<CompartmentDef>,
    pub sbo: Vec<SboTerm>,              // subset used for typing and checks

    pub entities: Vec<Entity>,          // molecules, proteins, complexes, genes
    pub reactions: Vec<Reaction>,
    pub pathways: Vec<Pathway>,

    pub names: NameIndex,               // normalized name -> entities
    pub reaction_names: NameIndex,      // "PFK", "hexokinase" -> reactions
    pub by_participant: Postings<ReactionId>, // entity -> reactions containing it
    pub lexicon: Lexicon,               // name-decoder rules (affixes, abbrevs)

    pub xref_lookup: XrefIndex,         // (namespace, id) -> entity / reaction
}

pub struct Header {
    pub format_version: u32,            // bump on any layout change
    pub built: String,                  // ISO date
    pub sources: Vec<SourceInfo>,       // name, version/date, license, url
}

pub type StrId = u32;
pub type EntityId = u32;
pub type ReactionId = u32;
pub type PathwayId = u32;
pub type NsId = u16;
pub type OrgId = u16;
pub type CompId = u8;
```

### 3.3 Sources (provenance)

```rust
bitflags! {
    pub struct Sources: u16 {
        const BIGG       = 1 << 0;
        const BIOMODELS  = 1 << 1;
        const CHEBI      = 1 << 2;
        const RHEA       = 1 << 3;
        const METANETX   = 1 << 4;
        const REACTOME   = 1 << 5;
        const UNIPROT    = 1 << 6;
        const GO         = 1 << 7;
        const CURATED    = 1 << 15;     // hand-maintained overrides in this repo
    }
}
```

### 3.4 Namespaces (identifier registry)

```rust
pub struct Namespace {
    pub prefix: StrId,        // "chebi"   (identifiers.org prefix)
    pub label: StrId,         // "ChEBI"
    pub pattern: StrId,       // "^CHEBI:\d+$"   for validation
    pub uri_template: StrId,  // "https://identifiers.org/chebi/{id}"
    pub legacy_urns: Vec<StrId>, // "urn:miriam:obo.chebi" for upgrading old models
}
```

### 3.5 Entities

```rust
pub enum EntityKind {
    SmallMolecule, Protein, Complex, Gene, Rna, Macromolecule, Process, Other,
}

pub struct Entity {
    pub kind: EntityKind,
    pub label: StrId,                 // preferred human name, "D-glucose 6-phosphate"
    pub xrefs: Vec<Xref>,             // all known database IDs
    pub formula: Option<StrId>,       // "C6H11O9P" (for element balance)
    pub charge: Option<i8>,
    pub flags: EntityFlags,           // CURRENCY (ATP, H2O, H+, ...), GENERIC, ...
    pub generalizes_to: Vec<EntityId>,// "is a kind of": alpha-D-glucose -> D-glucose
    pub parts: Vec<EntityId>,         // for complexes: "has part"
    pub organism: Option<OrgId>,      // proteins / genes
    pub compartments: Vec<CompId>,    // where it occurs in BiGG (any organism)
    pub sources: Sources,
}

pub struct Xref {
    pub ns: NsId,
    pub id: StrId,                    // "CHEBI:4170"
    pub sources: Sources,
}
```

`CURRENCY` is derived by the builder (very high reaction degree plus a curated
list). The matcher treats currency metabolites as optional, because kinetic
models routinely omit H+, H2O, Pi.

### 3.6 Reactions

```rust
pub struct Reaction {
    pub label: StrId,                 // "hexokinase (D-glucose:ATP)"
    pub xrefs: Vec<Xref>,             // bigg.reaction HEX1, rhea, ec-code, kegg, mnxr
    pub participants: Vec<Participant>,
    pub reversible: bool,             // informational; matching ignores direction
    pub enzymes: Vec<(OrgId, EntityId)>, // from BiGG gene-protein-reaction rules
    pub pathways: Vec<PathwayId>,
    pub sources: Sources,
}

pub struct Participant {
    pub entity: EntityId,
    pub coef: f32,                    // negative = consumed, positive = produced
    pub compartment: Option<CompId>,
}
```

One `Reaction` per distinct reaction across BiGG/Rhea after reconciliation
via MetaNetX (MNXR), not one per BiGG model. Which organisms have it is
recorded through `enzymes` and `Pathway::organisms`.

### 3.7 Pathways (context fingerprints)

```rust
pub struct Pathway {
    pub label: StrId,                 // "Glycolysis/Gluconeogenesis"
    pub source: Sources,              // BiGG subsystem, Reactome, BioModels cluster
    pub reactions: Vec<ReactionId>,
    pub core_entities: Vec<EntityId>, // non-currency entities, for fast overlap scoring
    pub organisms: Vec<OrgId>,
}
```

### 3.8 Names (the dictionary)

```rust
pub struct NameIndex {
    // Sorted by normalized key for binary search.
    pub keys: Vec<StrId>,
    pub postings: Vec<Vec<NameHit>>,  // parallel to keys
}

pub struct NameHit {
    pub target: u32,                  // EntityId or ReactionId
    pub kind: NameKind,               // Preferred, Synonym, Abbreviation, BiggId, ModelId
    pub sources: Sources,
    pub count: u16,                   // times seen with this meaning in curated models
}
```

`count` is the BioModels/BiGG statistic behind "named `PIP2` in 37 curated
models". Normalization (lowercase, strip punctuation and separators, Greek
letters to ASCII words, compartment suffix removed) lives in `kpack` as a
function, so the builder and the app normalize identically.

### 3.9 Name-decoder lexicon

Data, not code, so rules can be tuned without recompiling the engine.

```rust
pub struct Lexicon {
    pub rules: Vec<AffixRule>,
}

pub struct AffixRule {
    pub pattern: StrId,               // "_c$", "pp$", "^p", "_mRNA$", ":"
    pub position: AffixPos,           // Prefix, Suffix, Separator
    pub meaning: AffixMeaning,
    pub weight: f32,
}

pub enum AffixMeaning {
    Compartment(CompId),              // _c, _m, _e, _nuc
    Phosphorylated(u8),               // p, pp, _P, _PP
    ComplexSeparator,                 // ":", "_", "-" between known names
    MoleculeType(EntityKind),         // _mRNA, _gene
    Pool(PoolKind),                   // _tot, _free
}
```

### 3.10 Organisms, compartments, SBO

```rust
pub struct Organism {
    pub taxon: u32,                   // NCBI taxonomy id
    pub label: StrId,                 // "Saccharomyces cerevisiae"
    pub aliases: Vec<StrId>,          // "yeast", "S. cerevisiae"
    pub bigg_models: Vec<StrId>,      // "iMM904"
}

pub struct CompartmentDef {
    pub code: StrId,                  // BiGG code "m"
    pub go: StrId,                    // "GO:0005739"
    pub label: StrId,                 // "mitochondrion"
    pub synonyms: Vec<StrId>,         // "mito", "mitochondria", "mt"
}

pub struct SboTerm {
    pub id: u32,                      // 247 for SBO:0000247
    pub label: StrId,
    pub parents: Vec<u32>,
    pub applies_to: ElementKinds,     // species / reaction / parameter / ...
}
```

### 3.11 Splitting the pack later

v1 is a single pack. When it gets too large, split it into segments
(core, metabolism, signaling, proteins-per-organism) built together in one run
so IDs stay global; the header records each segment's ID ranges and the app
loads segments on demand.

## 4. Engine sketch (`annot-engine`)

Not the focus of this draft, but it fixes what the pack must support.

```rust
pub enum Target {
    Entity(EntityId),
    Reaction(ReactionId),
    External { ns: NsId, id: String },   // from a live lookup, not in the pack
}

pub struct Candidate {
    pub target: Target,
    pub relation: Relation,              // Is, IsVersionOf, HasPart, IsEncodedBy, ...
    pub score: f32,                      // 0..1, combined
    pub evidence: Vec<Evidence>,
}

pub struct Evidence {
    pub provider: ProviderId,
    pub weight: f32,                     // log-odds contribution
    pub message: Message,                // structured, rendered by the UI
}

pub trait EvidenceProvider {
    /// Add new candidates for an item (name decoder, dictionary, ...).
    fn propose(&self, ctx: &Context, item: ItemRef, out: &mut Candidates);
    /// Add evidence for or against existing candidates (reaction context,
    /// element balance, pathway overlap, compartment fit, ...).
    fn assess(&self, ctx: &Context, item: ItemRef, cands: &mut Candidates);
}
```

Providers, in rough build order:
1. Name decoder + dictionary (`names`, `lexicon`)
2. Entity-kind classifier (role, rate law, names)
3. Reaction matcher, tolerant of currency metabolites, direction, lumping
   (`by_participant`, `CURRENCY`)
4. Element/charge balance (`formula`, `charge`)
5. Compartment fit (`compartments`)
6. Pathway overlap (`pathways`)
7. Propagation: confirmed answers re-run providers on neighbors

Scores combine as summed log-odds, so each provider's contribution can be
shown and tuned independently.

## 5. Builder pipeline (`kpack-build`)

```
fetch     download each source into a local cache with checksums (re-runnable)
parse     one module per source -> source-specific records
reconcile map everything onto MNXM / MNXR / UniProt via MetaNetX
derive    normalized names, name counts, currency flags, pathway fingerprints,
          participant postings, xref index
validate  pattern-check every xref, report orphans and conflicts
write     postcard + zstd -> packs/kpack-<date>.bin, plus a JSON summary
```

Command shape: `kpack-build fetch`, `kpack-build build --out packs/`,
`kpack-build stats <pack>`.

Licensing: ChEBI, Rhea, MetaNetX are CC BY 4.0 (attribution in the header and
the app's About box); BioModels curated content is CC0.

BiGG (checked 2026-11-13):
- Legacy BiGG (UC San Diego, bigg.ucsd.edu, data frozen since Oct 2019):
  use, copy, modify and distribute for educational, research and non-profit
  purposes without fee, provided the copyright notice and the license
  paragraphs appear in all copies. Commercial use needs a UCSD agreement.
- Successor site bigg.bio (DTU BRIGHT Institute): new BRIGHT data is
  CC BY 4.0; legacy BiGG content keeps the UCSD license; models with their own
  embedded license keep it.
- Consequence: a free, academic, non-profit annotator may ship BiGG-derived
  data. The pack header and the About box must carry the UCSD copyright
  notice and license paragraphs, and the About box should state the tool is
  for research/non-profit use. Prefer bigg.bio as the source (newer data,
  mostly CC BY 4.0). Not legal advice; if commercial users matter, ask UCSD.

### 5.1 Status (2026-11-13)

Implemented: `kpack` (data model, name normalization, name and xref
indexes, encode/decode; builds for wasm32) and `kpack-build` with the BiGG
source.

```
cargo run --release -p kpack-build -- fetch          # into data/cache/bigg
cargo run --release -p kpack-build -- build          # -> packs/kpack.bin
cargo run --release -p kpack-build -- stats packs/kpack.bin
cargo run --release -p kpack-build -- lookup packs/kpack.bin ATP
cargo run --release -p kpack-build -- lookup packs/kpack.bin chebi:CHEBI:15422
```

BiGG source notes:
- Data comes from bigg.bio's JSON bulk downloads
  (`/api/v3/download/metabolites`, `/api/v3/download/reactions`). The legacy
  bigg.ucsd.edu files now redirect there.
- bigg.bio currently has 2,441 universal metabolites and 7,322 universal
  reactions, fewer than legacy BiGG had. The pack is about 1.3 MB.
- Metabolite rows are per charge form (`atp:0`, `atp:-4`); each form has its
  own ChEBI id. They are merged into one entity, and each xref records the
  charge of the form it denotes (`Xref::form_charge`).
- BiGG has no synonyms of its own; ChEBI supplies them (below).
- Compartment codes and names come from bigg.bio/api/v3/compartments. Note
  `w` there means "wildtype staph aureus", not cell wall.
- `LEGACY_NOTICE` in `sources/bigg.rs` must be replaced with the verbatim
  UCSD license text before a pack is published.

ChEBI source notes (`sources/chebi.rs`, release 257, CC BY 4.0):
- Files: the TSV flat files at ftp.ebi.ac.uk/pub/databases/chebi/flat_files
  (compounds, names, chemical_data, secondary_ids, relation, README for the
  release number). Columns are read by header name.
- ChEBI adds no entities. It enriches BiGG's: from each ChEBI id BiGG gives,
  it follows conjugate acid/base and tautomer links (up to 3 steps, each id
  to the nearest BiGG entity), adds those ids with their charges, adds
  ChEBI's secondary (merged) ids marked `Xref::secondary`, and adds the
  names.
- Names kept: English SYNONYM, IUPAC NAME, UNIPROT NAME and INN rows with
  status CHECKED or OK. Brand names and unreviewed submissions are dropped.
- "is a" links (`Xref::scope`):
  - Up to 2 steps up, a ChEBI class becomes a `Broader` xref of the entity
    (CHEBI:17634 D-glucose and CHEBI:17234 glucose for BiGG's
    D-glucopyranose). `Pack::find_xref` ignores these (identity only);
    `Pack::find_xref_related` returns them with their scope.
  - A class covering more than 10 entities is dropped as too broad
    ("organophosphate oxoanion" covered 225).
  - One step down, a child becomes a `Narrower` xref.
  - Where the parent is itself an entity, the link is
    `Entity::generalizes_to` instead, and the climb stops there.
  - Broader and narrower ids bring no names ("hexose" is not a name for
    glucose).
- The TSV reader handles quoted fields: ChEBI's compounds.tsv has 7
  definitions that run over several lines, which a line-based reader
  misreads. Row counts match Python's csv module on all five files.
- Result on the full data: all 2,441 entities enriched, all 4,215 BiGG
  ChEBI ids known to ChEBI, 44 extra charge forms, 5,262 secondary ids,
  26,460 names, 6,448 broader and 754 narrower ids, 52 entity-to-entity
  links, 281 classes dropped as too broad. Pack 1.8 MB. Format version 3
  (`Xref::secondary` in 2, `Xref::scope` in 3).
- Gap, for later: stereo prefixes. "fructose 1,6-bisphosphate" does not
  match ChEBI's "D-fructose 1,6-bisphosphate" by ChEBI names alone. (It now
  matches through BioModels, below, but the decoder should still handle it.)

BioModels source notes (`sources/biomodels.rs`, CC0):
- Fetch: the search API (`www.biomodels.org/search?query=curationstatus:
  "Manually curated"`) lists 1,096 curated models, 1,075 in SBML. Each is
  downloaded through `/model/files/{id}` and `/model/download/{id}`
  (~180 MB, ~10 minutes; resumable). Use `www.biomodels.org` directly: the
  ebi.ac.uk and bare biomodels.org addresses redirect and mangle quoted
  queries. Some file names contain spaces and must be escaped. There is no
  current bulk archive (the newest is r31, 2017).
- Mining: every species and reaction with `bqbiol:is` or `isVersionOf`
  terms, read with `sbml-io`. Resource URIs are taken apart by
  `kpack::split_resource` (identifiers.org path and compact forms, MIRIAM
  URNs, ChEBI and KEGG web links) and resolved by `namespaces::resolve`.
- Each resource votes for the targets it leads to; exact ids win over
  broader or narrower ones, and the target with most votes wins. A tie is
  ambiguous and skipped (mostly reactions annotated only with an EC number).
- Noise rule (`keep_name`): a (name, target) pair needs at least 20% of the
  models that annotate that name at all (resolved or not), and either 2
  models or a majority. This removes generic ids like "s1" and "x".
- `NameHit::count` is the number of models; `Sources::BIOMODELS` marks it.
- Benchmark split: `is_test_model` puts every fifth model by id number in
  the test set; `build --exclude-test-models` leaves them out (860 of 1,075
  models mined).
- Result on the full data: 1,075 models, 53,540 annotated elements, 17,821
  resolved to the pack, 3,180 ambiguous (2,784 reactions), 32,539 outside
  the pack (proteins, Reactome, etc.); 18,194 names kept, 841 dropped as
  noise. Pack 2.0 MB. No pack format change.

## 5.2 The model being annotated (`annot-model`)

Status (2026-11-14): implemented; builds for WebAssembly.

- `load` reads SBML (Level 2 V1-5, Level 3 V1-2) or Antimony (converted to
  SBML L3V2 through `antimony-rs`); `Workspace::to_sbml` writes it back in
  the level and version it was read in.
- `Workspace::engine_model` is the engine's view. `Workspace::resolution`
  says what an element is already annotated as in the pack, by the same
  voting rule the BioModels source and the benchmark use (the benchmark now
  reads models through `annot-model`).
- `Pack::resolve_resource` / `find_resource` read annotation URIs in every
  form models use, from the pack alone (no `kpack-build`), so the browser
  app can too. This needed `Namespace::id_prefix` (pack format version 4).
- Edits: `accept` (one undoable step: `bqbiol:is` terms from `uris_for`,
  plus an SBO term if the element has none: 247 molecule, 176 reaction,
  185 transport), `add_terms`, `remove_term`, `set_sbo`, `undo`, `redo`.
  Accepted URIs are compact identifiers.org form
  (`https://identifiers.org/CHEBI:15422`): one ChEBI id (the neutral form
  when known) and the KEGG compound for a molecule; Rhea, EC and KEGG
  reaction ids for a reaction; never secondary or broader ids.
- Safety: an edit changes only the element's MIRIAM terms. Other tools'
  annotation XML, history, notes and RDF descriptions the reader does not
  understand are written back unchanged; a new `metaid` is made unique; an
  annotation whose RDF block cannot be read at all is not edited
  (`EditError::UnreadableRdf`).
- Known limitation: MIRIAM terms inside an RDF description that also holds
  other statements are preserved but not seen by `terms` /
  `identity_uris`, so such an element looks unannotated to the annotator.
- Not yet: writing annotations back into Antimony text (an Antimony model
  is exported as SBML), and SBML -> Antimony.

## 5.3 The browser app (`annot-app`)

Status (2026-11-14): first working version, checked in Chrome on the
built-in example.

```
cd crates/annot-app
trunk serve --open            # needs packs/kpack.bin at the repository root
```

- Leptos 0.8 (client-side rendering), built with Trunk. Leptos 0.8.3's
  macros break with `server_fn_macro` 0.8.12 (it moved to syn 3), so
  `Cargo.lock` pins `server_fn_macro` and `server_fn_macro_default` to
  0.8.3; keep the pin until Leptos is upgraded.
- The pack is copied next to the app by Trunk and fetched on start
  (~2 MB; the WebAssembly is ~1.7 MB).
- Screens: start (drop or choose an SBML/Antimony file, or the built-in
  glycolysis example, `assets/glycolysis-example.ant`); overview (progress
  bars for molecules and reactions by status, "accept all high-confidence");
  Molecules / Reactions tabs with filters (all, to review, identified, no
  suggestion); each row shows the status, best suggestion and confidence
  with Accept; expanding shows the current annotation and up to five
  candidates with their evidence; reactions show their equation with
  molecule chips coloured by status (a chip jumps to that molecule);
  undo / redo; save annotated SBML (download); close.
- State lives in `session.rs` (no UI code): statuses are cached and
  recomputed after each edit.
- Checking a suggestion: every suggested (and identified) name links to its
  main database entry, and the details list each database id as a link
  ("ChEBI 16084", "KEGG Compound C00085"), all opening in a new tab. The
  links are the identifiers.org URIs that accepting writes, so what people
  check is exactly what gets saved.
- Every row has an explicit "Details / Hide" control (the whole row also
  toggles, but nothing showed that).
- Accepting from the details panel shows its result in the panel: the
  accepted candidate is highlighted and marked "Accepted", the current
  annotation appears above, and the other candidates offer "Use this
  instead", which replaces the identification in one undoable step
  (`Workspace::replace_identity`: removes the `is` / `isVersionOf` terms
  the pack knows, keeps the rest). Before this, a second Accept added a
  second identity beside the first, and the only feedback was the message
  at the bottom of the page.
- Synonyms: each suggestion shows its most familiar other name in the row
  ("D-glucopyranose, also glucose") and all of them in the details ("Also
  known as: glucose; D-glucose; Grape sugar; Dextrose; ..."). They come
  from `Entity::synonyms` / `Reaction::synonyms` in the pack (format
  version 5), chosen by `builder::pick_synonyms`: database names (ChEBI,
  BiGG) first; names found only in models must appear in at least two
  and be plain (no brackets, location words or glued location endings);
  no ids, charge forms, chemical formulas or names over 40 characters;
  full names before abbreviations, then by model usage and readability.
  ChEBI names are now stripped of the HTML its "ASCII name" column still
  carries.
- Molecule chips in a reaction's equation: hovering (or tabbing to) a
  chip shows a card with the molecule's name, id, compartment, status,
  first synonyms, a database link, Accept (when it has a suggestion) and
  "Show in Molecules list"; clicking a chip pins its card open (touch
  screens). Checking a reaction's molecules no longer leaves the reaction.
  If the user does jump to the Molecules list, a sticky "Back to reaction
  ..." button returns to the reaction, opened and scrolled into view.
- Serving `dist/` with another web server while running `trunk build`
  fails on Windows (the folder is in use); `trunk serve` avoids it.

Found while testing in the browser, and fixed in the engine:
- Compartment and organism variants of one BiGG reaction (PFK, PFKg)
  appeared as separate candidates, and ties went to the obscure one.
  Context now keeps one reaction per chemistry and prefers the plainest
  BiGG id.
- Matched cofactors did not count in a reaction's weight, so the
  phosphatase tied with the kinase. They now count a little.
Both raised novel-model reactions from 82.2% to 82.7% right first.

Learning from the user (2026-11-14):
- After every edit the session re-runs the engine with what is confirmed:
  `Species::known` and `Reaction::known` from the model's annotations
  (accepted or pre-existing). When a reaction's ids fit several BiGG
  compartment variants equally, the plainest BiGG id stands for them
  (status: "phosphoglycerate kinase (and 4 compartment variants)").
- A confirmed reaction pins down its unconfirmed molecules
  (`context::pinned_by_reactions`): the database reaction is lined up
  with the model's using the molecules already known, and what is left on
  a molecule's side is evidence for it; one left over, with every other
  reactant and product confirmed, is complete (High). In the example,
  confirming vPGK as phosphoglycerate kinase takes s9 from uncertain to a
  High 3-phosphoglycerate.
- Benchmark scenario `bench --confirmed-reactions`: every reaction given;
  species right first 91.0% on novel models (90.8% without), 74.6% of
  suggestions High (70.7%), High still right 99.3%. Across all test models
  species right first dips 91.9% -> 91.6%: a confirmed reaction pushes
  BiGG's form of a molecule (D-glucopyranose 6-phosphate) where curators
  often use another (alpha-D-glucose 6-phosphate).
- Reject: a rejected suggestion is removed after every provider (so no
  later provider reasons from it) and inside context's own rounds
  (`Species::rejected`, `Reaction::rejected`). Rejections live in the
  session, not the SBML; Details lists them with Restore. Buttons in the
  row, on each candidate and in the molecule chip card.

Renaming SBML ids (2026-11-14), `annot_model::Workspace::rename`:
- Species, reactions and compartments. In the app: Details shows "SBML
  id: s9 [Rename]"; the field opens focused with the id selected; Enter
  renames, Escape cancels; a refusal is explained in place.
- The new id must be a valid SId, different, and unused anywhere in the
  model (any `id="..."` in the written document, so parameters, unit
  definitions, layout glyphs and local parameters all count).
- On a copy of the document, every reference is renamed by walking
  sbml-io's data: species and modifier references, compartment
  references, conversion factors, initial assignment symbols, rule and
  event assignment variables, all MathML (kinetic laws, rules, initial
  assignments, constraints, stoichiometry math, event triggers, delays,
  priorities, assignments), and the Layout package (compartment, species,
  reaction, text, general and reference glyphs). Function definitions are
  left alone: SBML does not let them refer to model ids, and an argument
  of the same name shadows the id.
- Refused when a rate law has a local parameter with the old or new id
  (ambiguous), and when, after renaming, the written copy still refers to
  the old id (an attribute other than `name` / `metaid`, or a `<ci>`
  outside function definitions): those are references in raw XML that
  sbml-io does not take apart (other packages such as FBC objectives,
  CellDesigner annotations, raw MathML). Better refused than half done.
- One undoable step (`Change::Document`, a whole-document before/after).
  The session rebuilds the engine's view after every edit, undo and redo.
- Checked on all 1,075 cached curated BioModels (renaming the first
  species and the first reaction of each): 996 species and 940 reactions
  renamed, none with a reference left behind, none gaining a validation
  issue, undo restoring every model exactly; 51 species and 25 reactions
  refused, all for references in raw XML (mostly CellDesigner
  annotations: `species="s121"`, `reaction="re44"`, `sbmlid="pgi"`).
- Not yet: renaming parameters; updating references inside other
  packages' raw XML (it could be done for well-known ones such as FBC);
  renaming in Antimony text (an Antimony model is exported as SBML).

Known issues / next (in order):
- Done (2026-11-14): "Accept all" is now one undo step (`Workspace::batch`
  groups edits into one `Change::Group`).
- Done (2026-11-14): readable labels (`kpack-build/src/labels.rs`, run in
  `PackBuilder::finish`). Molecules lose charge states ("L-leucine
  zwitterion" -> "L-leucine", 145 labels). Reactions: SBML escapes undone
  ("R_glucose_transport__uniport_" -> "glucose transport (uniport)"),
  stray "_" and final "." dropped, charges off "Exchange of X(2-)."
  (about 2,000 labels). The 563 labelled only by an id take a synonym, else
  the name of a reaction with the same chemistry (shared Rhea, MetaNetX,
  MetaCyc, KEGG or SEED id; compartment qualifier dropped; PGKf ->
  "phosphoglycerate kinase", 99 labels), else what they do ("fumarate
  transport", "Exchange of X", "A + B = C"; transports and exchanges
  always this way). Synonyms that are only a charge state ("NADH
  dianion") are no longer offered. Only labels change; names the matcher
  looks up are as the sources gave them, and the benchmark is unchanged.
- Some molecule labels are chemists' names for the charged form
  ("3-phosphonato-D-glycerate", "hydrogenphosphate"); the familiar name is
  shown as a synonym. Preferring a familiar label is a possible next step.
- A molecule card opened near the bottom of the page needed extra space
  below the list (now padded); a card that opens upward when there is no
  room below would be better.
- No search for an element the engine has nothing for (OLS / UniProt),
  no keyboard shortcuts, no SBO / qualifier editing in the UI yet.
- Publishing to GitHub Pages needs a CI job that builds the pack (BiGG,
  ChEBI and BioModels fetch, ~10 minutes) or a pack published as a release
  asset.

## 6. Benchmark (`annot-cli bench`)

Status (2026-11-14): implemented.

```
cargo run --release -p kpack-build -- build --exclude-test-models --out packs/kpack-train.bin
cargo run --release -p annot-cli -- bench          # test set, writes bench/latest.md
```

- Test set: every fifth curated BioModels model by id number (215 of
  1,075), `biomodels::is_test_model`. The pack used must be built with
  `--exclude-test-models`; the harness refuses one that is not (unless
  `--allow-leak`).
- The engine sees `annot_engine::Model` only (names, ids, compartments,
  reaction participants), never annotations. The answer key for each
  element comes from its `bqbiol:is` / `isVersionOf` resources, resolved
  against the pack by the same voting as the BioModels source; on a tie,
  any tied target is accepted.
- Graded: elements annotated with something in the pack. Elements
  annotated with things outside a metabolism pack (proteins, Reactome) are
  counted but not graded.
- Metrics, for species and reactions: coverage, top-1/3/5, precision of the
  first suggestion, mean reciprocal rank, and how often a lone suggestion is
  right. Misses are listed in the report by kind (no suggestion, wrong, right
  but ranked lower).
- **Novel models.** About a quarter of the test models have a near-twin in
  the training set (species-name Jaccard >= 0.8). The report also scores the
  "novel" test models only (overlap < 0.5 with every training model), the
  fairer estimate for a model the tool has never seen.
- Each run appends a row to `bench/history.tsv`.

Baseline (names provider only, exact lookup), 2026-11-14:

| | Species | Reactions |
|---|---|---|
| Graded (all test models) | 1,267 | 641 |
| Right first, all 215 test models | 86.7% | 33.1% |
| Right first, 133 novel test models | 85.1% | 6.9% |
| First suggestion right, when there is one | 96.7% | 95.9% |

Reading: species names generalize; reaction names do not (the 33% came
from near-twin models). Most species misses are names wrapped in
compartment words ("Glucose in", "External Pyruvate", "ATP_mit",
"O2_ex"): the name decoder's job. Wrong answers are cryptic abbreviations
("T", "D", "AA"): reaction context's job.

With the name decoder (`annot_engine::decode`, provider `decoder`),
2026-11-14:

| Right first | Species, all | Reactions, all | Species, novel | Reactions, novel |
|---|---|---|---|---|
| names | 86.7% | 33.1% | 85.1% | 6.9% |
| names + decoder | 91.0% | 49.3% | 88.7% | 32.7% |

Precision of the first suggestion stayed at 96.7% (species) and 96.6%
(reactions). The decoder runs only on elements the names provider found
nothing for. Its rules, each with a trust factor: SBML prefixes (`M_`,
`R_`), location words at either end (built-in list, the pack's
compartments, the model's own compartment ids and names; single-letter
codes in ids only), location endings joined to a word ("Glcin"), bare
charges ("Ca2+"), brackets, D-/L- prefixes, and single words of short
names (not generic ones like "phosphate"). It also looks up database ids
embedded in SBML ids (KEGG `C00031` / `R00299`, ModelSEED, MetaNetX,
ChEBI), which is what lifted reactions: many models name reactions by
their KEGG id.

Remaining species misses are mostly cryptic abbreviations ("VK", "oro",
"dho", "cp", "DOPAC") and classes ("ROS", "Bioamines"); remaining
reaction misses are reactions with no usable name or id. Both are for
reaction context next.

With reaction context (provider `context`), 2026-11-14:

| Right first | Species, all | Reactions, all | Species, novel | Reactions, novel |
|---|---|---|---|---|
| names | 86.7% | 33.1% | 85.1% | 6.9% |
| names + decoder | 91.0% | 49.3% | 88.7% | 32.7% |
| names + decoder + context | 91.9% | 81.7% | 90.8% | 82.2% |

Precision of the first suggestion: species 95.4% (was 96.7%), reactions
91.0% (was 96.6%): context answers far more, and is right less often than
a direct name match. Counting the same chemistry under another BiGG id as
right changes little (reactions 82.7% on novel models), so the remaining
errors are real.

How it works (`providers/context.rs`):
- Anchors: species whose best candidate scores >= 1.4 and leads the next
  by a margin (second <= 0.75 x best).
- A model reaction is compared with the pack reactions that contain its
  anchored non-currency species, in both directions, ignoring
  compartments. Currency metabolites are optional; each one the pack
  reaction needs but the model does not show costs a little (this tells
  "G6P -> X" the isomerase from hexokinase run backwards). Every anchored
  non-currency species must match (one may be missing if three match).
- Transport reactions are considered only when the model reaction's
  species span more than one compartment. Without this rule, "IMP
  transport" fit any reaction with one anchored IMP perfectly and was the
  main source of wrong reactions.
- Species: one non-currency leftover on the species' side is strong
  evidence, two are weaker, a lone currency leftover weak. Evidence from
  several reactions adds up (capped).
- Two rounds: species found in round 1 (score >= 0.9, clear lead) anchor
  round 2, so identifications spread along a pathway.

Remaining reaction errors are mostly reactions with only one anchored
species, where several enzymes act on it (nucleotide metabolism:
nucleotidases vs kinases vs synthases).

### Confidence

`annot_engine::confidence` gives the first suggestion a level the app can
show. It rests on what the suggestion is based on
(`annot_engine::basis`: a direct name match, a decoded name or embedded
id, or context only), its score, and its lead over the runner-up:

- High: a name match with no competitor scoring >= 2.0, or a decoded match
  scoring >= 2.0 with a clear lead (runner-up <= 0.75 x best).
- Likely: other name matches >= 1.5, decoded >= 1.0, context >= 1.5.
- Uncertain: the rest.

The benchmark reports how often each level is right (`bench --observations
FILE` also dumps every graded first suggestion for re-calibration).
2026-11-14:

| Models | Level | Species: share | Species: right | Reactions: share | Reactions: right |
|---|---|---|---|---|---|
| all test | high | 69.2% | 99.4% | 24.0% | 99.3% |
| all test | likely | 27.0% | 90.9% | 70.3% | 91.6% |
| all test | uncertain | 3.8% | 54.3% | 5.7% | 48.5% |
| novel | high | 70.8% | 99.3% | 28.4% | 100.0% |
| novel | likely | 24.4% | 88.9% | 64.5% | 89.8% |
| novel | uncertain | 4.8% | 42.9% | 7.1% | 61.5% |

Caveat: the thresholds were read off this same test set (coarse, round
cut-offs, not fitted). A fresh check needs models outside BioModels' 1,075,
or a three-way split once there are more.

For the app: High can be offered for one-click acceptance (or bulk
"accept all high"), Likely shown for a quick check, Uncertain flagged.

### Confirmed molecules and complete reaction matches (2026-11-14)

- `annot_engine::Species::known`: a species' confirmed identity (accepted
  by the user, or already annotated in the model). Context treats it as a
  certain anchor. The app sets it and re-runs the engine after every edit;
  the benchmark leaves it empty (its answers come from the annotations).
- A reaction whose reactants and products are all *confirmed* (not just
  guessed) and match exactly one database reaction (all on the right
  sides, nothing missing, at most two unshown cofactors such as H+ and
  water) gets evidence marked `complete`, and is High. Using guessed
  molecules instead was tried first: High reactions were then right only
  95.5% of the time, since one wrong guess makes a "complete" match
  complete for the wrong molecules.
- New benchmark scenario `bench --confirmed-species`: every species is
  given its true identity, as if the user had confirmed them all.
  Reactions right first: 84.9% on novel models (82.7% without).
- High reactions in that scenario: right 96.0% strictly, 99.3% (273 of
  275 on novel models) when only answer keys that name a specific
  reaction are counted. The rest rest on EC numbers alone: curators wrote
  a KEGG reaction id the pack does not know plus EC numbers, so the key
  falls back to a generic reaction ("hexokinase" for EC 2.7.1.1) while the
  engine finds the exact one (alpha-glucokinase for "ATP + alpha-D-glucose
  -> ADP + alpha-D-glucose 6-phosphate", KEGG R01786). The report shows
  both figures; `Graded::reaction_key_coarse` marks such keys.
- In the example model, confirming the molecules lifts 5 of 8 reactions to
  High. The others stay Likely for real reasons: an unconfirmed molecule
  (s9), several transporters for glucose, and G6P accepted as
  alpha-D-glucose 6-phosphate while BiGG's hexokinase makes D-glucose
  6-phosphate.

Original plan:

- Take curated BioModels, strip their annotations, run the engine, and compare
  with the originals: precision, recall, rank of the correct answer, and how
  many confirmations were needed.
- **Avoid leakage:** split BioModels into train/test by model ID. Models in the
  test set must be excluded from the name statistics when building the pack
  used for benchmarking.
- Track results per release in `bench/`.

## 7. Library findings (antimony-rs, checked 2026-11-13)

Probe: an Antimony model with CV terms, SBO terms, notes, creators and dates
on every kind of item, converted with `antimony::sbml::write_sbml`.

Antimony -> Antimony (`write_document`): everything kept.

Antimony -> SBML:
- Kept: CV terms on model, compartments, species, reactions, parameters,
  events and unit definitions; notes; creators; created/modified dates.
- **Dropped: every `sboTerm`** (model, compartment, species, reaction,
  parameter, event, function). FIXED 2026-11-13 in both copies; see
  `reports/antimony-sbml-annotation-gap.md`.
- **Dropped: CV terms on function definitions.**
- Quirk: writing `m identity "..."` with the model's own name `m` as the
  subject annotates the model correctly but also creates a stray
  `<parameter id="m">`. Writing `model identity "..."` avoids it.
- `messages::annotation_dropped` exists but is never called, so these drops
  are silent.

Also: antimony-rs has no SBML -> Antimony converter. The annotator's Antimony
view of an SBML model needs one (or must work from SBML only).

## 8. Open questions

1. Function-definition CV terms, silent drops and the stray model-name
   parameter are still open (reported to PathwayDesigner).
2. Whether to write an SBML -> Antimony converter, or keep the Antimony view
   for models that start as Antimony.

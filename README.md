# PathwayAnnotator

## AI Assistance

This software was developed with the assistance of Anthropic's Claude, an AI model, through Claude Code. Stop here if you aren't comfortable with AI assisted code generation.

## Introduction

An SBML annotator for biologists and modelers, written in Rust. It will
run entirely in the browser (WebAssembly on GitHub Pages): load a model,
and it suggests what each species and reaction is (ChEBI, KEGG, Rhea, BiGG,
EC, ...) with the evidence for each suggestion, for you to confirm.

The suggestions come from curated data shipped with the app as a
**knowledge pack**, not from an online AI service: no accounts, no API
keys, and models never leave your machine. Only search text you choose
to send leaves it: pressing Search under "Search other databases" sends
that text (a name, or ChEBI ids and EC numbers) to EBI, UniProt or Rhea.

Status: the knowledge pack, the suggestion engine, the accuracy
benchmark and the web app work. The app is published from `main` at
https://sys-bio.github.io/PathwayAnnotator/ by the workflow in
`.github/workflows/pages.yml` (once GitHub Pages is switched on for the
repository). See [DESIGN.md](DESIGN.md) for the design, data sources and
results.

You can find a Zenodo technical report at https://zenodo.org/records/24237183
(DOI: [10.5281/zenodo.24237183](https://doi.org/10.5281/zenodo.24237183)). To cite PathwayAnnotator:
Sauro, H. M. (2026). PathwayAnnotator: A Web-based SBML Annotation Tool for Biochemical Networks.
Zenodo. https://doi.org/10.5281/zenodo.24237183

This work was supported by NIH Biomedical Imaging and Bioengineering award P41 EB023912 through HMS at the Center for Reproducible Biomedical Modeling reproduciblebiomodels.org

## How it suggests

- **Names.** A dictionary of names from BiGG, ChEBI and curated BioModels
  ("G6P" is glucose 6-phosphate in 17 curated models).
- **Name decoding.** Reads names as modelers write them: "Glucose in",
  "ATP_mit", "M_g6p_c", "Ca2+", database ids inside SBML ids.
- **Reaction context.** Known species identify their reactions, and a
  reaction's leftover participant identifies an unknown species:
  "Glc + ATP -> S3 + ADP" makes S3 glucose 6-phosphate.

On curated BioModels the engine has never seen, it puts the right answer
first for about 91% of species and 82% of reactions (metabolism). Each
suggestion carries a confidence level; "high" ones were right about 99% of
the time.

## Supported by:

This work was supported by NIH Biomedical Imaging and Bioengineering award P41 EB023912 through HMS at the Center for Reproducible Biomedical Modeling reproduciblebiomodels.org

## Layout

| Crate | What it is |
|---|---|
| `kpack` | The knowledge pack: data model, name and id indexes, encode/decode (WebAssembly) |
| `kpack-build` | Builds the pack from BiGG, ChEBI and BioModels (native) |
| `annot-engine` | The suggestion engine, its providers and confidence levels (WebAssembly) |
| `annot-model` | Loads SBML/Antimony, applies accepted annotations with undo, writes SBML (WebAssembly) |
| `annot-online` | Searches ChEBI (via OLS), UniProt and Rhea for what the pack does not know (WebAssembly; the app fetches) |
| `annot-cli` | The benchmark, later batch annotation (native) |
| `annot-app` | The browser app (Leptos, built with Trunk) |
| `sbml-io`, `antimony` | SBML and Antimony reading and writing |

## Building the pack and running the benchmark

Requires a recent stable Rust.

```
cargo run --release -p kpack-build -- fetch        # downloads BiGG, ChEBI, BioModels (~200 MB, ~10 min)
cargo run --release -p kpack-build -- build        # packs/kpack.bin
cargo run --release -p kpack-build -- lookup packs/kpack.bin G6P

cargo run --release -p kpack-build -- build --exclude-test-models --out packs/kpack-train.bin
cargo run --release -p annot-cli -- bench          # writes bench/latest.md
```

To run the app locally (needs [Trunk](https://trunkrs.dev) and the
`wasm32-unknown-unknown` target, and `packs/kpack.bin` built as above):

```
cd crates/annot-app
trunk serve --open
```

`cargo test --workspace` runs the tests; they use small excerpts in
`crates/kpack-build/tests/fixtures` and need no downloads.

## Publishing

Every push to `main` runs `.github/workflows/pages.yml`: it builds the
knowledge pack from `data/snapshot` and the app for the
`/PathwayAnnotator/` path, and deploys them to GitHub Pages. The build
downloads nothing: `data/snapshot` (42 MB) holds BiGG, ChEBI (pinned to
release 257, `RELEASE` in `crates/kpack-build/src/sources/chebi.rs`) and a
digest of the curated BioModels models (their species' and reactions' names
and annotations, all the pack takes from them; BioModels refuses downloads
from GitHub's build machines). Every deploy uses the same data until the
snapshot is refreshed; `data/snapshot/README.md` says how, with the data
licenses. `kpack-build build` uses the digest whenever no downloaded
BioModels models are in its cache. The first time, set the repository's
Settings > Pages > Build and deployment > Source to "GitHub Actions".

## Data and licenses

The code is licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option.

The knowledge pack is built from:

- [BiGG](https://bigg.bio): CC BY 4.0 (BRIGHT data) and the BiGG Models
  license for legacy UC San Diego content (educational, research and
  non-profit use; https://bigg.ucsd.edu/license). Its copyright notice and
  license paragraphs travel with every pack, verbatim, and the app shows
  them under About.
- [ChEBI](https://www.ebi.ac.uk/chebi): CC BY 4.0.
- [BioModels](https://www.biomodels.org): CC0.

Each pack records its sources, versions and required notices in its
header.

When the user asks (Details, "Search other databases"), the app also
searches [ChEBI](https://www.ebi.ac.uk/chebi) through EBI's
[Ontology Lookup Service](https://www.ebi.ac.uk/ols4),
[UniProt](https://www.uniprot.org), [Rhea](https://www.rhea-db.org)
(all CC BY 4.0) and the [Gene Ontology](https://geneontology.org)
(CC BY 4.0). The search text is sent to those services; nothing is
sent otherwise.

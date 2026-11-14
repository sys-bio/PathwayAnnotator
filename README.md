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
benchmark and a first version of the web app work; the app is not
published yet. See [DESIGN.md](DESIGN.md) for the design, data sources and
results.

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

## Data and licenses

The code is licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option.

The knowledge pack is built from:

- [BiGG](https://bigg.bio): CC BY 4.0 (BRIGHT data) and the BiGG Models
  license for legacy UC San Diego content (educational, research and
  non-profit use; https://bigg.ucsd.edu/license).
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

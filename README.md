# PathwayAnnotator

An SBML annotator for biologists and modelers, written in Rust. It will
run entirely in the browser (WebAssembly on GitHub Pages): load a model,
and it suggests what each species and reaction is (ChEBI, KEGG, Rhea, BiGG,
EC, ...) with the evidence for each suggestion, for you to confirm.

The suggestions come from curated data shipped with the app as a
**knowledge pack**, not from an online AI service: no accounts, no API
keys, and models never leave your machine.

Status: the knowledge pack, the suggestion engine and an accuracy
benchmark work; the web app is not built yet. See [DESIGN.md](DESIGN.md)
for the design, data sources and results.

## How it suggests

- **Names.** A dictionary of names from BiGG, ChEBI and curated BioModels
  ("G6P" is glucose 6-phosphate in 17 curated models).
- **Name decoding.** Reads names as modelers write them: "Glucose in",
  "ATP_mit", "M_g6p_c", "Ca2+", database ids inside SBML ids.
- **Reaction context.** Known species identify their reactions, and a
  reaction's leftover participant identifies an unknown species:
  "Glc + ATP -> S3 + ADP" makes S3 glucose 6-phosphate.

On curated BioModels the engine has never seen, it puts the right answer
first for about 91% of species and 82% of reactions (metabolism).

## Layout

| Crate | What it is |
|---|---|
| `kpack` | The knowledge pack: data model, name and id indexes, encode/decode (WebAssembly) |
| `kpack-build` | Builds the pack from BiGG, ChEBI and BioModels (native) |
| `annot-engine` | The suggestion engine and its providers (WebAssembly) |
| `annot-cli` | The benchmark, later batch annotation (native) |
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

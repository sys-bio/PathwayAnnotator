# antimony-rs: annotations lost when converting Antimony to SBML

Date: 2026-11-13
Crate: `antimony-rs` (`crates/antimony`), SBML output (`sbml` feature)
Found in: PathwayDesigner at commit `907f6ff`
Found by: the SBML Annotator project, while checking which annotations survive
an Antimony -> SBML conversion

## Summary

`antimony::sbml::write_sbml` / `to_sbml` silently drops some annotations that
the Antimony parser reads correctly. The Antimony -> Antimony round trip
(`write_document`) keeps everything; only the SBML output loses data.

| Problem | Status |
|---|---|
| 1. Every `sboTerm` is dropped (model, compartment, species, reaction, parameter, event, function) | **Fixed** (uncommitted, see below) |
| 2. CV terms (`identity`, ...) on function definitions are dropped | Open |
| 3. The drops are silent: `messages::annotation_dropped` exists but is never called | Open |
| 4. `m identity "..."`, using the model's own name, also creates a stray `<parameter id="m">` | Open (minor) |

## How it was found

An Antimony model with CV terms, SBO terms, notes, creators and dates on every
kind of item was converted with `write_sbml`, and the output was inspected.

Kept correctly in SBML:
- CV terms on the model, compartments, species, reactions, parameters, events
  and unit definitions
- notes (model and species)
- creators, created and modified dates (model)

Dropped:
- all `sboTerm` values (problem 1)
- `mm identity "..."` on function definition `mm` (problem 2)

## Problem 1: SBO terms dropped (fixed)

### Reproduce

```
model *m()
  compartment cyt = 1;
  species Glc in cyt = 1;
  Glc.sboTerm = 247
  cyt.sboTerm = 290
  model.sboTerm = 4
end
```

Before the fix the SBML had no `sboTerm` attribute anywhere. The parser had
stored the values (`Symbol::sbo_term`, `Module::sbo_term`), and
`antimony::write_document` wrote them back correctly.

### Cause

In `crates/antimony/src/sbml.rs`, `meta()` builds the `sbml_io::Meta` for each
element but never set `Meta::sbo_term`. In addition, function definitions were
given no `meta` at all. `sbml-io` already writes `sboTerm` for any element
whose `Meta::sbo_term` is set, so no `sbml-io` change was needed.

Only the hard-coded SBO terms survived: `SBO:0000410` on the implicit default
compartment and the modifier terms (20 / 459 / 19) from interactions.

### Fix

`crates/antimony/src/sbml.rs`:
- new helper `sbo_text(Option<u32>) -> Option<String>` (247 -> `SBO:0000247`)
- `meta()` takes an `sbo: Option<u32>` argument and sets `sbo_term`, including
  on elements that have no other annotation (an SBO term is an attribute and
  needs no metaid)
- `symbol_meta()` passes `symbol.sbo_term` (covers compartments, species,
  parameters, reactions, events)
- the model passes `Module::sbo_term`
- unit definitions pass `None` (`UnitDef` has no SBO field)
- function definitions get a `meta` with the SBO term of the symbol named
  after the function (`mm.sboTerm = 28` is stored on that symbol)

`crates/antimony/tests/sbml.rs`: new test `sbo_terms_reach_the_sbml` checks
all seven element kinds, that an element without an SBO term gets none, and
that the output validates and reads back.

### Status

- Applied to both copies: `PathwayDesigner/crates/antimony` and
  `Annotator/crates/antimony` (the files are identical).
- PathwayDesigner: `cargo test --workspace --all-features` passes (39 test
  binaries, 0 failures). The change is **not committed**: 2 files,
  55 insertions, 4 deletions.

## Problem 2: CV terms on function definitions dropped (open)

```
function mm(x)
  x/(1+x)
end
model *m()
  mm identity "http://identifiers.org/sbo/SBO:0000028"
end
```

The CV term is parsed onto the symbol `mm` and written back by
`write_document`, but `<functionDefinition id="mm">` has no annotation.

Suggested fix: in `function_definitions()`, use the symbol's full meta, i.e.
`self.module.symbol(&f.name).map(symbol_meta).unwrap_or_default()` (the same
pattern `annotation_meta` uses for reactions and events), instead of the
SBO-only meta added for problem 1. Notes on functions would then carry over
too. Add a test alongside `sbo_terms_reach_the_sbml`.

## Problem 3: drops are silent (open)

`messages::annotation_dropped(what)` ("annotations on {what} are not written
to SBML") is defined but has no callers. Anything the converter cannot carry
over should produce a warning diagnostic so users know. After problems 1 and 2
are fixed, check what else can still be dropped (for example annotations in
`Module::unparsed_sections`, which are kept for Antimony output only) and warn
for those.

## Problem 4: stray parameter for the model's name (open, minor)

```
model *m()
  S1 = 1;
  m identity "http://identifiers.org/biomodels.db/MODEL0001"
end
```

The CV term is correctly attached to the model (and written back as
`model identity "..."`), but the SBML also gains `<parameter id="m"
constant="true"/>` and the parser warns `Symbol "m" has no value`. Writing
`model identity "..."` avoids it. The existing test
`the_model_itself_can_carry_an_sbo_term` already guards the `model.` form
against a phantom symbol; the module-name form needs the same treatment.

## Related limitation (not a bug)

`antimony-rs` has no SBML -> Antimony converter, so an SBML file cannot be
shown or edited as Antimony.

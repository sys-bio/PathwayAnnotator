Three small hand-written SBML models in the shape of curated BioModels, for
the tests. They use molecules in the BiGG and ChEBI fixtures, and every
resource form the miner reads:

- BIOMD0000000001: G6P (MIRIAM URN), Glc (generic D-glucose, a broader
  ChEBI id), s1 = ATP, reaction HK = Rhea 17825.
- BIOMD0000000002: G6P (compact identifiers.org), s1 = fructose 6-phosphate
  (KEGG C00085).
- BIOMD0000000005 (in the benchmark test set): G6P (ChEBI web page link,
  the 2- form), s1 = ADP (KEGG URN), mix = ATP and ADP (ambiguous), prot = a
  UniProt protein (not in the pack).

"s1" names a different molecule in each model, so it is noise.

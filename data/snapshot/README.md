# Source data snapshot

The data the published knowledge pack is built from, kept here so the
GitHub Pages build needs no downloads and every deploy uses the same data.
`kpack-build build --cache data/snapshot` builds a pack from it.

| Folder / file | Source | Version | License |
|---|---|---|---|
| `bigg/` | [BiGG Models](https://bigg.bio) bulk downloads (`/api/v3/download/metabolites`, `/reactions`) | fetched on the date in `bigg/fetched.txt` | CC BY 4.0 (BRIGHT data) and the BiGG Models license (legacy UC San Diego content; notice below) |
| `chebi/` | [ChEBI](https://www.ebi.ac.uk/chebi) flat files | release in `chebi/README` (257) | CC BY 4.0 |
| `biomodels-digest.json.gz` | [BioModels](https://www.biomodels.org) curated models: the names and `is` / `isVersionOf` annotations of their species and reactions | fetch date inside the file | CC0 1.0 |

To refresh (then rebuild, check the benchmark, and commit):

```
cargo run --release -p kpack-build -- fetch --cache data/snapshot --source bigg
cargo run --release -p kpack-build -- fetch --cache data/snapshot --source chebi    # the release in chebi::RELEASE
cargo run --release -p kpack-build -- fetch --source biomodels                       # into data/cache (large)
cargo run --release -p kpack-build -- digest-biomodels
```

ChEBI data is from https://www.ebi.ac.uk/chebi - the version of ChEBI is 257.

## BiGG Models license notice

The BiGG Models license requires the following notice in all copies:

Copyright &copy; 2019 The Regents of the University of California

All Rights Reserved

Permission to use, copy, modify and distribute any part of BiGG Models for educational, research
and non-profit purposes, without fee, and without a written agreement is hereby granted, provided
that the above copyright notice, this paragraph and the following three paragraphs appear in all
copies.

Those desiring to incorporate BiGG Models into commercial products or use for commercial purposes
should contact the Technology Transfer & Intellectual Property Services, University of California,
San Diego, 9500 Gilman Drive, Mail Code 0910, La Jolla, CA 92093-0910, Ph: (858) 534-5815, FAX:
(858) 534-7345, e-mail: invent@ucsd.edu.

In no event shall the University of California be liable to any party for direct, indirect,
special, incidental, or consequential damages, including lost profits, arising out of the use of
this bigg database, even if the University of California has been advised of the possibility of
such damage.

The BiGG Models provided herein is on an "as is" basis, and the University of California has no
obligation to provide maintenance, support, updates, enhancements, or modifications. The
University of California makes no representations and extends no warranties of any kind, either
implied or express, including, but not limited to, the implied warranties of merchantability or
fitness for a particular purpose, or that the use of the BiGG Models will not infringe any patent,
trademark or other rights.

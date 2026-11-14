# Annotation benchmark, 2026-11-14

- Models: 215 (test set), 0 unreadable
- Engine providers: names, decoder, context
- Pack built 2026-11-14:
  - BiGG 2026-11-14
  - ChEBI 257
  - BioModels 2026-11-14, 860 curated models, test set excluded

| | Species | Reactions |
|---|---|---|
| Annotated | 3569 | 4095 |
| Annotated with something outside the pack | 2302 | 3454 |
| **Graded** | **1267** | **641** |
| Graded with tied annotations | 96 | 460 |
| Got a suggestion (coverage) | 1222 (96.4%) | 577 (90.0%) |
| **Right first (top-1)** | **91.9%** | **82.1%** |
| Right in top 3 | 93.4% | 84.4% |
| Right first, counting an equivalent reaction (same chemistry, Rhea or MetaNetX id, or an EC number the curator gave) | 91.9% | 82.1% |
| Right in top 5 | 93.6% | 84.7% |
| First suggestion right, when there is one (precision) | 95.3% | 91.2% |
| Mean reciprocal rank | 0.927 | 0.831 |
| Lone suggestion right | 933 of 949 (98.3%) | 243 of 258 (94.2%) |

### Novel test models only

133 of the 215 test models share less than 50% of their species names with every training model. The rest have a near-twin in the training set, which flatters the scores above; these are the fairer estimate for a model the tool has never seen.

| | Species | Reactions |
|---|---|---|
| Graded | 610 | 404 |
| Got a suggestion (coverage) | 96.7% | 90.8% |
| **Right first (top-1)** | **90.8%** | **82.7%** |
| Right in top 3 | 92.6% | 84.4% |
| Right first, counting an equivalent reaction (same chemistry, Rhea or MetaNetX id, or an EC number the curator gave) | 90.8% | 82.7% |
| First suggestion right, when there is one (precision) | 93.9% | 91.0% |

## Confidence levels

How often the first suggestion was right, by the engine's own          confidence level (`annot_engine::confidence`). "Share" is the share of graded elements          with a suggestion.

| Models | Level | Species: share | Species: right | Reactions: share | Reactions: right | Reactions: right or equivalent | Reactions: right, specific keys only |
|---|---|---|---|---|---|---|---|
| all | high | 69.1% | 99.4% | 28.1% | 99.4% | 99.4% | 100.0% of 121 |
| all | likely | 27.0% | 90.9% | 68.1% | 91.1% | 91.1% | 94.0% of 232 |
| all | uncertain | 3.8% | 53.2% | 3.8% | 31.8% | 31.8% | 31.6% of 19 |
| novel | high | 70.7% | 99.3% | 30.2% | 100.0% | 100.0% | 100.0% of 110 |
| novel | likely | 24.4% | 88.9% | 65.7% | 89.6% | 89.6% | 95.3% of 213 |
| novel | uncertain | 4.9% | 41.4% | 4.1% | 46.7% | 46.7% | 50.0% of 12 |

## Species not right first

45 with no suggestion, 35 with only wrong ones, 22 right but ranked lower.

### No suggestion (first 40 of 45)

| Model | Id | Name | Annotated as | First suggestion | Level |
|---|---|---|---|---|---|
| BIOMD0000000075 | IP3X_Cytosol | IP3X_Cytosol | 1D-myo-inositol 1,4,5-trisphosphate (mi145p) | - | - |
| BIOMD0000000105 | ROS |  | peroxynitrite (CE5643) / superoxide (o2s) / hypochlorous acid (CE4633) / hydrogen peroxide (h2o2) | - | - |
| BIOMD0000000260 | s2 | iron_in_Bone Marrow | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000275 | RA |  | 13-cis-retinoate (13_cis_retn) / 9-cis-retinoate (CE1617) | - | - |
| BIOMD0000000340 | VK | VK | phylloquinone (phllqne) / menaquinone-9 (mqn9) / menadione (mndn) / menaquinone-8 (mqn8) / menaquinone-7 (mqn7) | - | - |
| BIOMD0000000340 | VKH2 | VKH2 | phylloquinone (phllqne) / menaquinone-9 (mqn9) / menadione (mndn) / menaquinone-8 (mqn8) / menaquinone-7 (mqn7) | - | - |
| BIOMD0000000340 | VKO | VKO | phylloquinone (phllqne) / menaquinone-9 (mqn9) / menadione (mndn) / menaquinone-8 (mqn8) / menaquinone-7 (mqn7) | - | - |
| BIOMD0000000340 | VK_p | VK_p | phylloquinone (phllqne) / menaquinone-9 (mqn9) / menadione (mndn) / menaquinone-8 (mqn8) / menaquinone-7 (mqn7) | - | - |
| BIOMD0000000410 | _96 | Rec_active | peroxynitrite (CE5643) / superoxide (o2s) / hypochlorous acid (CE4633) / hydrogen peroxide (h2o2) | - | - |
| BIOMD0000000470 | s_1435 | phosphatidylethanolamine (dihexadec-9enoyl, n-C16:1) | 1,2-distearoylphosphatidylethanolamine (pe180) / PE(18:1(11Z)/18:1(9Z)) (pe18111Z1819Z) / 1,2-dihexadecanoyl-sn-glycero-3-phosphoethanolamine (pe160) | - | - |
| BIOMD0000000490 | Hn_aw | Hn_aw | histaminium (hista) | - | - |
| BIOMD0000000490 | Hn_b | Hn_b | histaminium (hista) | - | - |
| BIOMD0000000495 | mw08818dfe_fb12_45cc_8c1d_d965f142d0ce | EtnE | ethanolaminium (etha) | - | - |
| BIOMD0000000495 | mw812f63db_4cb0_40ad_b92b_9874be969dfe | Cho | choline (chol) | - | - |
| BIOMD0000000495 | mw919f8a86_e702_4b24_9cd7_adad694fcf9b | ChoE | choline (chol) | - | - |
| BIOMD0000000505 | CarCYT |  | (R)-carnitine (crn) / (S)-carnitine (crn__D) | - | - |
| BIOMD0000000505 | CarMAT |  | (R)-carnitine (crn) / (S)-carnitine (crn__D) | - | - |
| BIOMD0000000505 | NADtMAT |  | (R)-NADHX (nadhx__R) / (S)-NADHX (nadhx__S) / (R)-NADPHX (nadphx__R) / (S)-NADPHX (nadphx__S) / NADH (nadh) / NAD (nad) | - | - |
| BIOMD0000000575 | Bioamines | Bioamines | (Z)-3-aminoperacrylic acid (poaac) / 2-iminiopropionate (2amac) / trimethylammonium (tmam) / ethanolaminium (etha) / cysteaminium (cysam) | - | - |
| BIOMD0000000590 | cp | cp | carbamoyl phosphate (cbp) | - | - |
| BIOMD0000000590 | dho | dho | (S)-dihydroorotate (dhor__S) | - | - |
| BIOMD0000000725 | AHMDHP | AHMDHP | 2-amino-6-(hydroxymethyl)-7,8-dihydropteridin-4-ol (2ahhmp) | - | - |
| BIOMD0000000725 | DHNTP | DHNTP | 7,8-dihydroneopterin 3'-triphosphate (ahdt) | - | - |
| BIOMD0000000725 | Gln | Gln | L-glutamine (gln__L) | - | - |
| BIOMD0000000725 | HAD | HAD | (Z)-1,2-ethenediol (gcald) | - | - |
| BIOMD0000000725 | PTHP | PTHP | dyspropterin (6pthp) / (R)-dyspropterin (6pthp__R) | - | - |
| BIOMD0000000725 | SK | SK | shikimate (skm) | - | - |
| BIOMD0000000725 | ffTHFGlu | ffTHFGlu | 5-formyltetrahydrofolate (5fthf_any) / (6S)-5-formyltetrahydrofolate (5fthf) | - | - |
| BIOMD0000000735 | FeBM | FeBM | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeBM_0 | FeBM* | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeDuo | FeDuo* | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeDuo_0 | FeDuo | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeLiver | FeLiver* | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeLiver_0 | FeLiver | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeOutside | FeOutside* | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeOutside_0 | FeOutside | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeRBC | FeRBC* | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeRBC_0 | FeRBC | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeRest | FeRest* | iron (fe3) / iron (fe2) | - | - |
| BIOMD0000000735 | FeRest_0 | FeRest | iron (fe3) / iron (fe2) | - | - |

### Wrong suggestions only (first 35 of 35)

| Model | Id | Name | Annotated as | First suggestion | Level |
|---|---|---|---|---|---|
| BIOMD0000000050 | AA |  | acetate (ac) | arachidonate (arachd) | likely |
| BIOMD0000000050 | FA |  | formate (for) | fumarate (fum) | likely |
| BIOMD0000000050 | MG |  | methylglyoxal (mthgxl) | magnesium (mg2) | uncertain |
| BIOMD0000000065 | L | lactose_internal | lactose (lcts) | L-leucine (leu__L) | uncertain |
| BIOMD0000000070 | Lac | Lactate | (R)-lactate (lac__D) | (S)-lactate (lac__L) | likely |
| BIOMD0000000075 | DAG_PM | DAG_PM | 1,2-dilauroyl-sn-glycerol (12dgr120) / 1,2 ditetradecanoylglycerol (12dgr140) / 1,2-dioctadecanoyl-sn-glycerol (12dgr180) / 1,2-di-[(11Z)-octadecenoyl]glycerol (12dgr181) | water (h2o) | uncertain |
| BIOMD0000000085 | species_3 | T | GTP (gtp) | thymidine (thymd) | likely |
| BIOMD0000000085 | species_6 | GD | GDP (gdp) | maltose (malt) | uncertain |
| BIOMD0000000085 | species_8 | D | GDP (gdp) | L-aspartate (asp__L) | likely |
| BIOMD0000000090 | Hm | Hm | hydron (h) | His-Met (hismet) | likely |
| BIOMD0000000090 | Ho | Ho | hydron (h) | water (h2o) | likely |
| BIOMD0000000260 | s3 | iron_in_RBC | iron (fe3) / iron (fe2) | calcium (ca2) | uncertain |
| BIOMD0000000470 | s_0075 | 1,2-dihexadecanoyl-sn-glycerol 3-phosphate | 1,2-dimyrsitoylphosphatidate (pa140) | 1,2-dihexadecanoyl-sn-glycerol-3-phosphate (pa160) | likely |
| BIOMD0000000470 | s_0446 | alpha-D-Ribose 5-phosphate | D-ribofuranose 5-phosphate (r5p) | alpha-D-ribofuranose 5-phosphate (ar5p) | likely |
| BIOMD0000000470 | s_0654 | D-Glucosamine 6-phosphate | alpha-D-glucosamine 6-phosphate (gam6p) | 2-ammonio-2-deoxy-D-glucopyranose 6-phosphate (gam6p_any) | likely |
| BIOMD0000000470 | s_0979 | Hexadecenoyl-CoA (n-C16:1CoA) | (E)-hexadec-2-enoyl-CoA (hdd2coa) | palmitoleoyl-CoA (hdcoa) | likely |
| BIOMD0000000495 | mw73259e20_240e_4f3a_b2e0_9ca248658898 | SerE | D-serine (ser__D) / L-serine (ser__L) | O-acetyl-L-serine (acser) | uncertain |
| BIOMD0000000495 | mwf166ad55_4ff0_49fb_95d2_b657ad7653d5 | PE | 1,2-distearoylphosphatidylethanolamine (pe180) / PE(18:1(11Z)/18:1(9Z)) (pe18111Z1819Z) / 1,2-dihexadecanoyl-sn-glycero-3-phosphoethanolamine (pe160) | O-phosphonatoethanaminium (ethamp) | likely |
| BIOMD0000000510 | Rib_g | Rib_g | D-ribofuranose (rib__D) | 2-deoxy-D-ribofuranose (drib) | uncertain |
| BIOMD0000000515 | Rib5P_g | Rib5P_g | D-ribofuranose (rib__D) | aldehydo-D-arabinose 5-phosphate (ara5p) | uncertain |
| BIOMD0000000560 | ROS |  | peroxynitrite (CE5643) / superoxide (o2s) / hypochlorous acid (CE4633) / hydrogen peroxide (h2o2) | 2-oxoglutarate (akg) | uncertain |
| BIOMD0000000565 | cf6p | Fructose-6-Phosphate | D-fructofuranose 6-phosphate (f6p_any) | beta-D-fructofuranose 6-phosphate (f6p) | high |
| BIOMD0000000565 | cg6p | Glucose-6-Phosphate | D-glucose 6-phosphate (g6p_any) | alpha-D-glucose 6-phosphate (g6p_A) | high |
| BIOMD0000000575 | Neuromelanin | Neuromelanin | 5,6-dihydroxyindole (CE4888) | 3-sulfino-L-alanine (3sala) | uncertain |
| BIOMD0000000590 | ca | ca | N-carbamoyl-L-aspartate (cbasp) | calcium (ca2) | high |
| BIOMD0000000590 | omp | omp | orotidine 5'-phosphate (orot5p) | UDP (udp) | uncertain |
| BIOMD0000000590 | oro | oro | orotate (orot) | cytidine 5'-monophosphate (cmp) | uncertain |
| BIOMD0000000725 | CM | CM | chorismate (chor) | chloramphenicol (cm) | likely |
| BIOMD0000000725 | Formyl | Formyl | formate (for) | water (h2o) | uncertain |
| BIOMD0000000725 | fTHFGlu | fTHFGlu | 10-formyltetrahydrofolate (10fthf_any) / (6R)-10-formyltetrahydrofolate (10fthf) | L-glutamate (glu__L) | uncertain |
| BIOMD0000000750 | Na | Na | acetylcholine (ach) | sodium (na1) | high |
| BIOMD0000000805 | Le | Le | (R)-lactate (lac__D) / (S)-lactate (lac__L) | 3-iodo-L-tyrosine (3ityr__L) | uncertain |
| BIOMD0000000805 | Li | Li | (R)-lactate (lac__D) / (S)-lactate (lac__L) | 3,5-diiodo-L-tyrosinate (35diotyr) | uncertain |
| BIOMD0000000890 | c | c | cholesterol (chsterol) | calcium (ca2) | likely |
| BIOMD0000001055 | Fructose_6_Phosphate | Fructose-6-Phosphate | D-fructofuranose 6-phosphate (f6p_any) | beta-D-fructofuranose 6-phosphate (f6p) | high |

### Right answer ranked lower (first 22 of 22)

| Model | Id | Name | Annotated as | First suggestion | Level |
|---|---|---|---|---|---|
| BIOMD0000000015 | R5P | ribose-5-phosphate | D-ribofuranose 5-phosphate (r5p) | alpha-D-ribofuranose 5-phosphate (ar5p) (right one at 2) | likely |
| BIOMD0000000050 | Glu |  | D-glucopyranose (glc__D) | L-glutamate (glu__L) (right one at 2) | likely |
| BIOMD0000000070 | Rib5P | Ribose 5-phosphate | D-ribofuranose 5-phosphate (r5p) | alpha-D-ribofuranose 5-phosphate (ar5p) (right one at 2) | likely |
| BIOMD0000000380 | glc | glucose | alpha-D-glucose (glc__aD) | D-glucopyranose (glc__D) (right one at 2) | likely |
| BIOMD0000000380 | glx | glucose | alpha-D-glucose (glc__aD) | D-glucopyranose (glc__D) (right one at 2) | likely |
| BIOMD0000000470 | s_0032 | (R)-Pantoate | pantoic acid (pant) | (R)-pantoate (pant__R) (right one at 2) | likely |
| BIOMD0000000470 | s_0121 | 1-hexadecanoyl-sn-glycerol 3-phosphate | 1-acyl-sn-glycerol 3-phosphate (HC02042) | 1-palmitoyl-sn-glycerol 3-phosphate (1hdecg3p) (right one at 2) | likely |
| BIOMD0000000470 | s_0255 | 3,4-dihydroxy-2-butanone 4-phosphate | 2-hydroxy-3-oxobutyl phosphate (db4p_any) | (2S)-2-hydroxy-3-oxobutyl phosphate (db4p) (right one at 2) | likely |
| BIOMD0000000470 | s_0271 | 3-Deoxy-D-manno-2-octulosonate | keto-3-deoxy-D-manno-octulosonate (kdo_alt) | 3-deoxy-alpha-D-manno-oct-2-ulosonate (kdo) (right one at 2) | likely |
| BIOMD0000000470 | s_0546 | Co2+ | cobalt (cobalt2) | carbon dioxide (co2) (right one at 2) | likely |
| BIOMD0000000470 | s_0548 | Co2+ | cobalt (cobalt2) | carbon dioxide (co2) (right one at 2) | likely |
| BIOMD0000000495 | mwee54b5b4_b8c0_41df_8dda_5b160c5e10a5 | PC | phosphatidylcholine (pchol_hs) | cytidine 5'-monophosphate (cmp) (right one at 2) | likely |
| BIOMD0000000505 | C4AcetoacylCoAMAT |  | acetoacetyl-CoA (aacoa) | (3S)-3-hydroxy-3-methylglutaryl-CoA (hmgcoa) (right one at 2) | uncertain |
| BIOMD0000000575 | O2 | O2- | superoxide (o2s) | dioxygen (o2) (right one at 2) | likely |
| BIOMD0000000725 | AHMDPP | AHMDPP | (2-amino-4-hydroxy-7,8-dihydropteridin-6-yl)methyl diphosphate (2ahhmd) | 2-amino-6-(hydroxymethyl)-7,8-dihydropteridin-4-ol (2ahhmp) (right one at 2) | uncertain |
| BIOMD0000000725 | DHSK | DHSK | 3-dehydroshikimate (3dhsk) | 7-phospho-2-dehydro-3-deoxy-D-arabino-heptonate (2dda7p) (right one at 2) | uncertain |
| BIOMD0000000050 | Gly |  | glycine (gly) | glycerol (glyc) (right one at 3) | likely |
| BIOMD0000000725 | Gly | Gly | glycine (gly) | glycerol (glyc) (right one at 3) | likely |
| BIOMD0000001005 | NAM | NAM | nicotinamide (ncam) | nicotinate D-ribonucleotide (nicrnt) (right one at 4) | uncertain |
| BIOMD0000000495 | mw8796c919_9251_4970_8f87_0bca9ecfeb5c | Etn | ethanolaminium (etha) | sphinganine 1-phosphate (sph1p) (right one at 5) | uncertain |
| BIOMD0000000505 | NADHMAT |  | NADH (nadh) | acetoacetate (acac) (right one at 5) | likely |
| BIOMD0000000495 | mwcb834e43_dc57_45ae_9452_f4c10955caf1 | PCho | choline phosphate (cholp) | sphinganine 1-phosphate (sph1p) (right one at 6) | uncertain |

## Reactions not right first

64 with no suggestion, 34 with only wrong ones, 17 right but ranked lower.

### No suggestion (first 20 of 64)

| Model | Id | Name | Annotated as | First suggestion | Level |
|---|---|---|---|---|---|
| BIOMD0000000015 | dgnuc |  | 5'-nucleotidase (dGMP) (NTD8) / 5'-nucleotidase (dGMP) (NTD8pp) / 5'-nucleotidase (dGMP), lysosomal (NTD8l) | - | - |
| BIOMD0000000015 | mat |  | methionine adenosyltransferase (METAT) | - | - |
| BIOMD0000000070 | vATPase | ATPase | ATP maintenance requirement (ATPM) / Non-growth associated ATP maintenance (ATPPHm) | - | - |
| BIOMD0000000070 | vRibPiso | Ribose phosphate isomerase | ribose-5-phosphate isomerase (RPI) / Ribose-5-phosphate isomerase (RPIh) | - | - |
| BIOMD0000000085 | reaction_10 | RG*T hydrolysis | nucleoside-triphosphatase (GTP) (NTP3) / nucleoside-triphosphatase (GTP) (periplasm) (NTP3pp) / nucleoside-triphosphatase (GTP) (NTP3e) | - | - |
| BIOMD0000000085 | reaction_13 | G*AT hydrolysis | nucleoside-triphosphatase (GTP) (NTP3) / nucleoside-triphosphatase (GTP) (periplasm) (NTP3pp) / nucleoside-triphosphatase (GTP) (NTP3e) | - | - |
| BIOMD0000000105 | ProteasomeActivity1 |  | ATP maintenance requirement (ATPM) / Non-growth associated ATP maintenance (ATPPHm) | - | - |
| BIOMD0000000105 | ProteasomeActivity2 |  | ATP maintenance requirement (ATPM) / Non-growth associated ATP maintenance (ATPPHm) | - | - |
| BIOMD0000000105 | ProteasomeActivity3 |  | ATP maintenance requirement (ATPM) / Non-growth associated ATP maintenance (ATPPHm) | - | - |
| BIOMD0000000105 | ProteasomeActivity4 |  | ATP maintenance requirement (ATPM) / Non-growth associated ATP maintenance (ATPPHm) | - | - |
| BIOMD0000000105 | ProteasomeActivity5 |  | ATP maintenance requirement (ATPM) / Non-growth associated ATP maintenance (ATPPHm) | - | - |
| BIOMD0000000145 | R3 |  | nucleoside-triphosphatase (GTP) (NTP3) / nucleoside-triphosphatase (GTP) (periplasm) (NTP3pp) / nucleoside-triphosphatase (GTP) (NTP3e) | - | - |
| BIOMD0000000165 | Gs_gtp_hydrolysis | Gs_gtp_hydrolysis | nucleoside-triphosphatase (GTP) (NTP3) / nucleoside-triphosphatase (GTP) (periplasm) (NTP3pp) / nucleoside-triphosphatase (GTP) (NTP3e) | - | - |
| BIOMD0000000190 | SAMdc | S-adenosylmethionine decarboxylase | adenosylmethionine decarboxylase (ADMDC) | - | - |
| BIOMD0000000320 | AADC_clearance | L-Dopa clearance via AADC  | 3-Hydroxy-L-tyrosine carboxy-lyase (3HLYTCL) / L-Tryptophan decarboxy-lyase (LTDCL) | - | - |
| BIOMD0000000450 | VcMTCH | VcMTCH | methenyltetrahydrofolate cyclohydrolase (MTHFC_1) / methenyltetrahydrifikate cyclohydrolase (mitochondrial) (MTHFCm) / 5,10-Methenyltetrahydrofolate 5-hydrolase (decyclizing) (MTHFCx) | - | - |
| BIOMD0000000590 | r1 | r1 | carbamoyl-phosphate synthase (glutamine-hydrolysing) (HCGALm) / hydrogen-carbonate:L-glutamine amido-ligase (ADP-forming, carbamate-phosphorylating) (CBPSn) / carbamoyl-phosphate synthase (glutamine-hydrolysing) (CBPS) | - | - |
| BIOMD0000000590 | r2 | r2 | carbamoyl-phosphate:L-aspartate carbamoyltransferase (ASPCTn) / aspartate carbamoyltransferase (ASPCT_1) / carbamoyl-phosphate:L-aspartate carbamoyltransferase (ASPCTm) | - | - |
| BIOMD0000000590 | r3 | r3 | (S)-dihydroorotate amidohydrolase (DHORTSn) / (S)-dihydroorotate amidohydrolase (DHR) / dihydroorotase (DHORTS) | - | - |
| BIOMD0000000605 | v2 | v2 | hexokinase (D-glucose:ATP) (HEX1) | - | - |

### Wrong suggestions only (first 20 of 34)

| Model | Id | Name | Annotated as | First suggestion | Level |
|---|---|---|---|---|---|
| BIOMD0000000015 | ampd |  | AMP aminohydrolase (AMPDg) / Adenosine monophosphate deaminase (AMPDA) | insosine kinase (INSK) | uncertain |
| BIOMD0000000015 | gmps |  | GMP synthase (GMPS_glu_m) / GMP synthase (GMPS2) | GMP synthase (GMPS) | likely |
| BIOMD0000000015 | gnuc |  | 5'-nucleotidase (GMP) (NTD9) / 5'-nucleotidase (GMP) (NTD9pp) / 5'-nucleotidase (GMP), extracellular (NTD9e) / 5'-nucleotidase (GMP), lysosome (NTD9l) | guanine phosphoribosyltransferase (GUAPRT) | uncertain |
| BIOMD0000000015 | inuc |  | 5'-nucleotidase (IMP) (NTD11) / 5'-nucleotidase (IMP) (NTD11pp) | hypoxanthine phosphoribosyltransferase (Hypoxanthine) (HXPRT) | likely |
| BIOMD0000000070 | vGSHox | Glutathione oxidation | glutathioneoxidoreductase(apicoplast) (GDR_nadp_h) / glutathione oxidoreductase (GTHOr) / glutathione oxidoreductase (GTHOm) | glutathione peroxidase (GTHPe) | likely |
| BIOMD0000000080 | Reaction_6 | Hydrolysis of GTP to GDP | nucleoside-triphosphatase (GTP) (NTP3) / nucleoside-triphosphatase (GTP) (periplasm) (NTP3pp) / nucleoside-triphosphatase (GTP) (NTP3e) | phytoene synthase (PSY) | uncertain |
| BIOMD0000000085 | reaction_20 | RG*AT hydrolysis | nucleoside-triphosphatase (GTP) (NTP3) / nucleoside-triphosphatase (GTP) (periplasm) (NTP3pp) / nucleoside-triphosphatase (GTP) (NTP3e) | aspartase racemase (ASPR) | uncertain |
| BIOMD0000000085 | reaction_3 | G*T  hydrolysis | nucleoside-triphosphatase (GTP) (NTP3) / nucleoside-triphosphatase (GTP) (periplasm) (NTP3pp) / nucleoside-triphosphatase (GTP) (NTP3e) | Conversion of CHEBI:15903 to CHEBI:4167, because one is an instance of the other (GALM1) | uncertain |
| BIOMD0000000450 | V_CTGL | V_CTGL | gamma-glutamylcysteine synthetase (GLUCYS) / gamma-Glutamylcysteine synthase (GCL) | L-cysteine + O-phosphonato-L-homoserine = L-cystathionine (CYSTS_1) | likely |
| BIOMD0000000450 | V_MTHFR | V_MTHFR | methylenetetrahydrofolate dehydrogenase (NADP) (MTHFD) / methylenetetrahydrofolate dehydrogenase (NADP) mitochondrial (MTHFDm) | (6R)-5,10-methylenetetrahydrofolate = (6S)-5-methyltetrahydrofolate (MTHFR) | likely |
| BIOMD0000000470 | r_0664 | glucose-6-phosphate isomerase | alpha-D-Glucose 6-phosphate ketol-isomerase (PGI1) / alpha-D-Glucose 6-phosphate ketol-isomerase (PGI1c) | glucose-6-phosphate isomerase (PGI) | likely |
| BIOMD0000000470 | r_1962 | NAD(P) transhydrogenase (periplasm) | NAD transhydrogenase (NADTRHD) / NADPH:NAD+ oxidoreductase (NNOR) | NAD(P) transhydrogenase (periplasm) (THD2pp) | high |
| BIOMD0000000590 | r5 | r5 | orotate phosphoribosyltransferase (ORPT) | UMP kinase (UTP) (UMPK3) | uncertain |
| BIOMD0000000590 | r6 | r6 | orotidine-5'-phosphate decarboxylase (OMPDC) | UMP kinase (UTP) (UMPK3) | likely |
| BIOMD0000000590 | r7 | r7 | nucleoside-diphosphate kinase (ATP:UDP) (NDPK2) / nucleoside-diphosphate kinase (ATP:UDP), mitochondrial (NDPK2m) / nucleoside-diphosphate kinase (ATP:UDP), nuclear (NDPK2n) | UMP kinase (UTP) (UMPK3) | likely |
| BIOMD0000000605 | v6 | v6 | glycerol-3-phosphatase (G3PT) | Glycerol dehydrogenase (GLYCDx) | likely |
| BIOMD0000000690 | FolP | FolP | dihydropteroate synthase (DHPS2_1) / dihydropteroate synthase (mitochondrial) (FOLD3m) | dihydropteroate synthase (DHPS) | likely |
| BIOMD0000000725 | R24 | R24 - 10-Formyltetrahydrofolate synthetase | formate tetrahydrofolate ligase (mitochondrial irreversible) (FTHFLmi) / formate-tetrahydrofolate ligase (FTHFLi) | 5,6,7,8-tetrahydrofolyl-L-glutamic acid = L-glutamate + (6S)-5,6,7,8-tetrahydrofolate (THFGLUH) | uncertain |
| BIOMD0000000725 | R26 | R26 - 10-Formyltetrahydrofolate dehydrogenase | formyltetrahydrofolate dehydrogenase (FTHFDH) | 5,6,7,8-tetrahydrofolyl-L-glutamic acid = L-glutamate + (6S)-5,6,7,8-tetrahydrofolate (THFGLUH) | uncertain |
| BIOMD0000001090 | R_LCTO_cytop | (S)-lactate+oxygen=>pyruvate+hydrogenperoxide | (S)-lactate:oxygen 2-oxidoreductase (decarboxylating) (LO) | L-lactate dehydrogenase (LDH_L) | likely |

### Right answer ranked lower (first 17 of 17)

| Model | Id | Name | Annotated as | First suggestion | Level |
|---|---|---|---|---|---|
| BIOMD0000000070 | vLDHNADPH | Lactate dehydrogenase | L-lactate dehydrogenase (LDH_L) / L-lactate dehydrogenase (LDH_Lm) / (S)-Lactate:NAD+ oxidoreductase (LLDHh) | lactate dehydrogenase (LDHm) (right one at 2) | likely |
| BIOMD0000000245 | r3 | Pyruvate Dehydrogenase | pyruvate dehydrogenase(apicoplast) (PDHapi) / pyruvate dehydrogenase (PDH) / pyruvate dehydrogenase (PDHm) | pyruvate decarboxylase (PYRDC) (right one at 2) | likely |
| BIOMD0000000470 | r_0697 | glycerate kinase | glycerate kinase (GLYPT) / glycerate kinase (GLYCK) | glycerate kinase (GLYCK2) (right one at 2) | likely |
| BIOMD0000000635 | revreaction_38 | revreaction_38 | 3',5'-cyclic-nucleotide phosphodiesterase (PDE1) / 3',5'-cyclic-nucleotide phosphodiesterase,Golgi (PDE1g) / adenosine 3',5'-phosphate 5'-nucleotidohydrolase (APNn) | adenylate cyclase (ADECn) (right one at 2) | uncertain |
| BIOMD0000000635 | revreaction_39 | revreaction_39 | 3',5'-cyclic-nucleotide phosphodiesterase (PDE1) / 3',5'-cyclic-nucleotide phosphodiesterase,Golgi (PDE1g) / adenosine 3',5'-phosphate 5'-nucleotidohydrolase (APNn) | adenylate cyclase (ADECn) (right one at 2) | uncertain |
| BIOMD0000000635 | revreaction_40 | revreaction_40 | 3',5'-cyclic-nucleotide phosphodiesterase (PDE1) / 3',5'-cyclic-nucleotide phosphodiesterase,Golgi (PDE1g) / adenosine 3',5'-phosphate 5'-nucleotidohydrolase (APNn) | adenylate cyclase (ADECn) (right one at 2) | uncertain |
| BIOMD0000000635 | revreaction_41 | revreaction_41 | 3',5'-cyclic-nucleotide phosphodiesterase (PDE1) / 3',5'-cyclic-nucleotide phosphodiesterase,Golgi (PDE1g) / adenosine 3',5'-phosphate 5'-nucleotidohydrolase (APNn) | adenylate cyclase (ADECn) (right one at 2) | uncertain |
| BIOMD0000001090 | R_R04198_C3_cytop | (2S,4S)-4-Hydroxy-2,3,4,5-tetrahydrodipicolinate+NADH+H+=>2,3,4,5-Tetrahydrodipicolinate+NAD++H2O | Dihydrodipicolinate reductase (NADH) reversible (DHDPRx_r) | 4-hydroxy-tetrahydrodipicolinate reductase (H4THDPR) (right one at 2) | likely |
| BIOMD0000000225 | vpfk | vpfk | phosphofructokinase (PFKg) / phosphofructokinase (PFK) | fructose-bisphosphatase (FBP) (right one at 3) | likely |
| BIOMD0000000470 | r_0761 | hexokinase (D-fructose:ATP) | hexokinase (D-glucose:ATP) (HEX1) | hexokinase (D-fructose:ATP) (HEX7) (right one at 3) | likely |
| BIOMD0000000470 | r_0925 | malate dehydrogenase | malate dehydrogenase (MDHi2) / malate dehydrogenase (mitochondrial) (MDHm) / malate dehydrogenase (peroxisomal) (MDHp) / (S)-malate:NAD+ oxidoreductase (MDHf) / (S)-malate:NAD+ oxidoreductase (MDHh) | Malate dehydrogenase (ubiquinone 8 as acceptor) (MDH2) (right one at 3) | likely |
| BIOMD0000000470 | r_1230 | protoporphyrinogen oxidase (aerobic) | Protoporphyrinogen oxidase (aerobic) (PPPGO2_1) / protoporphyrinogen IX oxidase (PPPGO_1) | protoporphyrinogen oxidase (aerobic) (PPPGO) (right one at 3) | likely |
| BIOMD0000000590 | r8 | r8 | CTP synthase (glutamine) (CTPS2n) / CTP synthase (glutamine) (CTPS2) | dCTP deaminase (DCTPD2_1) (right one at 3) | likely |
| BIOMD0000000725 | R6 | R6 - Phosphoenolpyruvate:3-phosphoshikimate 5-O-(1-carboxyvinyl)-transferase | 3-phosphoshikimate 1-carboxyvinyltransferase (PSCVT) / enolpyruvylshikimate phosphate synthase (PSCIT) | UDP-N-acetylglucosamine 1-carboxyvinyltransferase (UAGCVT_1) (right one at 3) | likely |
| BIOMD0000001055 | R11 | R11 | pyruvate carboxylase (PC) / pyruvate carboxylase (PCm) | oxaloacetate decarboxylase (OAADC) (right one at 3) | likely |
| BIOMD0000000015 | pyr |  | orotate phosphoribosyltransferase (ORPT) | adenine phosphoribosyltransferase (ADPT2) (right one at 4) | uncertain |
| BIOMD0000000470 | r_0096 | 3-cis-2-trans-enoyl-CoA isomerase | dodecenoyl-Coenzyme A delta isomerase (DCIm) | 3-cis-2-trans-enoyl-CoA isomerase (CTECOAI7) (right one at 4) | likely |

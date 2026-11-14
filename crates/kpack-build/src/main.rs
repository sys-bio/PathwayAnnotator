//! `kpack-build`: download sources, build the knowledge pack, inspect it.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use kpack::{EntityFlags, Pack, ReactionId, Target, XrefScope};
use kpack_build::{PackBuilder, date, fetch, sources};

#[derive(Parser)]
#[command(name = "kpack-build", about = "Build the annotator's knowledge pack")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download the sources into the cache.
    Fetch {
        #[arg(long, default_value = "data/cache")]
        cache: PathBuf,
        /// Which source: bigg, chebi, biomodels, or all. BioModels takes a
        /// while (one download per model) and resumes where it stopped.
        #[arg(long, default_value = "all")]
        source: String,
    },
    /// Build a pack from the cached sources.
    Build {
        #[arg(long, default_value = "data/cache")]
        cache: PathBuf,
        #[arg(long, default_value = "packs/kpack.bin")]
        out: PathBuf,
        /// Leave the benchmark's test models out of the BioModels names.
        #[arg(long)]
        exclude_test_models: bool,
    },
    /// Summarize a pack.
    Stats { pack: PathBuf },
    /// Look up a name ("ATP", "G6P") or an id ("chebi:CHEBI:15422",
    /// "bigg.reaction:PFK") in a pack.
    Lookup { pack: PathBuf, query: String },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Fetch { cache, source } => fetch_sources(&cache, &source),
        Command::Build {
            cache,
            out,
            exclude_test_models,
        } => build(&cache, &out, exclude_test_models),
        Command::Stats { pack } => load(&pack).map(|p| stats(&p)),
        Command::Lookup { pack, query } => load(&pack).map(|p| lookup(&p, &query)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn fetch_sources(cache: &std::path::Path, source: &str) -> Result<()> {
    let all = source == "all";
    let mut known = all;
    if all || source == "bigg" {
        fetch::fetch_bigg(cache)?;
        known = true;
    }
    if all || source == "chebi" {
        fetch::fetch_chebi(cache)?;
        known = true;
    }
    if all || source == "biomodels" {
        sources::biomodels::fetch(cache)?;
        known = true;
    }
    if known {
        Ok(())
    } else {
        Err(format!("unknown source {source:?}: use bigg, chebi, biomodels or all").into())
    }
}

fn build(cache: &std::path::Path, out: &std::path::Path, exclude_test_models: bool) -> Result<()> {
    let mut b = PackBuilder::new();
    let bigg_dir = cache.join("bigg");
    let s = sources::bigg::load(&mut b, &bigg_dir, &fetch::fetched_date(&bigg_dir))?;
    eprintln!(
        "BiGG: {} metabolites, {} reactions, {} reactions skipped",
        s.metabolites, s.reactions, s.skipped_reactions
    );
    for (ns, n) in &s.unknown_namespaces {
        eprintln!("  unknown namespace {ns}: {n} ids dropped");
    }
    let c = sources::chebi::load(&mut b, &cache.join("chebi"))?;
    let rows: Vec<_> = c.rows.iter().map(|(f, n)| format!("{f} {n}")).collect();
    eprintln!("ChEBI rows read: {}", rows.join(", "));
    eprintln!(
        "ChEBI: {} entities enriched; {} BiGG ids found, {} unknown to ChEBI, {} shared by two entities; \
         {} charge forms, {} secondary ids, {} names added",
        c.entities, c.seeds, c.seeds_unknown, c.shared_seeds, c.forms_added, c.secondary_ids_added, c.names_added
    );
    eprintln!(
        "ChEBI is_a: {} broader ids, {} narrower ids, {} entity-to-entity links; \
         {} classes dropped as too broad (over {} entities)",
        c.broader_added,
        c.narrower_added,
        c.entity_links,
        c.broad_classes_dropped,
        sources::chebi::MAX_FANOUT
    );
    let bm_dir = cache.join("biomodels");
    let m = sources::biomodels::load(&mut b, &bm_dir, &fetch::fetched_date(&bm_dir), exclude_test_models)?;
    let i = &m.ingest;
    eprintln!(
        "BioModels: {} models listed, {} mined, {} held out, {} missing, {} unreadable",
        m.listed,
        i.models,
        m.held_out,
        m.missing,
        m.unreadable.len()
    );
    for (id, e) in m.unreadable.iter().take(5) {
        eprintln!("  unreadable {id}: {e}");
    }
    eprintln!(
        "BioModels: {} annotated elements: {} resolved, {} ambiguous ({} of them reactions), \
         {} not in the pack; {} names kept, {} dropped as noise",
        i.elements, i.resolved, i.ambiguous, i.ambiguous_reactions, i.unresolved, i.names_kept, i.names_dropped
    );
    let mut unknown: Vec<_> = i.unknown_namespaces.iter().collect();
    unknown.sort_by(|a, b| b.1.cmp(a.1));
    let top: Vec<_> = unknown.iter().take(8).map(|(ns, n)| format!("{ns} {n}")).collect();
    if !top.is_empty() {
        eprintln!("  namespaces not in the pack: {}", top.join(", "));
    }
    for w in b.warnings.iter().take(20) {
        eprintln!("  warning: {w}");
    }
    if b.warnings.len() > 20 {
        eprintln!("  ... and {} more warnings", b.warnings.len() - 20);
    }
    let pack = b.finish(date::today());
    let bytes = kpack::encode(&pack);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, &bytes)?;
    eprintln!("wrote {} ({} bytes)", out.display(), bytes.len());
    Ok(())
}

fn load(path: &std::path::Path) -> Result<Pack> {
    let bytes = std::fs::read(path)?;
    Ok(kpack::decode(&bytes)?)
}

fn stats(p: &Pack) {
    println!("built {}", p.header.built);
    for s in &p.header.sources {
        println!("source {} ({}), {}", s.name, s.version, s.license);
    }
    println!("entities      {}", p.entities.len());
    println!("reactions     {}", p.reactions.len());
    println!("names         {} (entities), {} (reactions)", p.names.len(), p.reaction_names.len());
    println!("xref rows     {}", p.xrefs.len());
    println!("strings       {}", p.strings.len());
    println!("compartments  {}", p.compartments.len());
    let currency = p
        .entities
        .iter()
        .filter(|e| e.flags.contains(EntityFlags::CURRENCY))
        .count();
    println!("currency      {currency}");
    let mut per_ns: BTreeMap<&str, usize> = BTreeMap::new();
    for x in p.entities.iter().flat_map(|e| &e.xrefs).chain(p.reactions.iter().flat_map(|r| &r.xrefs)) {
        *per_ns.entry(p.namespace_prefix(x.ns)).or_default() += 1;
    }
    for (ns, n) in per_ns {
        println!("  {ns:20} {n}");
    }
}

fn lookup(p: &Pack, query: &str) {
    // "prefix:id" where prefix is a namespace, else a name.
    if let Some((prefix, id)) = query.split_once(':')
        && p.namespace_id(prefix).is_some() {
            let targets = p.find_xref_related(prefix, id);
            if targets.is_empty() {
                println!("no match for {prefix} {id}");
            }
            for (t, scope) in targets {
                if scope != XrefScope::Exact {
                    println!("[{prefix}:{id} is {scope:?} than:]");
                }
                match t {
                    Target::Entity(e) => show_entity(p, e),
                    Target::Reaction(r) => show_reaction(p, r),
                }
            }
            return;
        }
    let entities = p.entities_named(query);
    let reactions = p.reactions_named(query);
    if entities.is_empty() && reactions.is_empty() {
        println!("no match for {query:?}");
    }
    for h in entities {
        println!("[{:?} name, {}, in {} curated models]", h.kind, h.sources.names().join(", "), h.count);
        show_entity(p, h.target);
    }
    for h in reactions {
        println!("[{:?} name, {}, in {} curated models]", h.kind, h.sources.names().join(", "), h.count);
        show_reaction(p, h.target);
    }
}

fn show_entity(p: &Pack, id: u32) {
    let e = p.entity(id);
    println!("entity {id}: {}", p.str(e.label));
    if !e.synonyms.is_empty() {
        let syn: Vec<&str> = e.synonyms.iter().map(|x| p.str(*x)).collect();
        println!("  also known as: {}", syn.join("; "));
    }
    if let Some(f) = e.formula {
        println!("  formula {} charge {:?}", p.str(f), e.charge);
    }
    if e.flags.contains(EntityFlags::CURRENCY) {
        println!("  currency metabolite");
    }
    let comps: Vec<_> = e
        .compartments
        .iter()
        .map(|c| p.str(p.compartments[*c as usize].label))
        .collect();
    println!("  compartments: {}", comps.join(", "));
    for x in &e.xrefs {
        let charge = x.form_charge.map(|c| format!(" (charge {c})")).unwrap_or_default();
        let old = if x.secondary { " (secondary)" } else { "" };
        let scope = match x.scope {
            XrefScope::Exact => "",
            XrefScope::Broader => " (broader)",
            XrefScope::Narrower => " (narrower)",
        };
        println!(
            "  {}:{}{charge}{old}{scope} [{}]",
            p.namespace_prefix(x.ns),
            p.str(x.id),
            x.sources.names().join(", ")
        );
    }
    for &g in &e.generalizes_to {
        println!("  is a kind of entity {g}: {}", p.str(p.entity(g).label));
    }
    let reactions = p.reactions_with(id);
    println!("  in {} reactions", reactions.len());
    for &r in reactions.iter().take(8) {
        println!("    {}", equation(p, r));
    }
}

fn show_reaction(p: &Pack, id: ReactionId) {
    let r = p.reaction(id);
    println!("reaction {id}: {}", p.str(r.label));
    if !r.synonyms.is_empty() {
        let syn: Vec<&str> = r.synonyms.iter().map(|x| p.str(*x)).collect();
        println!("  also known as: {}", syn.join("; "));
    }
    println!("  {}", equation(p, id));
    for x in &r.xrefs {
        println!("  {}:{}", p.namespace_prefix(x.ns), p.str(x.id));
    }
}

/// "glc__D_c + atp_c -> g6p_c + adp_c + h_c", using BiGG ids.
fn equation(p: &Pack, id: ReactionId) -> String {
    let r = p.reaction(id);
    let side = |produced: bool| {
        r.participants
            .iter()
            .filter(|x| (x.coef > 0.0) == produced)
            .map(|x| {
                let e = p.entity(x.entity);
                let name = p
                    .entity_xrefs(x.entity, "bigg.metabolite")
                    .next()
                    .map(|b| p.str(b.id))
                    .unwrap_or_else(|| p.str(e.label));
                let comp = x
                    .compartment
                    .map(|c| format!("_{}", p.str(p.compartments[c as usize].code)))
                    .unwrap_or_default();
                let n = x.coef.abs();
                if n == 1.0 {
                    format!("{name}{comp}")
                } else {
                    format!("{n} {name}{comp}")
                }
            })
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let name = p
        .reaction(id)
        .xrefs
        .iter()
        .find(|x| p.namespace_prefix(x.ns) == "bigg.reaction")
        .map(|x| p.str(x.id))
        .unwrap_or("?");
    format!("{name}: {} -> {}", side(false), side(true))
}

//! `annot-cli`: the annotator on the command line.
//!
//! `annot-cli bench` runs the accuracy benchmark (DESIGN.md section 6).

mod bench;
mod input;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use kpack_build::sources::biomodels;

#[derive(Parser)]
#[command(name = "annot-cli", about = "The SBML annotator on the command line")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Grade the engine on curated BioModels it has not learned from.
    Bench {
        /// A pack built with `kpack-build build --exclude-test-models`.
        #[arg(long, default_value = "packs/kpack-train.bin")]
        pack: PathBuf,
        /// The cached curated models (from `kpack-build fetch`).
        #[arg(long, default_value = "data/cache/biomodels")]
        models: PathBuf,
        /// Which models: test (held out from the pack), train, or all.
        #[arg(long, default_value = "test")]
        set: String,
        /// Where to write latest.md and append history.tsv.
        #[arg(long, default_value = "bench")]
        out: PathBuf,
        /// Grade the test set even though the pack has learned its names.
        #[arg(long)]
        allow_leak: bool,
    },
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Bench {
            pack,
            models,
            set,
            out,
            allow_leak,
        } => run_bench(&pack, &models, &set, &out, allow_leak),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_bench(
    pack_path: &std::path::Path,
    models: &std::path::Path,
    set: &str,
    out: &std::path::Path,
    allow_leak: bool,
) -> Result<()> {
    let pack = kpack::decode(&std::fs::read(pack_path).map_err(|e| format!("cannot read {}: {e}", pack_path.display()))?)?;
    let held_out = pack
        .header
        .sources
        .iter()
        .any(|s| s.name == "BioModels" && s.version.contains("test set excluded"));
    if set != "train" && !held_out && !allow_leak {
        return Err(format!(
            "{} has learned the test models' names, so the scores would be inflated. \
             Build one with `kpack-build build --exclude-test-models --out {}`, \
             or pass --allow-leak.",
            pack_path.display(),
            pack_path.display()
        )
        .into());
    }

    let index = models.join(biomodels::INDEX_FILE);
    let ids: Vec<String> = serde_json::from_str(
        &std::fs::read_to_string(&index).map_err(|e| format!("cannot read {}: {e}", index.display()))?,
    )?;
    let chosen: Vec<&String> = ids
        .iter()
        .filter(|id| match set {
            "test" => biomodels::is_test_model(id),
            "train" => !biomodels::is_test_model(id),
            "all" => true,
            _ => false,
        })
        .collect();
    if chosen.is_empty() {
        return Err(format!("no models in set {set:?} (use test, train or all)").into());
    }

    // For the test set, the training models' species names, to tell novel
    // test models from near-twins of training ones.
    let training: Vec<_> = if set == "test" {
        ids.iter()
            .filter(|id| !biomodels::is_test_model(id))
            .filter_map(|id| {
                let xml = std::fs::read_to_string(models.join(format!("{id}.xml"))).ok()?;
                input::read(id, &xml, &pack).ok()
            })
            .map(|g| bench::name_set(&g.model))
            .collect()
    } else {
        Vec::new()
    };

    let mut results = bench::Results::default();
    for id in chosen {
        let Ok(xml) = std::fs::read_to_string(models.join(format!("{id}.xml"))) else {
            results.unreadable.push((id.clone(), "missing".into()));
            continue;
        };
        match input::read(id, &xml, &pack) {
            Ok(g) => {
                let novel = set == "test"
                    && bench::best_overlap(&bench::name_set(&g.model), &training) < bench::NOVEL_OVERLAP;
                bench::grade(&pack, &g, novel, &mut results);
            }
            Err(e) => results.unreadable.push((id.clone(), e)),
        }
    }
    results.species_misses.sort();
    results.reaction_misses.sort();

    let providers: Vec<&str> = annot_engine::providers().iter().map(|p| p.name()).collect();
    let date = kpack_build::date::today();
    let report = bench::report(&results, &pack, set, &date, &providers);
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("latest.md"), &report)?;
    let history = out.join("history.tsv");
    let mut lines = std::fs::read_to_string(&history).unwrap_or_else(|_| format!("{}\n", bench::HISTORY_HEADER));
    // Keep every row; bring an older header up to date.
    if let Some((first, rest)) = lines.split_once('\n')
        && first != bench::HISTORY_HEADER
        && first.starts_with("date\t")
    {
        lines = format!("{}\n{rest}", bench::HISTORY_HEADER);
    }
    lines.push_str(&bench::history_line(&results, &pack, set, &date, &providers));
    lines.push('\n');
    std::fs::write(&history, lines)?;

    let (s, r) = (&results.species, &results.reactions);
    let pct = |n: usize, d: usize| if d == 0 { 0.0 } else { 100.0 * n as f64 / d as f64 };
    println!(
        "{} models ({set} set). Species: {} graded, top-1 {:.1}%, top-3 {:.1}%, coverage {:.1}%. \
         Reactions: {} graded, top-1 {:.1}%, top-3 {:.1}%, coverage {:.1}%.",
        results.models,
        s.gradable,
        pct(s.top1, s.gradable),
        pct(s.top3, s.gradable),
        pct(s.suggested, s.gradable),
        r.gradable,
        pct(r.top1, r.gradable),
        pct(r.top3, r.gradable),
        pct(r.suggested, r.gradable),
    );
    if set == "test" {
        let (ns, nr) = (&results.novel_species, &results.novel_reactions);
        println!(
            "Novel test models only ({} of {}): species top-1 {:.1}% of {}, reactions top-1 {:.1}% of {}.",
            results.novel_models,
            results.models,
            pct(ns.top1, ns.gradable),
            ns.gradable,
            pct(nr.top1, nr.gradable),
            nr.gradable
        );
    }
    println!("report: {}", out.join("latest.md").display());
    Ok(())
}

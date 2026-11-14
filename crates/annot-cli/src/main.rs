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
        /// Also write every graded first suggestion (basis, score, lead,
        /// right or not) as TSV, for calibrating confidence levels.
        #[arg(long)]
        observations: Option<PathBuf>,
        /// Give the engine every species' true identity, as if the user had
        /// confirmed them all, and grade the reactions in that situation.
        #[arg(long)]
        confirmed_species: bool,
        /// Give the engine every reaction's true identity, as if the user had
        /// confirmed them all, and grade the species in that situation.
        #[arg(long)]
        confirmed_reactions: bool,
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
            observations,
            confirmed_species,
            confirmed_reactions,
        } => run_bench(&BenchOptions {
            pack,
            models,
            set,
            out,
            allow_leak,
            observations,
            confirmed_species,
            confirmed_reactions,
        }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What `bench` was asked to do.
struct BenchOptions {
    pack: PathBuf,
    models: PathBuf,
    set: String,
    out: PathBuf,
    allow_leak: bool,
    observations: Option<PathBuf>,
    confirmed_species: bool,
    confirmed_reactions: bool,
}

fn run_bench(o: &BenchOptions) -> Result<()> {
    let (pack_path, models, set, out) = (o.pack.as_path(), o.models.as_path(), o.set.as_str(), o.out.as_path());
    let (allow_leak, observations) = (o.allow_leak, o.observations.as_deref());
    let (confirmed_species, confirmed_reactions) = (o.confirmed_species, o.confirmed_reactions);
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
            Ok(mut g) => {
                if confirmed_species {
                    confirm_species(&mut g);
                }
                if confirmed_reactions {
                    confirm_reactions(&mut g);
                }
                let novel = set == "test"
                    && bench::best_overlap(&bench::name_set(&g.model), &training) < bench::NOVEL_OVERLAP;
                bench::grade(&pack, &g, novel, &mut results);
            }
            Err(e) => results.unreadable.push((id.clone(), e)),
        }
    }
    if let Some(path) = observations {
        let mut t = String::from("kind\tnovel\tbasis\tscore\tratio\tright\tlevel\n");
        for o in &results.observations {
            t.push_str(&format!(
                "{}\t{}\t{}\t{:.3}\t{:.3}\t{}\t{}\n",
                if o.reaction { "reaction" } else { "species" },
                o.novel,
                format!("{:?}", o.basis).to_lowercase(),
                o.score,
                o.ratio,
                o.right,
                o.level.label()
            ));
        }
        std::fs::write(path, t)?;
        // Every miss, with the first suggestion's evidence, beside it.
        let mut m = String::from("kind\tmodel\telement\tname\ttruth\tfirst\tlevel\twhy\n");
        for (reaction, list) in [(false, &results.species_misses), (true, &results.reaction_misses)] {
            for x in list {
                m.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    if reaction { "reaction" } else { "species" },
                    x.model,
                    x.element,
                    x.name.as_deref().unwrap_or(""),
                    x.truth,
                    x.first.as_deref().unwrap_or(""),
                    x.level.map_or("-", |l| l.label()),
                    x.why
                ));
            }
        }
        std::fs::write(path.with_extension("misses.tsv"), m)?;
    }
    results.species_misses.sort();
    results.reaction_misses.sort();

    let providers: Vec<&str> = annot_engine::providers().iter().map(|p| p.name()).collect();
    let date = kpack_build::date::today();
    let mut scenario = set.to_string();
    if confirmed_species {
        scenario.push_str("+confirmed-species");
    }
    if confirmed_reactions {
        scenario.push_str("+confirmed-reactions");
    }
    let set = scenario.as_str();
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
    if set.starts_with("test") {
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

/// Tells the engine each species' identity from the answer key, as a user
/// confirming them would. Only unambiguous answers are given.
fn confirm_species(g: &mut input::Graded) {
    for (i, key) in g.species.iter().enumerate() {
        if let input::Key::Known { accept, tie: false } = key
            && let [kpack::Target::Entity(e)] = accept.as_slice()
        {
            g.model.species[i].known = Some(*e);
        }
    }
}

/// Tells the engine each reaction's identity from the answer key, as a user
/// confirming them would. Only unambiguous answers are given.
fn confirm_reactions(g: &mut input::Graded) {
    for (i, key) in g.reactions.iter().enumerate() {
        if let input::Key::Known { accept, tie: false } = key
            && let [kpack::Target::Reaction(r)] = accept.as_slice()
        {
            g.model.reactions[i].known = Some(*r);
        }
    }
}

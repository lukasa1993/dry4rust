mod cargo_proxy;

use clap::Parser;
use dry4rust::{find_duplicates, Duplicate, Error, VERSION};
use serde::Serialize;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "dry4rust", version = VERSION, about = "Native duplicate-code analysis for Rust")]
struct Args {
    #[arg(value_name = "PATH_FRAGMENT")]
    filters: Vec<String>,
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Cargo features to enable for DRY source scope.
    #[arg(long, value_delimiter = ',', conflicts_with = "all_features")]
    features: Vec<String>,
    /// Disable Cargo default features. May be combined with --features.
    #[arg(long, conflicts_with = "all_features")]
    no_default_features: bool,
    /// Enable every Cargo feature. Fails normally if the project forbids that combination.
    #[arg(long)]
    all_features: bool,
    #[arg(long, default_value_t = 30)]
    min_tokens: usize,
    #[arg(long, default_value_t = 50)]
    max_groups: usize,
    #[arg(long, default_value_t = 100)]
    max_occurrences_per_window: usize,
    #[arg(long)]
    include_tests: bool,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    fail: bool,
}

#[derive(Serialize)]
struct Report<'a> {
    schema_version: u8,
    tool: &'static str,
    version: &'static str,
    root: String,
    summary: Summary,
    duplicates: &'a [Duplicate],
}
#[derive(Serialize)]
struct Summary {
    groups: usize,
    min_tokens: usize,
}

fn run() -> Result<u8, Error> {
    let args = Args::parse();
    let root = args.root.canonicalize()?;
    let cargo_args = cargo_proxy::feature_args(
        &args.features,
        args.all_features,
        args.no_default_features,
    );
    let _cargo_proxy = cargo_proxy::install(&root, "dry4rust", &cargo_args)?;
    let duplicates = find_duplicates(
        &root,
        args.min_tokens,
        args.max_groups,
        args.max_occurrences_per_window,
        args.include_tests,
        &args.filters,
    )?;
    if args.json {
        let report = Report {
            schema_version: 1,
            tool: "dry4rust",
            version: VERSION,
            root: root.to_string_lossy().to_string(),
            summary: Summary {
                groups: duplicates.len(),
                min_tokens: args.min_tokens,
            },
            duplicates: &duplicates,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("serializable report")
        );
    } else {
        println!("DRY Report\n==========");
        if duplicates.is_empty() {
            println!("No duplicate groups found.");
        }
        for duplicate in &duplicates {
            let locations = duplicate
                .locations
                .iter()
                .map(|item| format!("{}:{}-{}", item.file, item.start_line, item.end_line))
                .collect::<Vec<_>>()
                .join(" <-> ");
            println!("{} tokens: {locations}", duplicate.token_count);
        }
    }
    Ok(if args.fail && !duplicates.is_empty() {
        2
    } else {
        0
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("dry4rust: {error}");
            ExitCode::from(1)
        }
    }
}

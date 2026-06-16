use ballast_digest::{build_digest, load_events, load_survey, load_trend, DigestInputs};
use chrono::DateTime;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "ballast-digest",
    about = "Synthesize disk health from survey + trend + guard events into a ranked digest block",
    version
)]
struct Args {
    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Number of top reclaimable entries to show
    #[arg(long, default_value = "10")]
    top_k: usize,

    /// Override current time for deterministic tests (RFC3339)
    #[arg(long)]
    now: Option<String>,

    /// Override survey JSON source (path to file)
    #[arg(long)]
    survey_json: Option<PathBuf>,

    /// Override trend JSON source (path to file)
    #[arg(long)]
    trend_json: Option<PathBuf>,

    /// Override guard events JSONL file path
    #[arg(long)]
    events_file: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    let now = args.now.as_deref().and_then(|s| {
        s.parse::<DateTime<chrono::Utc>>().ok()
    });

    let survey = load_survey(args.survey_json.as_deref());
    let trend = load_trend(args.trend_json.as_deref());
    let events = load_events(args.events_file.as_deref());

    let digest = build_digest(DigestInputs {
        survey,
        trend,
        events,
        now,
        top_k: args.top_k,
    });

    if args.json {
        match serde_json::to_string_pretty(&digest.json) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("error serializing JSON: {e}");
                std::process::exit(1);
            }
        }
    } else {
        println!("{}", digest.human);
    }
}

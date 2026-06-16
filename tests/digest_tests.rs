use ballast_digest::{
    build_digest, parse_events, parse_survey, parse_trend, rank_entries, DigestInputs,
    ReclaimEntry,
};
use chrono::DateTime;
use std::path::Path;

fn fixtures_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures"))
}

fn survey_full() -> ballast_digest::SurveyOutput {
    let text = std::fs::read_to_string(fixtures_dir().join("survey_full.json")).unwrap();
    parse_survey(&text).unwrap()
}

fn trend_full() -> ballast_digest::TrendOutput {
    let text = std::fs::read_to_string(fixtures_dir().join("trend_full.json")).unwrap();
    parse_trend(&text).unwrap()
}

fn trend_empty() -> ballast_digest::TrendOutput {
    let text = std::fs::read_to_string(fixtures_dir().join("trend_empty.json")).unwrap();
    parse_trend(&text).unwrap()
}

fn events_full() -> Vec<ballast_digest::GuardEvent> {
    let text = std::fs::read_to_string(fixtures_dir().join("events_full.jsonl")).unwrap();
    parse_events(&text).unwrap()
}

const NOW: &str = "2026-06-16T12:00:00Z";

fn now() -> DateTime<chrono::Utc> {
    NOW.parse().unwrap()
}

// AC1: All inputs present → block with headline, top-K ranked list, flow/ETA line, reclaimed-bytes line
#[test]
fn ac1_all_inputs_present() {
    let digest = build_digest(DigestInputs {
        survey: Some(survey_full()),
        trend: Some(trend_full()),
        events: Some(events_full()),
        now: Some(now()),
        top_k: 10,
    });

    let human = &digest.human;
    // Must have headline with SLO band
    assert!(
        human.contains("SLO="),
        "headline must contain SLO= but got:\n{human}"
    );
    // Must have DISK keyword
    assert!(
        human.contains("DISK"),
        "headline must contain DISK but got:\n{human}"
    );
    // Must have flow line
    assert!(
        human.contains("fastest-growing:"),
        "must have flow line but got:\n{human}"
    );
    // Must have ledger line
    assert!(
        human.contains("reclaimed last 24h:"),
        "must have ledger line but got:\n{human}"
    );
    // Must have ranked entries (fossil/stale/warm)
    assert!(
        human.contains("fossil") || human.contains("stale") || human.contains("warm"),
        "must have classified entries but got:\n{human}"
    );
    // JSON output must be parseable
    let json_str = serde_json::to_string(&digest.json).unwrap();
    assert!(!json_str.is_empty());
}

// AC2: Trend absent → "no trend snapshots yet" in flow line
#[test]
fn ac2_trend_absent_explicit_message() {
    let digest = build_digest(DigestInputs {
        survey: Some(survey_full()),
        trend: None,
        events: Some(events_full()),
        now: Some(now()),
        top_k: 10,
    });

    let human = &digest.human;
    assert!(
        human.contains("no trend snapshots yet"),
        "flow line must say 'no trend snapshots yet' when trend absent, got:\n{human}"
    );
    // Must still render (exit 0 implied by not panicking)
}

// AC2b: Trend present but empty paths → also says "no trend snapshots yet"
#[test]
fn ac2b_trend_empty_paths() {
    let digest = build_digest(DigestInputs {
        survey: Some(survey_full()),
        trend: Some(trend_empty()),
        events: Some(events_full()),
        now: Some(now()),
        top_k: 10,
    });

    let human = &digest.human;
    assert!(
        human.contains("no trend snapshots yet"),
        "flow line must say 'no trend snapshots yet' for empty paths, got:\n{human}"
    );
}

// AC3: Guard events absent → "no guard events yet"
#[test]
fn ac3_events_absent() {
    let digest = build_digest(DigestInputs {
        survey: Some(survey_full()),
        trend: Some(trend_full()),
        events: None,
        now: Some(now()),
        top_k: 10,
    });

    let human = &digest.human;
    assert!(
        human.contains("no guard events yet"),
        "ledger must say 'no guard events yet' when events absent, got:\n{human}"
    );
}

// AC4: Reclaimable entries ranked safest-biggest-first (size × reap_safety)
#[test]
fn ac4_ranking_safest_biggest_first() {
    let entries = vec![
        ReclaimEntry {
            path: "a".to_string(),
            size_bytes: 1000,
            reap_safety: 0.5,
            class: "warm".to_string(),
            growth_rate_bytes_per_day: None,
            eta_to_high_water_days: None,
        },
        ReclaimEntry {
            path: "b".to_string(),
            size_bytes: 500,
            reap_safety: 0.9,
            class: "fossil".to_string(),
            growth_rate_bytes_per_day: None,
            eta_to_high_water_days: None,
        },
        ReclaimEntry {
            path: "c".to_string(),
            size_bytes: 2000,
            reap_safety: 0.8,
            class: "stale".to_string(),
            growth_rate_bytes_per_day: None,
            eta_to_high_water_days: None,
        },
    ];
    // Scores: a=500, b=450, c=1600  → order should be c, a, b
    let ranked = rank_entries(entries);
    assert_eq!(ranked[0].path, "c", "highest score (c=1600) should be first");
    assert_eq!(ranked[1].path, "a", "second score (a=500) should be second");
    assert_eq!(ranked[2].path, "b", "lowest score (b=450) should be last");

    // Labels must be preserved
    assert_eq!(ranked[0].class, "stale");
    assert_eq!(ranked[1].class, "warm");
    assert_eq!(ranked[2].class, "fossil");
}

// AC4b: In the full digest, classes from survey are preserved in output
#[test]
fn ac4b_class_labels_in_output() {
    let digest = build_digest(DigestInputs {
        survey: Some(survey_full()),
        trend: None,
        events: None,
        now: Some(now()),
        top_k: 10,
    });

    let human = &digest.human;
    assert!(
        human.contains("fossil"),
        "output must include 'fossil' class label, got:\n{human}"
    );
}

// AC5: --json output is stable (fields present, parseable)
#[test]
fn ac5_json_output_stable() {
    let digest = build_digest(DigestInputs {
        survey: Some(survey_full()),
        trend: Some(trend_full()),
        events: Some(events_full()),
        now: Some(now()),
        top_k: 5,
    });

    let json = &digest.json;
    // headline fields
    assert!(!json.headline.slo_band.is_empty());
    // top_entries capped at top_k
    assert!(json.top_entries.len() <= 5);
    // each entry has score
    for entry in &json.top_entries {
        assert!(entry.score > 0.0 || entry.reap_safety == 0.0);
        assert!(!entry.path.is_empty());
        assert!(!entry.class.is_empty());
    }
    // flow and ledger present
    assert!(!json.flow.status.is_empty());
    assert!(!json.ledger.status.is_empty());

    // Serialize/deserialize round-trip check
    let s = serde_json::to_string_pretty(json).unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v.get("headline").is_some());
    assert!(v.get("top_entries").is_some());
    assert!(v.get("flow").is_some());
    assert!(v.get("ledger").is_some());
}

// AC6: Tool is read-only — no deletions or scheduling
// (structural test: no std::fs::remove* calls in lib.rs)
#[test]
fn ac6_read_only_no_deletion() {
    let lib_src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
    assert!(
        !lib_src.contains("remove_file")
            && !lib_src.contains("remove_dir")
            && !lib_src.contains("std::fs::remove"),
        "lib.rs must not contain any deletion calls"
    );
    let main_src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"));
    assert!(
        !main_src.contains("remove_file")
            && !main_src.contains("remove_dir")
            && !main_src.contains("std::fs::remove"),
        "main.rs must not contain any deletion calls"
    );
}

// AC7: cargo test green with fixed --now (no wall-clock in verdict logic)
// Verified by: all tests use `now()` fixture; library never calls Utc::now() in verdict path
// This test confirms the `--now` path is exercised
#[test]
fn ac7_deterministic_with_fixed_now() {
    let t1 = build_digest(DigestInputs {
        survey: Some(survey_full()),
        trend: Some(trend_full()),
        events: Some(events_full()),
        now: Some(now()),
        top_k: 10,
    });
    let t2 = build_digest(DigestInputs {
        survey: Some(survey_full()),
        trend: Some(trend_full()),
        events: Some(events_full()),
        now: Some(now()),
        top_k: 10,
    });
    assert_eq!(
        t1.human, t2.human,
        "output must be deterministic with fixed --now"
    );
}

// Bonus: graceful on all-absent inputs
#[test]
fn all_absent_does_not_panic() {
    let digest = build_digest(DigestInputs {
        survey: None,
        trend: None,
        events: None,
        now: Some(now()),
        top_k: 10,
    });
    assert!(digest.human.contains("DISK"));
    assert!(digest.human.contains("no guard events yet"));
    assert!(digest.human.contains("no trend snapshots yet"));
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── Survey types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct SurveyPath {
    pub path: String,
    pub size_bytes: u64,
    pub class: String,
    #[serde(default)]
    pub reap_safety: f64,
}

#[derive(Debug, Deserialize)]
pub struct SurveyOutput {
    pub paths: Vec<SurveyPath>,
}

// ─── Trend types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct TrendPath {
    pub path: String,
    pub bytes_per_day: f64,
    pub eta_to_high_water_days: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct TrendOutput {
    #[serde(default)]
    pub paths: Vec<TrendPath>,
}

// ─── Guard event types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct GuardEvent {
    pub ts: String,
    #[serde(default)]
    pub usage_pct: u8,
    #[serde(default)]
    pub slo_band: String,
    #[serde(default)]
    pub exit_code: i32,
    #[serde(default)]
    pub reclaimed_bytes: u64,
}

// ─── Digest types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ReclaimEntry {
    pub path: String,
    pub size_bytes: u64,
    pub reap_safety: f64,
    pub class: String,
    pub growth_rate_bytes_per_day: Option<f64>,
    pub eta_to_high_water_days: Option<f64>,
}

impl ReclaimEntry {
    pub fn score(&self) -> f64 {
        self.size_bytes as f64 * self.reap_safety
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ReclaimEntryJson {
    pub path: String,
    pub size_bytes: u64,
    pub reap_safety: f64,
    pub class: String,
    pub growth_rate_bytes_per_day: Option<f64>,
    pub eta_to_high_water_days: Option<f64>,
    pub score: f64,
}

#[derive(Debug, Serialize)]
pub struct DigestJson {
    pub headline: HeadlineJson,
    pub top_entries: Vec<ReclaimEntryJson>,
    pub flow: FlowJson,
    pub ledger: LedgerJson,
}

#[derive(Debug, Serialize)]
pub struct HeadlineJson {
    pub usage_pct: Option<u8>,
    pub free_bytes: Option<u64>,
    pub slo_band: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct FlowJson {
    pub fastest_path: Option<String>,
    pub bytes_per_day: Option<f64>,
    pub eta_to_high_water_days: Option<f64>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct LedgerJson {
    pub reclaimed_bytes_24h: u64,
    pub status: String,
}

// ─── Parsers ──────────────────────────────────────────────────────────────────

pub fn parse_survey(text: &str) -> Result<SurveyOutput, String> {
    serde_json::from_str(text).map_err(|e| format!("survey parse error: {e}"))
}

pub fn parse_trend(text: &str) -> Result<TrendOutput, String> {
    serde_json::from_str(text).map_err(|e| format!("trend parse error: {e}"))
}

pub fn parse_events(text: &str) -> Result<Vec<GuardEvent>, String> {
    let mut events = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<GuardEvent>(line) {
            Ok(ev) => events.push(ev),
            Err(e) => eprintln!("warn: guard event line {i} skipped: {e}"),
        }
    }
    Ok(events)
}

// ─── Ranking ──────────────────────────────────────────────────────────────────

pub fn rank_entries(mut entries: Vec<ReclaimEntry>) -> Vec<ReclaimEntry> {
    entries.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(std::cmp::Ordering::Equal));
    entries
}

fn bytes_human(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if b >= GB {
        format!("{:.1}G", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.1}M", b as f64 / MB as f64)
    } else if b >= KB {
        format!("{:.1}K", b as f64 / KB as f64)
    } else {
        format!("{b}B")
    }
}

// ─── Digest builder ───────────────────────────────────────────────────────────

pub struct DigestInputs {
    pub survey: Option<SurveyOutput>,
    pub trend: Option<TrendOutput>,
    pub events: Option<Vec<GuardEvent>>,
    pub now: Option<DateTime<Utc>>,
    pub top_k: usize,
}

pub struct DigestOutput {
    pub human: String,
    pub json: DigestJson,
}

pub fn build_digest(inputs: DigestInputs) -> DigestOutput {
    // ── Headline ──────────────────────────────────────────────────────────────
    let (usage_pct, free_bytes, slo_band, headline_source) =
        if let Some(events) = &inputs.events {
            // Use most recent guard event for headline
            if let Some(last) = events.last() {
                let band = if last.slo_band.is_empty() {
                    "unknown".to_string()
                } else {
                    last.slo_band.clone()
                };
                (Some(last.usage_pct), None::<u64>, band, "guard-events")
            } else {
                (None, None, "unknown".to_string(), "no-data")
            }
        } else {
            (None, None, "unknown".to_string(), "no-data")
        };

    let headline_str = match (usage_pct, free_bytes) {
        (Some(pct), Some(free)) => {
            format!("DISK {}% free={} SLO={}", pct, bytes_human(free), slo_band)
        }
        (Some(pct), None) => {
            format!("DISK {}% SLO={}", pct, slo_band)
        }
        _ => format!("DISK SLO={}", slo_band),
    };

    // ── Build ranked entries ──────────────────────────────────────────────────
    let trend_map: std::collections::HashMap<String, &TrendPath> = inputs
        .trend
        .as_ref()
        .map(|t| t.paths.iter().map(|p| (p.path.clone(), p)).collect())
        .unwrap_or_default();

    let entries: Vec<ReclaimEntry> = inputs
        .survey
        .as_ref()
        .map(|s| {
            s.paths
                .iter()
                .map(|p| {
                    let trend_info = trend_map.get(&p.path);
                    ReclaimEntry {
                        path: p.path.clone(),
                        size_bytes: p.size_bytes,
                        reap_safety: p.reap_safety,
                        class: p.class.clone(),
                        growth_rate_bytes_per_day: trend_info.map(|t| t.bytes_per_day),
                        eta_to_high_water_days: trend_info.and_then(|t| t.eta_to_high_water_days),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let ranked = rank_entries(entries);
    let top_k: Vec<_> = ranked.into_iter().take(inputs.top_k).collect();

    // ── Flow line ─────────────────────────────────────────────────────────────
    let (flow_str, flow_path, flow_bpd, flow_eta) =
        if let Some(trend) = &inputs.trend {
            if trend.paths.is_empty() {
                (
                    "fastest-growing: no trend snapshots yet".to_string(),
                    None,
                    None,
                    None,
                )
            } else {
                // Find fastest growing path
                let fastest = trend
                    .paths
                    .iter()
                    .max_by(|a, b| a.bytes_per_day.partial_cmp(&b.bytes_per_day).unwrap_or(std::cmp::Ordering::Equal));
                if let Some(f) = fastest {
                    let eta_str = match f.eta_to_high_water_days {
                        Some(d) => format!(", ETA to 90%: {:.0} days", d),
                        None => String::new(),
                    };
                    let bpd = bytes_human(f.bytes_per_day as u64);
                    let s = format!(
                        "fastest-growing: {} at +{}/day{}",
                        f.path, bpd, eta_str
                    );
                    (s, Some(f.path.clone()), Some(f.bytes_per_day), f.eta_to_high_water_days)
                } else {
                    ("fastest-growing: stable".to_string(), None, None, None)
                }
            }
        } else {
            (
                "fastest-growing: no trend snapshots yet".to_string(),
                None,
                None,
                None,
            )
        };

    // ── Ledger line ───────────────────────────────────────────────────────────
    let (ledger_str, reclaimed_24h) = if let Some(events) = &inputs.events {
        if events.is_empty() {
            ("reclaimed last 24h: no guard events yet".to_string(), 0u64)
        } else {
            // Sum reclaimed_bytes from events within ~24h window
            let cutoff = inputs.now.unwrap_or_else(Utc::now)
                - chrono::Duration::hours(24);
            let reclaimed: u64 = events
                .iter()
                .filter(|e| {
                    e.ts.parse::<DateTime<Utc>>()
                        .map(|t| t >= cutoff)
                        .unwrap_or(false)
                })
                .map(|e| e.reclaimed_bytes)
                .sum();
            let s = if reclaimed == 0 {
                "reclaimed last 24h: 0B (no guard events yet)".to_string()
            } else {
                format!("reclaimed last 24h: {}", bytes_human(reclaimed))
            };
            (s, reclaimed)
        }
    } else {
        ("reclaimed last 24h: no guard events yet".to_string(), 0u64)
    };

    // ── Human output ──────────────────────────────────────────────────────────
    let mut lines = vec![headline_str.clone()];
    for entry in &top_k {
        let growth = match entry.growth_rate_bytes_per_day {
            Some(r) => format!(" +{}/day", bytes_human(r as u64)),
            None => String::new(),
        };
        lines.push(format!(
            "  {:>8}  {:.2}  {:8}  {}{}",
            bytes_human(entry.size_bytes),
            entry.reap_safety,
            entry.class,
            entry.path,
            growth,
        ));
    }
    if top_k.is_empty() {
        lines.push("  (no reclaimable entries)".to_string());
    }
    lines.push(flow_str);
    lines.push(ledger_str);

    let human = lines.join("\n");

    // ── JSON output ───────────────────────────────────────────────────────────
    let json = DigestJson {
        headline: HeadlineJson {
            usage_pct,
            free_bytes,
            slo_band,
            source: headline_source.to_string(),
        },
        top_entries: top_k
            .iter()
            .map(|e| ReclaimEntryJson {
                path: e.path.clone(),
                size_bytes: e.size_bytes,
                reap_safety: e.reap_safety,
                class: e.class.clone(),
                growth_rate_bytes_per_day: e.growth_rate_bytes_per_day,
                eta_to_high_water_days: e.eta_to_high_water_days,
                score: e.score(),
            })
            .collect(),
        flow: FlowJson {
            fastest_path: flow_path,
            bytes_per_day: flow_bpd,
            eta_to_high_water_days: flow_eta,
            status: if flow_bpd.is_some() { "growing".to_string() } else { "no-data".to_string() },
        },
        ledger: LedgerJson {
            reclaimed_bytes_24h: reclaimed_24h,
            status: if inputs.events.is_some() { "ok".to_string() } else { "absent".to_string() },
        },
    };

    DigestOutput { human, json }
}

// ─── Source loading ───────────────────────────────────────────────────────────

pub fn load_survey(
    survey_json_override: Option<&Path>,
) -> Option<SurveyOutput> {
    let text = if let Some(p) = survey_json_override {
        std::fs::read_to_string(p).ok()?
    } else {
        // Run ballast-survey --json
        let out = std::process::Command::new("ballast-survey")
            .arg("--json")
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()?
    };
    parse_survey(&text).ok()
}

pub fn load_trend(
    trend_json_override: Option<&Path>,
) -> Option<TrendOutput> {
    let text = if let Some(p) = trend_json_override {
        std::fs::read_to_string(p).ok()?
    } else {
        let out = std::process::Command::new("ballast-trend")
            .args(["report", "--json"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()?
    };
    parse_trend(&text).ok()
}

pub fn load_events(
    events_file_override: Option<&Path>,
) -> Option<Vec<GuardEvent>> {
    let path = if let Some(p) = events_file_override {
        p.to_path_buf()
    } else {
        let home = std::env::var("HOME").ok()?;
        std::path::PathBuf::from(home)
            .join(".local/state/ballast/guard-events.jsonl")
    };
    let text = std::fs::read_to_string(&path).ok()?;
    parse_events(&text).ok()
}

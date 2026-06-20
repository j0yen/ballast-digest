# ballast-digest

One disk-health block from three sources: what is big (survey), what is growing (trend), and what the guard has been doing (events) — ranked, with the question "what should I reclaim first" answered at the top.

## Why it exists

The other ballast tools each answer one question. `ballast-survey` says what is big right now; `ballast-trend` says what is growing and how fast; `ballast-guard` logs whether usage crossed an SLO band and how much it reclaimed. Read separately, they are three files in three formats. `ballast-digest` joins them into one ranked block you can read in a glance or paste into a journal.

The ranking is the point. A path that is large *and* safe to reclaim should sit above one that is large but risky. So entries are scored `size_bytes × reap_safety` and sorted descending — big-and-safe first, big-and-risky last. The growth rate and ETA from trend ride along on each row so a path that is both large and accelerating is visible as one fact.

`ballast-digest` reads, ranks, and prints. It never walks the filesystem and never deletes anything.

## Install

```sh
cargo install --path .
```

This installs the `ballast-digest` binary.

## Quickstart

With no flags, the digest gathers its own inputs: it runs `ballast-survey --json`, runs `ballast-trend report --json`, and reads guard events from `~/.local/state/ballast/guard-events.jsonl`. Any source that is missing degrades to a plain "no data yet" line rather than failing.

```sh
ballast-digest
```

To run it on fixed inputs — for a test, a demo, or a one-off over captured files — point each source at a file:

```sh
ballast-digest \
  --survey-json fixtures/survey_full.json \
  --trend-json  fixtures/trend_full.json \
  --events-file fixtures/events_full.jsonl \
  --now 2026-06-16T12:00:00Z
```

```text
DISK 94% SLO=warn
      2.0G  0.95  fossil    /home/jsy/wintermute/recall/target +500.0M/day
      1.0G  0.80  stale     /home/jsy/wintermute/drydock/target
    800.0M  0.90  fossil    /home/jsy/wintermute/brain/target +100.0M/day
    500.0M  0.50  warm      /home/jsy/.cache/pip
fastest-growing: /home/jsy/wintermute/recall/target at +500.0M/day, ETA to 90%: 3 days
reclaimed last 24h: 2.0G
```

Add `--json` for the machine-readable form (headline, ranked `top_entries` with scores, flow, ledger). Use `--top-k N` to change how many reclaim rows print (default 10).

## How to read the block

| Line | Source | What it tells you |
|------|--------|-------------------|
| Headline `DISK …% SLO=…` | latest guard event | current usage and SLO band |
| Ranked rows | survey × trend | size, `reap_safety`, class, path, and growth rate per reclaimable subtree |
| `fastest-growing:` | trend | the path adding bytes quickest, with ETA to the high-water mark |
| `reclaimed last 24h:` | guard events | bytes the guard has freed in the trailing 24 hours |

Rows are ranked by `size_bytes × reap_safety`: large and safe sorts above large and risky. The growth rate (`+.../day`) appears only when trend has a rate for that path — a path with one snapshot gets no fabricated number.

## Flags

```text
--json                 emit structured JSON instead of the text block
--top-k <N>            number of ranked reclaim rows (default 10)
--now <RFC3339>        reference time for the 24h reclaimed window (deterministic tests)
--survey-json <FILE>   read survey from a file instead of running ballast-survey
--trend-json <FILE>    read trend from a file instead of running ballast-trend
--events-file <FILE>   read guard events from this path instead of the default
```

## Part of the ballast fleet

A family of read-mostly disk-health tools for the wintermute workspace. `ballast-digest` is the read layer on top of the other three.

| Tool | Job |
|------|-----|
| [`ballast-survey`](https://github.com/j0yen/ballast-survey) | Measure what is big right now |
| [`ballast-trend`](https://github.com/j0yen/ballast-trend) | Measure what is growing and how fast |
| [`ballast-guard`](https://github.com/j0yen/ballast-guard) | Watch usage against an SLO; log events; reclaim on opt-in |
| [`ballast-pilot`](https://github.com/j0yen/ballast-pilot) | Wire the guard to an hourly systemd timer |
| **`ballast-digest`** | Synthesize survey + trend + events into one ranked block ← you are here |

## Status

v0.1.0. The synthesis, ranking, and both output forms work and are covered by tests against the bundled `fixtures/`. The default no-flag path depends on `ballast-survey` and `ballast-trend` being on `PATH` and on the guard event log existing; any absent source degrades to a "no data yet" line.

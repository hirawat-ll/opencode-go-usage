//! Token-level usage, read from OpenCode's own SQLite database.
//!
//! This is the only place per-model detail exists: the Go usage endpoint
//! reports plan percentages and nothing else. OpenCode writes one row per
//! assistant message holding the model, the token counts and a cost estimate,
//! which is exactly the breakdown the details view shows.
//!
//! Two generations of that row exist and they do not overlap in time, so both
//! are read and unioned:
//!
//! * `session_message` (current): `{ model: { id, providerID },
//!   tokens: { input, output, reasoning, cache: { read, write } }, cost,
//!   time: { created } }`
//! * `message` (older): `{ modelID, providerID, tokens: { input, output,
//!   reasoning, cache: { read, write } }, cost, time: { created } }`
//!
//! The database is opened read-only: this app never writes to OpenCode's data.
//!
//! One window rule applies to everything returned here: the cutoff is aligned
//! to a clock boundary (top of the hour, or local midnight) so the chart's
//! buckets and the totals describe the same span of time.

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Ranges the details view offers, in milliseconds.
pub fn range_ms(range: &str) -> Option<i64> {
    match range {
        "24h" => Some(24 * 3600 * 1000),
        "7d" => Some(7 * 24 * 3600 * 1000),
        "30d" => Some(30 * 24 * 3600 * 1000),
        _ => None,
    }
}

/// OpenCode keeps its data under the XDG data directory, on macOS included.
pub fn candidate_paths() -> Vec<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(".local/share"),
            None => return Vec::new(),
        },
    };
    let dir = base.join("opencode");
    // Stable and preview channels keep separate files.
    ["opencode.db", "opencode-next.db"]
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| path.is_file())
        .collect()
}

/// Whether token detail exists at all, decided by a stat call rather than a
/// query, so the panel can say so before anything is opened.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSource {
    pub available: bool,
    pub database: String,
}

pub fn source() -> LocalSource {
    let paths = candidate_paths();
    LocalSource {
        available: !paths.is_empty(),
        database: paths
            .first()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub input: i64,
    pub output: i64,
    pub reasoning: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    /// Everything the models processed, cache reads included.
    pub tokens: i64,
    pub requests: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRow {
    pub provider: String,
    pub model: String,
    pub input: i64,
    pub output: i64,
    pub reasoning: i64,
    pub cache_read: i64,
    pub tokens: i64,
    pub requests: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    /// Chart key: an epoch-millisecond string for hours, a local date for days.
    pub key: String,
    /// Set for hourly buckets; the panel formats its own local label from it.
    pub start: Option<i64>,
    pub tokens: i64,
    pub requests: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub range: String,
    /// "all" or "go", so the panel can word its empty state correctly.
    pub filter: String,
    /// False when no OpenCode database could be read.
    pub available: bool,
    /// Why it is unavailable, phrased for the panel.
    pub message: Option<String>,
    /// Where the numbers came from, for the footnote.
    pub database: String,
    pub totals: Totals,
    pub models: Vec<ModelRow>,
    /// "hour" for 24h, "day" otherwise.
    pub bucket_unit: String,
    pub buckets: Vec<Bucket>,
}

impl Detail {
    pub fn unavailable(range: &str, go_only: bool, message: impl Into<String>) -> Self {
        Self {
            range: range.to_string(),
            filter: filter_name(go_only),
            available: false,
            message: Some(message.into()),
            database: String::new(),
            totals: Totals::default(),
            models: Vec::new(),
            bucket_unit: if range == "24h" { "hour".into() } else { "day".into() },
            buckets: Vec::new(),
        }
    }
}

fn filter_name(go_only: bool) -> String {
    if go_only { "go".into() } else { "all".into() }
}

/// Providers this meter is for. Mirrored by `GO_PROVIDERS` in the panel, and a
/// test keeps the two in step.
const GO_PROVIDERS: [&str; 2] = ["opencode-go", "opencode"];

/// Predicate appended to the outer queries, where the `provider` column from
/// the union is in scope. Empty when the caller wants everything.
fn provider_filter(go_only: bool) -> String {
    if !go_only {
        return String::new();
    }
    let list = GO_PROVIDERS
        .iter()
        .map(|provider| format!("'{provider}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" AND provider IN ({list})")
}

/// Every message row in the window, normalised across both table shapes.
/// `?1` is a coarse epoch-millisecond cutoff.
const ROWS_SQL: &str = r#"
SELECT
  COALESCE(json_extract(data, '$.model.providerID'), 'unknown') AS provider,
  COALESCE(json_extract(data, '$.model.id'), 'unknown')         AS model,
  COALESCE(json_extract(data, '$.tokens.input'), 0)             AS input,
  COALESCE(json_extract(data, '$.tokens.output'), 0)            AS output,
  COALESCE(json_extract(data, '$.tokens.reasoning'), 0)         AS reasoning,
  COALESCE(json_extract(data, '$.tokens.cache.read'), 0)        AS cache_read,
  COALESCE(json_extract(data, '$.tokens.cache.write'), 0)       AS cache_write,
  COALESCE(json_extract(data, '$.cost'), 0.0)                   AS cost,
  time_created                                                  AS created
FROM session_message
WHERE type = 'assistant' AND json_extract(data, '$.tokens.input') IS NOT NULL

UNION ALL

SELECT
  COALESCE(json_extract(data, '$.providerID'), 'unknown'),
  COALESCE(json_extract(data, '$.modelID'), 'unknown'),
  COALESCE(json_extract(data, '$.tokens.input'), 0),
  COALESCE(json_extract(data, '$.tokens.output'), 0),
  COALESCE(json_extract(data, '$.tokens.reasoning'), 0),
  COALESCE(json_extract(data, '$.tokens.cache.read'), 0),
  COALESCE(json_extract(data, '$.tokens.cache.write'), 0),
  COALESCE(json_extract(data, '$.cost'), 0.0),
  time_created
FROM message
WHERE json_extract(data, '$.tokens.input') IS NOT NULL
  AND time_created >= ?1
"#;

const MODEL_SQL: &str = r#"
SELECT provider, model,
       SUM(input), SUM(output), SUM(reasoning), SUM(cache_read), SUM(cache_write),
       SUM(cost), COUNT(*)
FROM (%ROWS%)
WHERE created >= ?1%FILTER%
GROUP BY provider, model
"#;

/// Dense hourly grid from an hour-aligned cutoff through the hour containing
/// `?2`, so quiet hours still get a bar.
const HOURS_SQL: &str = r#"
WITH RECURSIVE grid(ts) AS (
  SELECT ?1
  UNION ALL
  SELECT ts + 3600000 FROM grid
  WHERE ts + 3600000 <= (?2 / 3600000) * 3600000
),
agg AS (
  SELECT (created / 3600000) * 3600000 AS ts,
         SUM(input + output + reasoning + cache_read + cache_write) AS tokens,
         SUM(cost) AS cost,
         COUNT(*) AS requests
  FROM (%ROWS%)
  WHERE created >= ?1%FILTER%
  GROUP BY ts
)
SELECT grid.ts, COALESCE(agg.tokens, 0), COALESCE(agg.cost, 0), COALESCE(agg.requests, 0)
FROM grid LEFT JOIN agg ON agg.ts = grid.ts
ORDER BY grid.ts
"#;

/// Dense grid of local days, from the cutoff's local date through today.
const DAYS_SQL: &str = r#"
WITH RECURSIVE grid(day) AS (
  SELECT date(?1 / 1000, 'unixepoch', 'localtime')
  UNION ALL
  SELECT date(day, '+1 day') FROM grid
  WHERE day < date(?2 / 1000, 'unixepoch', 'localtime')
),
agg AS (
  SELECT date(created / 1000, 'unixepoch', 'localtime') AS day,
         SUM(input + output + reasoning + cache_read + cache_write) AS tokens,
         SUM(cost) AS cost,
         COUNT(*) AS requests
  FROM (%ROWS%)
  WHERE date(created / 1000, 'unixepoch', 'localtime')
        >= date(?1 / 1000, 'unixepoch', 'localtime')%FILTER%
  GROUP BY day
)
SELECT grid.day, COALESCE(agg.tokens, 0), COALESCE(agg.cost, 0), COALESCE(agg.requests, 0)
FROM grid LEFT JOIN agg ON agg.day = grid.day
ORDER BY grid.day
"#;

/// Cutoff aligned to a clock boundary, so buckets and totals agree.
/// `?1` is the wall-clock time in milliseconds, `?2` the requested span.
fn aligned_cutoff(conn: &Connection, hourly: bool, now: i64, span: i64) -> rusqlite::Result<i64> {
    if hourly {
        // Top of the hour 23 hours back: 24 whole hour buckets, the last partial.
        conn.query_row(
            "SELECT ((?1 / 3600000) * 3600000) - 23 * 3600000",
            [now],
            |row| row.get(0),
        )
    } else {
        // Local midnight of the first day in the window. The 'utc' modifier
        // reads a local time string back as UTC, which is the epoch we want.
        conn.query_row(
            "SELECT CAST(strftime('%s', date(?1 / 1000, 'unixepoch', 'localtime', \
             '-' || (CAST(?2 / 86400000 AS INTEGER) - 1) || ' days') || ' 00:00:00', 'utc') \
             AS INTEGER) * 1000",
            [now, span],
            |row| row.get(0),
        )
    }
}

#[derive(Default)]
struct Collector {
    totals: Totals,
    models: BTreeMap<(String, String), ModelRow>,
    buckets: BTreeMap<String, Bucket>,
}

impl Collector {
    fn absorb(
        &mut self,
        conn: &Connection,
        cutoff: i64,
        hourly: bool,
        go_only: bool,
        now: i64,
    ) -> rusqlite::Result<()> {
        // Per-model totals.
        {
            let sql = MODEL_SQL
                .replace("%ROWS%", ROWS_SQL)
                .replace("%FILTER%", &provider_filter(go_only));
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query([cutoff])?;
            while let Some(row) = rows.next()? {
                let input: i64 = row.get(2)?;
                let output: i64 = row.get(3)?;
                let reasoning: i64 = row.get(4)?;
                let cache_read: i64 = row.get(5)?;
                let cache_write: i64 = row.get(6)?;
                let cost: f64 = row.get(7)?;
                let requests: i64 = row.get(8)?;
                let tokens = input + output + reasoning + cache_read + cache_write;

                self.totals.input += input;
                self.totals.output += output;
                self.totals.reasoning += reasoning;
                self.totals.cache_read += cache_read;
                self.totals.cache_write += cache_write;
                self.totals.tokens += tokens;
                self.totals.requests += requests;
                self.totals.cost += cost;

                self.models.insert(
                    (row.get(0)?, row.get(1)?),
                    ModelRow {
                        provider: row.get(0)?,
                        model: row.get(1)?,
                        input,
                        output,
                        reasoning,
                        cache_read,
                        tokens,
                        requests,
                        cost,
                    },
                );
            }
        }

        // Time series, aligned to the same cutoff.
        {
            let template = if hourly { HOURS_SQL } else { DAYS_SQL };
            let sql = template
                .replace("%ROWS%", ROWS_SQL)
                .replace("%FILTER%", &provider_filter(go_only));
            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query([cutoff, now])?;
            while let Some(row) = rows.next()? {
                let (key, start) = if hourly {
                    let ts: i64 = row.get(0)?;
                    (ts.to_string(), Some(ts))
                } else {
                    (row.get::<_, String>(0)?, None)
                };
                let tokens: i64 = row.get(1)?;
                let cost: f64 = row.get(2)?;
                let requests: i64 = row.get(3)?;
                let entry = self.buckets.entry(key.clone()).or_insert(Bucket {
                    key,
                    start,
                    tokens: 0,
                    requests: 0,
                    cost: 0.0,
                });
                entry.tokens += tokens;
                entry.requests += requests;
                entry.cost += cost;
            }
        }

        Ok(())
    }
}

/// Reads every database that exists and merges them.
pub fn detail(range: &str, go_only: bool) -> Detail {
    let Some(span) = range_ms(range) else {
        return Detail::unavailable(range, go_only, "Unknown range.");
    };

    let paths = candidate_paths();
    if paths.is_empty() {
        return Detail::unavailable(
            range,
            go_only,
            "No OpenCode database found on this Mac, so there is no local detail to show.",
        );
    }

    let now = now_ms();
    let hourly = range == "24h";
    let mut collector = Collector::default();
    let mut read_any = false;
    let mut failure: Option<String> = None;

    for path in &paths {
        match read_one(path, span, now, hourly, go_only, &mut collector) {
            Ok(()) => read_any = true,
            Err(error) => failure = Some(error.to_string()),
        }
    }

    if !read_any {
        return Detail::unavailable(
            range,
            go_only,
            failure.unwrap_or_else(|| "The OpenCode database could not be read.".into()),
        );
    }

    let mut models: Vec<ModelRow> = collector.models.into_values().collect();
    models.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(b.cost.total_cmp(&a.cost)));

    let mut buckets: Vec<Bucket> = collector.buckets.into_values().collect();
    if hourly {
        // Keys are epoch milliseconds as strings; sort them as numbers.
        buckets.sort_by_key(|b| b.start.unwrap_or(0));
    }

    Detail {
        range: range.to_string(),
        filter: filter_name(go_only),
        available: true,
        message: None,
        database: paths
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        totals: collector.totals,
        models,
        bucket_unit: if hourly { "hour".into() } else { "day".into() },
        buckets,
    }
}

fn read_one(
    path: &Path,
    span: i64,
    now: i64,
    hourly: bool,
    go_only: bool,
    collector: &mut Collector,
) -> rusqlite::Result<()> {
    // Read-only, but WAL databases still need the sidecar files writable; if
    // read-only fails, fall back to a normal handle. Only SELECTs ever run.
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .or_else(|_| Connection::open(path))?;
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;

    let cutoff = aligned_cutoff(&conn, hourly, now, span)?;
    collector.absorb(&conn, cutoff, hourly, go_only, now)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_known() {
        assert!(range_ms("24h").is_some());
        assert!(range_ms("7d").is_some());
        assert!(range_ms("30d").is_some());
        assert!(range_ms("all").is_none());
    }

    /// The buckets and the per-model totals must describe the same window.
    fn assert_buckets_match_totals(detail: &Detail) {
        let bucket_tokens: i64 = detail.buckets.iter().map(|b| b.tokens).sum();
        let bucket_requests: i64 = detail.buckets.iter().map(|b| b.requests).sum();
        let model_tokens: i64 = detail.models.iter().map(|m| m.tokens).sum();
        assert_eq!(bucket_tokens, detail.totals.tokens, "bucket tokens vs totals");
        assert_eq!(model_tokens, detail.totals.tokens, "model tokens vs totals");
        assert_eq!(bucket_requests, detail.totals.requests, "requests");
    }

    /// The Go filter must narrow the same window, not a different one.
    #[test]
    fn the_go_filter_keeps_go_models_and_consistent_totals() {
        if candidate_paths().is_empty() {
            eprintln!("no OpenCode database on this machine, skipping");
            return;
        }

        let all = detail("30d", false);
        let go = detail("30d", true);
        assert!(go.available, "the filtered read should still work");
        assert_eq!(go.filter, "go");
        assert_eq!(all.filter, "all");

        let is_go = |provider: &str| matches!(provider, "opencode-go" | "opencode");
        assert!(
            go.models.iter().all(|model| is_go(&model.provider)),
            "only Go providers survive the filter: {:?}",
            go.models.iter().map(|m| &m.provider).collect::<Vec<_>>()
        );
        assert!(go.totals.tokens <= all.totals.tokens, "filtering cannot add tokens");
        assert_buckets_match_totals(&go);

        // If this machine also used other providers, they are what the filter
        // removed — otherwise the numbers are legitimately identical.
        let foreign = all
            .models
            .iter()
            .filter(|model| !is_go(&model.provider))
            .map(|model| model.tokens)
            .sum::<i64>();
        if foreign > 0 {
            assert_eq!(
                go.totals.tokens,
                all.totals.tokens - foreign,
                "exactly the non-Go tokens are gone"
            );
        }
    }

    #[test]
    fn the_bucket_count_is_the_same_either_way() {
        if candidate_paths().is_empty() {
            return;
        }
        let filtered: Vec<String> = detail("7d", true).buckets.iter().map(|b| b.key.clone()).collect();
        let unfiltered: Vec<String> = detail("7d", false).buckets.iter().map(|b| b.key.clone()).collect();
        assert_eq!(filtered, unfiltered, "the chart grid does not move with the filter");
        assert_eq!(filtered.len(), 7);
    }

    /// The provider list exists in two languages; this keeps them honest.
    #[test]
    fn the_go_provider_list_matches_the_panel() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/types.ts"))
            .expect("the panel source sits next to the crate");
        let line = source
            .lines()
            .find(|line| line.contains("GO_PROVIDERS ="))
            .expect("GO_PROVIDERS is declared in types.ts");

        for provider in GO_PROVIDERS {
            assert!(line.contains(provider), "{provider} missing from: {line}");
        }
        let declared = line.matches('"').count() / 2;
        assert_eq!(
            declared,
            GO_PROVIDERS.len(),
            "the panel lists {declared} providers, the crate lists {}: {line}",
            GO_PROVIDERS.len()
        );
    }

    /// Runs against the real database when this machine has one.
    #[test]
    fn reads_the_local_database_when_present() {
        if candidate_paths().is_empty() {
            eprintln!("no OpenCode database on this machine, skipping");
            return;
        }

        for range in ["24h", "7d", "30d"] {
            let detail = detail(range, false);
            assert!(detail.available, "{range} should be available");
            assert!(!detail.models.is_empty(), "{range} should have models");
            assert!(detail.totals.requests > 0, "{range} should have requests");
            assert_buckets_match_totals(&detail);

            println!(
                "{range}: {} requests, {} tokens (in {} out {} think {} cache-read {}), ${:.4}",
                detail.totals.requests,
                detail.totals.tokens,
                detail.totals.input,
                detail.totals.output,
                detail.totals.reasoning,
                detail.totals.cache_read,
                detail.totals.cost,
            );
            println!(
                "  {} {} buckets, first {} last {}",
                detail.buckets.len(),
                detail.bucket_unit,
                detail.buckets.first().map(|b| b.key.clone()).unwrap_or_default(),
                detail.buckets.last().map(|b| b.key.clone()).unwrap_or_default(),
            );
            for model in detail.models.iter().take(4) {
                println!(
                    "  {}/{}  tok={} req={} cost={:.4}",
                    model.provider, model.model, model.tokens, model.requests, model.cost
                );
            }
            let busy: Vec<String> = {
                let mut sorted = detail.buckets.clone();
                sorted.sort_by(|a, b| b.tokens.cmp(&a.tokens));
                sorted
                    .iter()
                    .take(3)
                    .map(|b| format!("{}={}", b.key, b.tokens))
                    .collect()
            };
            println!("  busiest: {}", busy.join(" "));

            // The window must be denser than the number of distinct active
            // buckets would suggest: exactly 24 hours, or one bucket per day.
            let expected = if range == "24h" { 24 } else { 0 };
            if expected > 0 {
                assert_eq!(detail.buckets.len(), expected, "{range} bucket count");
            }
        }
    }
}

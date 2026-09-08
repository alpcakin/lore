//! Usage statistics and remembered placeholder values.
//!
//! Kept apart from the definition files on purpose. Definitions are meant to be
//! tracked in the user's own git repository; counters that change on every
//! keystroke would make that repository permanently dirty and conflict on every
//! sync.
//!
//! Ranking uses frecency: each use adds a point, and points decay exponentially.
//! Only the running score and the moment it was last touched are stored, so the
//! cost is two columns per entry rather than a use history.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

/// How long a score takes to halve, in seconds.
pub const HALF_LIFE: f64 = 14.0 * 24.0 * 60.0 * 60.0;

/// Score a freshly saved command starts at, worth roughly three recent uses.
///
/// This is what keeps new entries near the top without a separate "added
/// today" rule: the head start decays on the same curve as everything else, so
/// an entry that never gets used slides down instead of falling off a cliff.
pub const NEW_ENTRY_BONUS: f64 = 3.0;

/// How long to wait for another terminal to finish writing.
const BUSY_TIMEOUT: Duration = Duration::from_secs(3);

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS usage (
    id           TEXT PRIMARY KEY,
    score        REAL NOT NULL,
    last_used_at INTEGER NOT NULL,
    pinned       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS param_values (
    id    TEXT NOT NULL,
    name  TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (id, name)
);
";

/// What ranking needs to know about one entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    pub value: f64,
    pub pinned: bool,
}

/// Seconds since the Unix epoch.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// A score after `elapsed` seconds of decay.
pub fn decay(score: f64, elapsed: i64) -> f64 {
    if elapsed <= 0 {
        return score;
    }
    score * 0.5f64.powf(elapsed as f64 / HALF_LIFE)
}

pub struct Stats {
    conn: Connection,
}

impl Stats {
    /// Opens the statistics database, creating it if it does not exist.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let conn =
            Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        Self::prepare(conn)
    }

    pub fn in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(conn: Connection) -> Result<Self> {
        // Several terminals share this file, so concurrent readers must not
        // block on a writer and a writer must wait rather than fail.
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))
            .context("failed to enable write-ahead logging")?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.execute_batch(SCHEMA)
            .context("failed to initialise the statistics schema")?;

        Ok(Self { conn })
    }

    /// Records that an entry was selected, decaying its previous score first.
    pub fn record_use(&self, id: &str, now: i64) -> Result<()> {
        let previous = self.raw_score(id)?;
        let score = match previous {
            Some((score, last_used_at)) => decay(score, now - last_used_at) + 1.0,
            None => 1.0,
        };
        self.write_score(id, score, now)
    }

    /// Gives a newly saved entry its head start.
    pub fn record_new(&self, id: &str, now: i64) -> Result<()> {
        self.write_score(id, NEW_ENTRY_BONUS, now)
    }

    /// Current scores for every entry that has one, decayed to `now`.
    ///
    /// Entries absent from the result have never been used and rank below any
    /// entry that has.
    pub fn scores(&self, now: i64) -> Result<HashMap<String, Score>> {
        let mut statement = self
            .conn
            .prepare("SELECT id, score, last_used_at, pinned FROM usage")?;

        let rows = statement.query_map([], |row| {
            let id: String = row.get(0)?;
            let score: f64 = row.get(1)?;
            let last_used_at: i64 = row.get(2)?;
            let pinned: bool = row.get(3)?;
            Ok((
                id,
                Score {
                    value: decay(score, now - last_used_at),
                    pinned,
                },
            ))
        })?;

        Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
    }

    pub fn set_pinned(&self, id: &str, pinned: bool, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO usage (id, score, last_used_at, pinned) VALUES (?1, 0.0, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET pinned = ?3",
            params![id, now, pinned],
        )?;
        Ok(())
    }

    /// Remembers the value last entered for a placeholder, so the next prompt
    /// arrives pre-filled.
    pub fn remember_param(&self, id: &str, name: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO param_values (id, name, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(id, name) DO UPDATE SET value = ?3",
            params![id, name, value],
        )?;
        Ok(())
    }

    pub fn last_params(&self, id: &str) -> Result<BTreeMap<String, String>> {
        let mut statement = self
            .conn
            .prepare("SELECT name, value FROM param_values WHERE id = ?1")?;
        let rows = statement.query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()?)
    }

    /// Drops everything remembered about an entry the user deleted.
    pub fn forget(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM usage WHERE id = ?1", [id])?;
        self.conn
            .execute("DELETE FROM param_values WHERE id = ?1", [id])?;
        Ok(())
    }

    fn raw_score(&self, id: &str) -> Result<Option<(f64, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT score, last_used_at FROM usage WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    fn write_score(&self, id: &str, score: f64, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO usage (id, score, last_used_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET score = ?2, last_used_at = ?3",
            params![id, score, now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 24 * 60 * 60;

    fn score_of(stats: &Stats, id: &str, now: i64) -> f64 {
        stats.scores(now).unwrap()[id].value
    }

    #[test]
    fn a_score_halves_over_one_half_life() {
        let halved = decay(4.0, HALF_LIFE as i64);
        assert!((halved - 2.0).abs() < 1e-6, "expected 2.0, got {halved}");
    }

    #[test]
    fn a_score_does_not_grow_backwards_in_time() {
        assert_eq!(decay(4.0, 0), 4.0);
        assert_eq!(decay(4.0, -DAY), 4.0);
    }

    #[test]
    fn first_use_scores_one_point() {
        let stats = Stats::in_memory().unwrap();
        stats.record_use("git.log", 0).unwrap();
        assert_eq!(score_of(&stats, "git.log", 0), 1.0);
    }

    #[test]
    fn repeated_use_accumulates_on_top_of_decay() {
        let stats = Stats::in_memory().unwrap();
        let half_life = HALF_LIFE as i64;

        stats.record_use("git.log", 0).unwrap();
        stats.record_use("git.log", half_life).unwrap();

        // The first point has halved by then, so the running score is 1.5.
        let score = score_of(&stats, "git.log", half_life);
        assert!((score - 1.5).abs() < 1e-6, "expected 1.5, got {score}");
    }

    #[test]
    fn a_daily_habit_outranks_a_forgotten_burst() {
        let stats = Stats::in_memory().unwrap();
        let now = 60 * DAY;

        for day in 0..60 {
            stats.record_use("daily", day * DAY).unwrap();
        }
        for _ in 0..20 {
            stats.record_use("burst", 0).unwrap();
        }

        assert!(score_of(&stats, "daily", now) > score_of(&stats, "burst", now));
    }

    #[test]
    fn a_new_entry_starts_ahead_but_slides_without_use() {
        let stats = Stats::in_memory().unwrap();

        stats.record_new("fresh", 0).unwrap();
        stats.record_use("occasional", 0).unwrap();
        assert!(score_of(&stats, "fresh", 0) > score_of(&stats, "occasional", 0));

        // Two months later the head start is spent and steady use has won.
        let later = 60 * DAY;
        stats.record_use("occasional", 30 * DAY).unwrap();
        stats.record_use("occasional", 55 * DAY).unwrap();
        assert!(score_of(&stats, "fresh", later) < score_of(&stats, "occasional", later));
    }

    #[test]
    fn an_unused_entry_has_no_score() {
        let stats = Stats::in_memory().unwrap();
        stats.record_use("git.log", 0).unwrap();
        assert!(!stats.scores(0).unwrap().contains_key("docker.ps"));
    }

    #[test]
    fn pinning_survives_later_use() {
        let stats = Stats::in_memory().unwrap();
        stats.set_pinned("git.log", true, 0).unwrap();
        stats.record_use("git.log", DAY).unwrap();

        let score = stats.scores(DAY).unwrap()["git.log"];
        assert!(score.pinned);
        assert_eq!(score.value, 1.0);
    }

    #[test]
    fn pinning_an_unused_entry_does_not_invent_a_score() {
        let stats = Stats::in_memory().unwrap();
        stats.set_pinned("git.log", true, 0).unwrap();
        assert_eq!(score_of(&stats, "git.log", 0), 0.0);
    }

    #[test]
    fn remembered_parameters_round_trip_and_overwrite() {
        let stats = Stats::in_memory().unwrap();
        stats
            .remember_param("k8s.logs", "namespace", "dev")
            .unwrap();
        stats.remember_param("k8s.logs", "pod", "api-0").unwrap();
        stats
            .remember_param("k8s.logs", "namespace", "prod")
            .unwrap();

        let values = stats.last_params("k8s.logs").unwrap();
        assert_eq!(values["namespace"], "prod");
        assert_eq!(values["pod"], "api-0");
        assert!(stats.last_params("other").unwrap().is_empty());
    }

    #[test]
    fn two_connections_can_write_the_same_database() {
        let path = std::env::temp_dir().join(format!("lore-stats-{}.db", std::process::id()));
        let _ = fs::remove_file(&path);

        let first = Stats::open(&path).unwrap();
        let second = Stats::open(&path).unwrap();

        first.record_use("git.log", 0).unwrap();
        second.record_use("docker.ps", 0).unwrap();
        first.record_use("docker.ps", DAY).unwrap();

        // Each connection sees the other's writes, and the second use of
        // docker.ps landed on top of the decayed first one.
        let scores = second.scores(DAY).unwrap();
        assert_eq!(scores.len(), 2);
        assert!((scores["docker.ps"].value - (decay(1.0, DAY) + 1.0)).abs() < 1e-9);
        assert_eq!(scores["git.log"].value, decay(1.0, DAY));

        drop(first);
        drop(second);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn forgetting_an_entry_clears_its_score_and_parameters() {
        let stats = Stats::in_memory().unwrap();
        stats.record_use("gone", 0).unwrap();
        stats.remember_param("gone", "path", "/tmp").unwrap();

        stats.forget("gone").unwrap();

        assert!(stats.scores(0).unwrap().is_empty());
        assert!(stats.last_params("gone").unwrap().is_empty());
    }
}

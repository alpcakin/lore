//! Filtering and ordering the command list.
//!
//! A query is split on whitespace and every term has to be found as a
//! contiguous run of characters. Letting the letters of one term scatter across
//! a field fills the list with entries that share nothing with what was typed:
//! `git` otherwise matches `Get-ChildItem` through its g, i and t.
//!
//! Scattered matching survives only as a fallback for a query that finds nothing
//! at all, where a mistyped guess beats an empty list.
//!
//! Match quality is the primary sort key and is deliberately coarse. Fine
//! grained scores reorder neighbouring entries for reasons the user cannot see,
//! and a picker whose order cannot be predicted breaks the muscle memory it
//! exists to serve. Frecency only breaks ties inside a bucket.

use std::cmp::Ordering;
use std::collections::HashMap;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::model::{Entry, Layer};
use crate::store::stats::Score;

/// An entry with its command already resolved for the active shell.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub entry: &'a Entry,
    pub cmd: &'a str,
}

/// How closely a field matched. Ordered weakest to strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Quality {
    /// The term appears only as a scattered subsequence, from the fallback pass.
    Scattered,
    /// The term appears somewhere inside a word.
    Inside,
    /// The term starts a word.
    WordStart,
    /// The field starts with the term.
    Prefix,
}

/// Which field matched. Ordered least to most significant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    Tags,
    Desc,
    Cmd,
}

/// Everything the ordering depends on, highest wins.
struct Rank {
    quality: Option<(Quality, Field)>,
    pinned: bool,
    /// User entries outrank builtins only while the query is empty. Once the
    /// user is searching, a builtin may well be exactly what they want.
    layer: Option<Layer>,
    frecency: f64,
    tiebreak: u32,
}

pub struct Ranker {
    matcher: Matcher,
    buffer: Vec<char>,
}

impl Default for Ranker {
    fn default() -> Self {
        Self::new()
    }
}

impl Ranker {
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            buffer: Vec::new(),
        }
    }

    /// Indices into `candidates`, best first, with non-matching entries removed.
    pub fn rank(
        &mut self,
        candidates: &[Candidate<'_>],
        scores: &HashMap<String, Score>,
        query: &str,
    ) -> Vec<usize> {
        let terms = terms(query);
        if terms.is_empty() {
            return self.order(candidates, scores, |_, _| Some((None, 0)));
        }

        let found = self.order(candidates, scores, |_, candidate| {
            all_terms(candidate, &terms).map(|quality| (Some(quality), 0))
        });
        if !found.is_empty() {
            return found;
        }

        // Nothing contained what was typed, so loosen off rather than showing an
        // empty panel.
        let pattern = fuzzy_pattern(query);
        self.order(candidates, scores, |ranker, candidate| {
            ranker
                .scattered(candidate, &pattern)
                .map(|(field, score)| (Some((Quality::Scattered, field)), score))
        })
    }

    #[allow(clippy::type_complexity)]
    fn order(
        &mut self,
        candidates: &[Candidate<'_>],
        scores: &HashMap<String, Score>,
        assess: impl Fn(&mut Self, &Candidate<'_>) -> Option<(Option<(Quality, Field)>, u32)>,
    ) -> Vec<usize> {
        let mut ranked: Vec<(usize, Rank)> = Vec::with_capacity(candidates.len());

        for (index, candidate) in candidates.iter().enumerate() {
            let Some((quality, tiebreak)) = assess(self, candidate) else {
                continue;
            };
            let score = scores.get(&candidate.entry.id);

            ranked.push((
                index,
                Rank {
                    quality,
                    pinned: score.is_some_and(|s| s.pinned),
                    layer: quality.is_none().then_some(candidate.entry.layer),
                    frecency: score.map(|s| s.value).unwrap_or_default(),
                    tiebreak,
                },
            ));
        }

        ranked.sort_by(|(left_index, left), (right_index, right)| {
            compare(left, right).then_with(|| {
                // Ids are unique and the input is id-ordered, so this makes the
                // result stable rather than merely deterministic.
                candidates[*left_index]
                    .entry
                    .id
                    .cmp(&candidates[*right_index].entry.id)
            })
        });

        ranked.into_iter().map(|(index, _)| index).collect()
    }

    fn scattered(&mut self, candidate: &Candidate<'_>, pattern: &Pattern) -> Option<(Field, u32)> {
        let tags = candidate.entry.tags.join(" ");
        let fields = [
            (Field::Cmd, candidate.cmd),
            (Field::Desc, candidate.entry.desc.as_str()),
            (Field::Tags, tags.as_str()),
        ];

        fields
            .into_iter()
            .filter_map(|(field, haystack)| {
                let score =
                    pattern.score(Utf32Str::new(haystack, &mut self.buffer), &mut self.matcher)?;
                Some((field, score))
            })
            .max_by_key(|(field, score)| (*field, *score))
    }

    /// Character positions in `haystack` that the query matched, for
    /// highlighting.
    ///
    /// Called only for the rows actually on screen, so its cost does not scale
    /// with the size of the library.
    pub fn highlight(&mut self, haystack: &str, query: &str) -> Vec<u32> {
        let terms = terms(query);
        if terms.is_empty() {
            return Vec::new();
        }

        let mut found: Vec<u32> = terms
            .iter()
            .filter_map(|term| find(haystack, term))
            .flat_map(|(_, covered)| covered)
            .collect();

        if found.is_empty() {
            let pattern = fuzzy_pattern(query);
            pattern.indices(
                Utf32Str::new(haystack, &mut self.buffer),
                &mut self.matcher,
                &mut found,
            );
        }

        found.sort_unstable();
        found.dedup();
        found
    }
}

fn fuzzy_pattern(query: &str) -> Pattern {
    Pattern::new(
        query.trim(),
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    )
}

fn terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .collect()
}

/// The weakest match among the terms, or `None` if any term is missing.
///
/// Taking the weakest is what keeps an entry that only mentions one term in its
/// tags below an entry whose command contains them all.
fn all_terms(candidate: &Candidate<'_>, terms: &[String]) -> Option<(Quality, Field)> {
    let tags = candidate.entry.tags.join(" ");
    let fields = [
        (Field::Cmd, candidate.cmd),
        (Field::Desc, candidate.entry.desc.as_str()),
        (Field::Tags, tags.as_str()),
    ];

    terms
        .iter()
        .map(|term| {
            fields
                .iter()
                .filter_map(|(field, haystack)| {
                    quality_of(haystack, term).map(|quality| (quality, *field))
                })
                .max()
        })
        .try_fold(None, |weakest: Option<(Quality, Field)>, best| {
            let best = best?;
            Some(Some(match weakest {
                Some(weakest) => weakest.min(best),
                None => best,
            }))
        })
        .flatten()
}

fn quality_of(haystack: &str, term: &str) -> Option<Quality> {
    let (at, _) = find(haystack, term)?;

    if at == 0 {
        return Some(Quality::Prefix);
    }
    if starts_word(haystack, at) {
        return Some(Quality::WordStart);
    }
    Some(Quality::Inside)
}

/// The byte offset where `needle` first occurs, with the character positions it
/// covers.
fn find(haystack: &str, needle: &str) -> Option<(usize, Vec<u32>)> {
    if needle.is_empty() {
        return None;
    }

    let length = needle.chars().count();
    haystack
        .char_indices()
        .enumerate()
        .find(|(_, (offset, _))| starts_with_ignore_case(&haystack[*offset..], needle))
        .map(|(position, (offset, _))| {
            let covered = (position..position + length).map(|n| n as u32).collect();
            (offset, covered)
        })
}

/// Whether the character before `at` ends a word, making `at` the start of one.
fn starts_word(haystack: &str, at: usize) -> bool {
    haystack[..at]
        .chars()
        .next_back()
        .is_none_or(|previous| !previous.is_alphanumeric())
}

fn starts_with_ignore_case(haystack: &str, needle: &str) -> bool {
    let mut haystack = haystack.chars().flat_map(char::to_lowercase);
    let mut needle = needle.chars().flat_map(char::to_lowercase);

    loop {
        match (needle.next(), haystack.next()) {
            (None, _) => return true,
            (Some(_), None) => return false,
            (Some(wanted), Some(found)) if wanted != found => return false,
            _ => {}
        }
    }
}

fn compare(left: &Rank, right: &Rank) -> Ordering {
    right
        .quality
        .cmp(&left.quality)
        .then(right.pinned.cmp(&left.pinned))
        .then(right.layer.cmp(&left.layer))
        .then(right.frecency.total_cmp(&left.frecency))
        .then(right.tiebreak.cmp(&left.tiebreak))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CommandBody, Entry, Layer};
    use std::collections::BTreeMap;

    fn entry(id: &str, cmd: &str, desc: &str, tags: &[&str], layer: Layer) -> Entry {
        Entry {
            id: id.to_string(),
            cmd: CommandBody::Shared(cmd.to_string()),
            desc: desc.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            params: BTreeMap::new(),
            danger: false,
            layer,
        }
    }

    fn candidates(entries: &[Entry]) -> Vec<Candidate<'_>> {
        entries
            .iter()
            .map(|entry| Candidate {
                entry,
                cmd: match &entry.cmd {
                    CommandBody::Shared(cmd) => cmd.as_str(),
                    CommandBody::PerShell(_) => unreachable!("test entries are shared"),
                },
            })
            .collect()
    }

    fn scored(pairs: &[(&str, f64)]) -> HashMap<String, Score> {
        pairs
            .iter()
            .map(|(id, value)| {
                (
                    id.to_string(),
                    Score {
                        value: *value,
                        pinned: false,
                    },
                )
            })
            .collect()
    }

    fn order<'a>(
        entries: &'a [Entry],
        scores: &HashMap<String, Score>,
        query: &str,
    ) -> Vec<&'a str> {
        let candidates = candidates(entries);
        Ranker::new()
            .rank(&candidates, scores, query)
            .into_iter()
            .map(|index| candidates[index].entry.id.as_str())
            .collect()
    }

    fn sample() -> Vec<Entry> {
        vec![
            entry(
                "docker.ps",
                "docker ps -a",
                "List containers",
                &["docker"],
                Layer::Builtin,
            ),
            entry(
                "git.log",
                "git log --oneline",
                "Show history",
                &["git"],
                Layer::Builtin,
            ),
            entry(
                "git.push",
                "git push --force-with-lease",
                "Publish the branch",
                &["git"],
                Layer::User,
            ),
        ]
    }

    #[test]
    fn an_empty_query_keeps_everything() {
        let entries = sample();
        assert_eq!(order(&entries, &HashMap::new(), "").len(), 3);
    }

    #[test]
    fn an_empty_query_puts_user_entries_before_builtins() {
        let entries = sample();
        assert_eq!(order(&entries, &HashMap::new(), "")[0], "git.push");
    }

    #[test]
    fn an_empty_query_ranks_by_frecency_within_a_layer() {
        let entries = sample();
        let scores = scored(&[("git.log", 5.0), ("docker.ps", 1.0)]);
        assert_eq!(
            order(&entries, &scores, ""),
            ["git.push", "git.log", "docker.ps"]
        );
    }

    #[test]
    fn a_query_filters_out_entries_that_do_not_match() {
        let entries = sample();
        assert_eq!(order(&entries, &HashMap::new(), "docker"), ["docker.ps"]);
    }

    #[test]
    fn scattered_letters_do_not_count_as_a_match() {
        let entries = vec![
            entry(
                "sys.list",
                "Get-ChildItem -Path <dir> -Recurse",
                "Find files under a directory",
                &[],
                Layer::Builtin,
            ),
            entry(
                "git.log",
                "git log --oneline",
                "Show history",
                &[],
                Layer::Builtin,
            ),
        ];
        // Get-ChildItem carries a g, an i and a t, but never "git" together.
        assert_eq!(order(&entries, &HashMap::new(), "git"), ["git.log"]);
    }

    #[test]
    fn every_term_has_to_be_found() {
        let entries = vec![
            entry(
                "git.clean",
                "git clean -nfdx",
                "Preview a clean",
                &[],
                Layer::Builtin,
            ),
            entry(
                "git.log",
                "git log --oneline",
                "Show history",
                &[],
                Layer::Builtin,
            ),
        ];
        assert_eq!(order(&entries, &HashMap::new(), "git cl"), ["git.clean"]);
    }

    #[test]
    fn terms_may_land_in_different_fields() {
        let entries = vec![entry(
            "docker.logs",
            "docker logs -f <container>",
            "Follow the output of a running container",
            &["debug"],
            Layer::Builtin,
        )];
        // "docker" from the command, "running" from the description.
        assert_eq!(
            order(&entries, &HashMap::new(), "docker running"),
            ["docker.logs"]
        );
    }

    #[test]
    fn a_term_inside_a_word_still_matches() {
        let entries = sample();
        assert_eq!(order(&entries, &HashMap::new(), "onelin"), ["git.log"]);
    }

    #[test]
    fn a_query_that_matches_nothing_falls_back_to_scattered_letters() {
        let entries = sample();
        // No entry contains "dps", but docker ps spells it out in order.
        assert_eq!(order(&entries, &HashMap::new(), "dps"), ["docker.ps"]);
    }

    #[test]
    fn the_fallback_still_reports_nothing_for_a_hopeless_query() {
        let entries = sample();
        assert!(order(&entries, &HashMap::new(), "zzzzq").is_empty());
    }

    #[test]
    fn match_quality_outranks_frecency() {
        let entries = sample();
        let scores = scored(&[("docker.ps", 500.0)]);
        let ranked = order(&entries, &scores, "git");
        assert!(!ranked.contains(&"docker.ps"));
    }

    #[test]
    fn match_quality_outranks_the_user_layer() {
        let entries = vec![
            entry(
                "user.thing",
                "kubectl describe thing",
                "Describe a thing",
                &[],
                Layer::User,
            ),
            entry(
                "builtin.kubectl",
                "kubectl get pods",
                "List pods",
                &[],
                Layer::Builtin,
            ),
        ];
        assert_eq!(
            order(&entries, &HashMap::new(), "kubectl get")[0],
            "builtin.kubectl"
        );
    }

    #[test]
    fn frecency_breaks_ties_inside_a_quality_bucket() {
        let entries = sample();
        let scores = scored(&[("git.push", 1.0), ("git.log", 9.0)]);
        assert_eq!(order(&entries, &scores, "git"), ["git.log", "git.push"]);
    }

    #[test]
    fn a_command_match_outranks_a_description_match() {
        let entries = vec![
            entry(
                "by.desc",
                "ls -la",
                "show docker containers",
                &[],
                Layer::Builtin,
            ),
            entry(
                "by.cmd",
                "docker ps",
                "list running things",
                &[],
                Layer::Builtin,
            ),
        ];
        assert_eq!(order(&entries, &HashMap::new(), "docker")[0], "by.cmd");
    }

    #[test]
    fn a_description_match_finds_a_command_by_intent() {
        let entries = sample();
        assert_eq!(order(&entries, &HashMap::new(), "history"), ["git.log"]);
    }

    #[test]
    fn a_tag_match_still_finds_the_entry() {
        let entries = vec![entry(
            "sys.ports",
            "ss -tulpn",
            "Show listening sockets",
            &["network", "troubleshooting"],
            Layer::Builtin,
        )];
        assert_eq!(order(&entries, &HashMap::new(), "network"), ["sys.ports"]);
    }

    #[test]
    fn pinning_wins_inside_the_empty_state() {
        let entries = sample();
        let mut scores = scored(&[("docker.ps", 0.1)]);
        scores.get_mut("docker.ps").unwrap().pinned = true;
        assert_eq!(order(&entries, &scores, "")[0], "docker.ps");
    }

    #[test]
    fn pinning_never_overrides_match_quality() {
        let entries = sample();
        let mut scores = scored(&[("docker.ps", 100.0)]);
        scores.get_mut("docker.ps").unwrap().pinned = true;
        assert!(!order(&entries, &scores, "git").contains(&"docker.ps"));
    }

    #[test]
    fn a_word_start_outranks_a_match_inside_a_word() {
        let entries = vec![
            // "arg" is buried in the middle of "cargo".
            entry("inside", "cargo build --release", "", &[], Layer::Builtin),
            // "arg" opens a word of its own.
            entry("word.start", "git argocd sync", "", &[], Layer::Builtin),
        ];
        assert_eq!(
            order(&entries, &HashMap::new(), "arg"),
            ["word.start", "inside"]
        );
    }

    #[test]
    fn ordering_is_stable_when_nothing_distinguishes_entries() {
        let entries = sample();
        let first = order(&entries, &HashMap::new(), "");
        assert_eq!(first, order(&entries, &HashMap::new(), ""));
    }

    #[test]
    fn highlight_marks_the_term_it_found() {
        let mut ranker = Ranker::new();
        assert_eq!(ranker.highlight("docker ps", "ps"), [7, 8]);
        assert_eq!(ranker.highlight("docker ps", "docker"), [0, 1, 2, 3, 4, 5]);
        assert!(ranker.highlight("docker ps", "").is_empty());
    }

    #[test]
    fn highlight_falls_back_with_the_search() {
        let mut ranker = Ranker::new();
        assert_eq!(ranker.highlight("docker ps", "dps"), [0, 7, 8]);
    }

    /// Not an assertion: timing thresholds are flaky on shared runners. Run it
    /// with `cargo test --release -- --ignored --nocapture` to re-measure the
    /// per-keystroke budget.
    #[test]
    #[ignore = "measurement, not a pass or fail"]
    fn measure_ranking_cost_at_scale() {
        let entries: Vec<Entry> = (0..2000)
            .map(|n| {
                entry(
                    &format!("ns{}.entry{n}", n % 20),
                    &format!("kubectl get pods -n namespace{n} -o wide --context cluster{n}"),
                    &format!("List pods in namespace {n} with node and address columns"),
                    &["kubernetes", "kubectl", "pods"],
                    Layer::Builtin,
                )
            })
            .collect();

        let candidates = candidates(&entries);
        let scores = HashMap::new();
        let mut ranker = Ranker::new();

        for query in ["", "k", "ku", "kub", "kube", "pods", "get pods", "zzz"] {
            let started = std::time::Instant::now();
            let ranked = ranker.rank(&candidates, &scores, query);
            println!(
                "query {:>9?}: {:>5} matches in {:>8.3?}",
                query,
                ranked.len(),
                started.elapsed()
            );
        }
    }
}

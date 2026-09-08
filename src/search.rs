//! Filtering and ordering the command list.
//!
//! Match quality is the primary sort key and is deliberately coarse. Fine
//! grained fuzzy scores would reorder neighbouring entries for reasons the user
//! cannot see, and a picker whose order cannot be predicted breaks the muscle
//! memory it exists to serve. Frecency only breaks ties inside a bucket.

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
    /// The query appears only as a scattered subsequence.
    Fuzzy,
    /// The query starts a word inside the field.
    WordStart,
    /// The field starts with the query.
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
    fuzzy: u32,
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
        let query = query.trim();
        let pattern = (!query.is_empty()).then(|| {
            Pattern::new(
                query,
                CaseMatching::Smart,
                Normalization::Smart,
                AtomKind::Fuzzy,
            )
        });

        let mut ranked: Vec<(usize, Rank)> = candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                let rank = self.rank_one(candidate, scores, query, pattern.as_ref())?;
                Some((index, rank))
            })
            .collect();

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

    fn rank_one(
        &mut self,
        candidate: &Candidate<'_>,
        scores: &HashMap<String, Score>,
        query: &str,
        pattern: Option<&Pattern>,
    ) -> Option<Rank> {
        let score = scores.get(&candidate.entry.id);

        let (quality, fuzzy) = match pattern {
            None => (None, 0),
            Some(pattern) => {
                let best = self.best_field(candidate, query, pattern)?;
                (Some((best.0, best.1)), best.2)
            }
        };

        Some(Rank {
            quality,
            pinned: score.is_some_and(|s| s.pinned),
            layer: pattern.is_none().then_some(candidate.entry.layer),
            frecency: score.map(|s| s.value).unwrap_or_default(),
            fuzzy,
        })
    }

    /// The strongest match across the searchable fields, if any field matched.
    fn best_field(
        &mut self,
        candidate: &Candidate<'_>,
        query: &str,
        pattern: &Pattern,
    ) -> Option<(Quality, Field, u32)> {
        let tags = candidate.entry.tags.join(" ");
        let fields = [
            (Field::Cmd, candidate.cmd),
            (Field::Desc, candidate.entry.desc.as_str()),
            (Field::Tags, tags.as_str()),
        ];

        fields
            .into_iter()
            .filter_map(|(field, haystack)| {
                let fuzzy =
                    pattern.score(Utf32Str::new(haystack, &mut self.buffer), &mut self.matcher)?;
                Some((quality_of(haystack, query), field, fuzzy))
            })
            .max_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)))
    }

    /// Character positions in `haystack` that `query` matched, for highlighting.
    ///
    /// Called only for the rows actually on screen, so its cost does not scale
    /// with the size of the library.
    pub fn highlight(&mut self, haystack: &str, query: &str) -> Vec<u32> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }

        let pattern = Pattern::new(
            query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut indices = Vec::new();
        pattern.indices(
            Utf32Str::new(haystack, &mut self.buffer),
            &mut self.matcher,
            &mut indices,
        );
        indices.sort_unstable();
        indices.dedup();
        indices
    }
}

fn compare(left: &Rank, right: &Rank) -> Ordering {
    right
        .quality
        .cmp(&left.quality)
        .then(right.pinned.cmp(&left.pinned))
        .then(right.layer.cmp(&left.layer))
        .then(right.frecency.total_cmp(&left.frecency))
        .then(right.fuzzy.cmp(&left.fuzzy))
}

fn quality_of(haystack: &str, query: &str) -> Quality {
    if starts_with_ignore_case(haystack, query) {
        return Quality::Prefix;
    }

    let mut previous = ' ';
    for (offset, current) in haystack.char_indices() {
        if !previous.is_alphanumeric()
            && current.is_alphanumeric()
            && starts_with_ignore_case(&haystack[offset..], query)
        {
            return Quality::WordStart;
        }
        previous = current;
    }

    Quality::Fuzzy
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
        let ranked = order(&entries, &HashMap::new(), "");
        assert_eq!(ranked[0], "git.push");
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
    fn match_quality_outranks_frecency() {
        let entries = sample();
        // docker.ps is heavily used, but only git commands start with "git".
        let scores = scored(&[("docker.ps", 500.0)]);
        let ranked = order(&entries, &scores, "git");
        assert!(ranked.starts_with(&["git.log"]) || ranked.starts_with(&["git.push"]));
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
        // Both match, but only the builtin starts with the query.
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
    fn a_word_start_outranks_a_scattered_match() {
        let entries = vec![
            entry(
                "scattered",
                // "pod" appears only scattered across "port-forward doc".
                "kubectl port-forward doc",
                "",
                &[],
                Layer::Builtin,
            ),
            entry("word.start", "kubectl get pods", "", &[], Layer::Builtin),
        ];
        assert_eq!(
            order(&entries, &HashMap::new(), "pod"),
            ["word.start", "scattered"]
        );
    }

    #[test]
    fn ordering_is_stable_when_nothing_distinguishes_entries() {
        let entries = sample();
        let first = order(&entries, &HashMap::new(), "");
        let second = order(&entries, &HashMap::new(), "");
        assert_eq!(first, second);
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
            let elapsed = started.elapsed();
            println!(
                "query {:>9?}: {:>5} matches in {:>8.3?}",
                query,
                ranked.len(),
                elapsed
            );
        }
    }

    #[test]
    fn highlight_reports_matched_positions() {
        let mut ranker = Ranker::new();
        assert_eq!(ranker.highlight("docker ps", "dps"), [0, 7, 8]);
        assert!(ranker.highlight("docker ps", "").is_empty());
    }
}

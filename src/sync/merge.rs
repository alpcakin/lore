//! Combining two copies of a library that changed apart.
//!
//! Git moves the file between machines, but it merges by line, and two
//! machines that each saved a command have both added lines at the end of the
//! same list. Git calls that a conflict although nothing clashes. The merge
//! here works on entries by id instead, so the only real conflict is one entry
//! changed differently in two places.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use crate::model::Entry;
use crate::store::definitions::{self, NewEntry};

/// The outcome of combining this machine's library with the synced one.
#[derive(Debug)]
pub struct Merged {
    /// The library as it should now be, on this machine and in the repository.
    pub text: String,
    /// Changes taken from other machines.
    pub received: usize,
    /// Changes this machine had made since it last synced.
    pub sent: usize,
    /// Entries that were changed on both sides. The first id holds the
    /// version that reached the repository first, and the second holds this
    /// machine's version, kept under a new id rather than lost.
    pub kept_both: Vec<(String, String)>,
}

/// One copy of a library, reduced to what the merge compares.
struct Side {
    entries: BTreeMap<String, Entry>,
    /// Ids in the order the file declares them, so additions arrive in the
    /// order they were made.
    order: Vec<String>,
    disabled: BTreeSet<String>,
}

impl Side {
    fn parse(text: &str, origin: &str) -> Result<Self> {
        let library = definitions::parse_user(text, origin)?;
        let order = library.commands.iter().map(|e| e.id.clone()).collect();
        let entries = library
            .commands
            .into_iter()
            .map(|entry| (entry.id.clone(), entry))
            .collect();

        Ok(Self {
            entries,
            order,
            disabled: library.disabled.into_iter().collect(),
        })
    }
}

/// Merges `ours` and `theirs`, both descended from `base`.
///
/// `base` is the library as it was the last time this machine synced, and
/// `None` when it never has. `theirs` is `None` when the repository holds no
/// library yet.
///
/// When only one side changed, its text is taken whole, so nothing about how
/// either file is written changes for no reason. Only when both changed is the
/// other side's work spliced into this machine's text, entry by entry, which
/// leaves this machine's comments and ordering where they were.
///
/// When both sides changed the same entry, the repository's version keeps the
/// id and this machine's is added under a new one. Deciding it that way on
/// every machine means they agree after one round instead of passing their
/// differences back and forth.
pub fn merge(base: Option<&str>, ours: &str, theirs: Option<&str>) -> Result<Merged> {
    let base = Side::parse(base.unwrap_or_default(), "the library as last synced")?;
    let local = Side::parse(ours, "this machine's library")?;
    let sent = changes(&base, &local);

    let unchanged = |text: &str, received| Merged {
        text: text.to_string(),
        received,
        sent,
        kept_both: Vec::new(),
    };

    let Some(theirs) = theirs else {
        return Ok(unchanged(ours, 0));
    };
    let remote = Side::parse(theirs, "the synced library")?;
    let incoming = changes(&base, &remote);

    if incoming == 0 {
        return Ok(unchanged(ours, 0));
    }
    if sent == 0 {
        return Ok(Merged {
            text: theirs.to_string(),
            received: incoming,
            sent: 0,
            kept_both: Vec::new(),
        });
    }

    let mut text = ours.to_string();
    let mut received = 0;
    let mut kept_both = Vec::new();
    let mut taken: BTreeSet<String> = local
        .entries
        .keys()
        .chain(remote.entries.keys())
        .cloned()
        .collect();

    let mut ids = remote.order.clone();
    for id in local.order.iter().chain(&base.order) {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }

    for id in ids {
        let was = base.entries.get(&id);
        let mine = local.entries.get(&id);
        let other = remote.entries.get(&id);

        // Nothing to take: the two agree, or only this side changed.
        if mine == other || other == was {
            continue;
        }

        let untouched_here = mine == was;
        received += 1;
        match (mine, other) {
            // New or changed there, and either untouched here or removed here:
            // an edit is kept rather than lost to a removal.
            (None, Some(other)) => {
                text = definitions::upserted(&text, &NewEntry::from(other))?.0;
            }
            (Some(_), Some(other)) if untouched_here => {
                text = definitions::upserted(&text, &NewEntry::from(other))?.0;
            }
            (Some(mine), Some(other)) => {
                let copy = unused(&id, &taken);
                taken.insert(copy.clone());

                text = definitions::upserted(&text, &NewEntry::from(other))?.0;
                let mut kept = NewEntry::from(mine);
                kept.id = copy.clone();
                text = definitions::appended(&text, &kept)?;

                kept_both.push((id, copy));
            }
            // Removed there and untouched here.
            (Some(_), None) if untouched_here => {
                text = definitions::removed(&text, &id).unwrap_or(text);
            }
            // Changed here and removed there: this machine's version stands
            // and goes back up with the next push.
            (Some(_), None) | (None, None) => received -= 1,
        }
    }

    let patterns: BTreeSet<&String> = base
        .disabled
        .iter()
        .chain(&local.disabled)
        .chain(&remote.disabled)
        .collect();

    for pattern in patterns {
        let was = base.disabled.contains(pattern);
        let mine = local.disabled.contains(pattern);
        let other = remote.disabled.contains(pattern);

        // Either the two agree or only this side changed. With a yes or no
        // there is no third way for both to have changed.
        if mine == other || other == was {
            continue;
        }

        text = if other {
            definitions::with_disabled(&text, pattern)?
        } else {
            definitions::with_enabled(&text, pattern)
        };
        received += 1;
    }

    // Whatever went wrong splicing, the result is never pushed unless it
    // loads.
    Side::parse(&text, "the merged library")?;

    Ok(Merged {
        text,
        received,
        sent,
        kept_both,
    })
}

/// How many entries and hidden patterns differ between two copies.
fn changes(from: &Side, to: &Side) -> usize {
    let ids: BTreeSet<&String> = from.entries.keys().chain(to.entries.keys()).collect();
    let entries = ids
        .into_iter()
        .filter(|id| from.entries.get(*id) != to.entries.get(*id))
        .count();

    entries + from.disabled.symmetric_difference(&to.disabled).count()
}

/// The first of `id-2`, `id-3` and so on that nothing is using.
fn unused(id: &str, taken: &BTreeSet<String>) -> String {
    (2..)
        .map(|n| format!("{id}-{n}"))
        .find(|candidate| !taken.contains(candidate))
        .expect("the sequence is unbounded")
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "version: 1\ncommands:\n";

    fn item(id: &str, desc: &str) -> String {
        format!("  - id: {id}\n    cmd: echo {id}\n    desc: {desc}\n")
    }

    fn library(items: &[(&str, &str)]) -> String {
        let mut text = HEADER.to_string();
        for (id, desc) in items {
            text.push_str(&item(id, desc));
        }
        text
    }

    fn entries(text: &str) -> Vec<(String, String)> {
        definitions::parse_user(text, "test")
            .unwrap()
            .commands
            .into_iter()
            .map(|entry| (entry.id, entry.desc))
            .collect()
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(id, desc)| (id.to_string(), desc.to_string()))
            .collect()
    }

    #[test]
    fn a_first_sync_to_an_empty_repository_sends_everything() {
        let ours = library(&[("a", "A")]);
        let merged = merge(None, &ours, None).unwrap();

        assert_eq!(merged.text, ours);
        assert_eq!((merged.received, merged.sent), (0, 1));
    }

    #[test]
    fn a_new_machine_takes_the_library_as_it_is() {
        let theirs = format!("# my commands\n{}", library(&[("a", "A")]));
        let merged = merge(None, "", Some(&theirs)).unwrap();

        assert_eq!(
            merged.text, theirs,
            "the other machine's comments were lost"
        );
        assert_eq!((merged.received, merged.sent), (1, 0));
    }

    #[test]
    fn nothing_changed_anywhere_is_nothing_to_do() {
        let text = library(&[("a", "A")]);
        let merged = merge(Some(&text), &text, Some(&text)).unwrap();

        assert_eq!(merged.text, text);
        assert_eq!((merged.received, merged.sent), (0, 0));
    }

    /// The case line based merging gets wrong: both machines added to the end
    /// of the same list.
    #[test]
    fn two_machines_adding_different_commands_keep_both() {
        let base = library(&[("a", "A")]);
        let ours = library(&[("a", "A"), ("mine", "Mine")]);
        let theirs = library(&[("a", "A"), ("theirs", "Theirs")]);

        let merged = merge(Some(&base), &ours, Some(&theirs)).unwrap();

        assert_eq!(
            entries(&merged.text),
            pairs(&[("a", "A"), ("mine", "Mine"), ("theirs", "Theirs")])
        );
        assert_eq!((merged.received, merged.sent), (1, 1));
        assert!(merged.kept_both.is_empty());
    }

    #[test]
    fn this_machines_comments_survive_a_merge() {
        let base = library(&[("a", "A")]);
        let ours = format!(
            "# kept by hand\n{}",
            library(&[("a", "A"), ("mine", "Mine")])
        );
        let theirs = library(&[("a", "A"), ("theirs", "Theirs")]);

        let merged = merge(Some(&base), &ours, Some(&theirs)).unwrap();
        assert!(
            merged.text.starts_with("# kept by hand\n"),
            "{}",
            merged.text
        );
    }

    #[test]
    fn a_removal_elsewhere_reaches_this_machine() {
        let base = library(&[("a", "A"), ("b", "B")]);
        let ours = library(&[("a", "A"), ("b", "B"), ("mine", "Mine")]);
        let theirs = library(&[("a", "A")]);

        let merged = merge(Some(&base), &ours, Some(&theirs)).unwrap();
        assert_eq!(
            entries(&merged.text),
            pairs(&[("a", "A"), ("mine", "Mine")])
        );
    }

    #[test]
    fn an_edit_elsewhere_reaches_this_machine() {
        let base = library(&[("a", "A"), ("b", "B")]);
        let ours = library(&[("a", "A"), ("b", "B"), ("mine", "Mine")]);
        let theirs = library(&[("a", "A changed"), ("b", "B")]);

        let merged = merge(Some(&base), &ours, Some(&theirs)).unwrap();
        assert_eq!(
            entries(&merged.text),
            pairs(&[("a", "A changed"), ("b", "B"), ("mine", "Mine")])
        );
    }

    #[test]
    fn an_entry_edited_in_both_places_is_kept_twice() {
        let base = library(&[("a", "A")]);
        let ours = library(&[("a", "Mine")]);
        let theirs = library(&[("a", "Theirs")]);

        let merged = merge(Some(&base), &ours, Some(&theirs)).unwrap();

        assert_eq!(
            entries(&merged.text),
            pairs(&[("a", "Theirs"), ("a-2", "Mine")])
        );
        assert_eq!(merged.kept_both, [("a".to_string(), "a-2".to_string())]);
    }

    /// The other machine then syncs against what this one pushed. It must
    /// simply take the copy, not treat its own version as a fresh conflict.
    #[test]
    fn a_conflict_settles_after_one_round() {
        let base = library(&[("a", "A")]);
        let pushed_first = library(&[("a", "Theirs")]);
        let merged = merge(Some(&base), &library(&[("a", "Mine")]), Some(&pushed_first)).unwrap();

        // The machine that pushed first now syncs with no changes of its own.
        let settled = merge(Some(&pushed_first), &pushed_first, Some(&merged.text)).unwrap();

        assert_eq!(settled.text, merged.text);
        assert!(settled.kept_both.is_empty());
    }

    #[test]
    fn an_edit_wins_over_a_removal_on_the_other_side() {
        let base = library(&[("a", "A"), ("b", "B")]);

        let edited_here = library(&[("a", "Edited"), ("b", "B")]);
        let removed_there = library(&[("b", "B")]);
        let merged = merge(Some(&base), &edited_here, Some(&removed_there)).unwrap();
        assert_eq!(entries(&merged.text), pairs(&[("a", "Edited"), ("b", "B")]));

        let removed_here = library(&[("b", "B"), ("c", "C")]);
        let edited_there = library(&[("a", "Edited"), ("b", "B")]);
        let merged = merge(Some(&base), &removed_here, Some(&edited_there)).unwrap();
        assert_eq!(
            entries(&merged.text),
            pairs(&[("b", "B"), ("c", "C"), ("a", "Edited")])
        );
    }

    #[test]
    fn hidden_builtins_merge_like_entries() {
        let base = format!("{}disabled:\n  - old.*\n", library(&[("a", "A")]));
        let ours = format!(
            "{}disabled:\n  - old.*\n",
            library(&[("a", "A"), ("m", "M")])
        );
        let theirs = format!("{}disabled:\n  - new.*\n", library(&[("a", "A")]));

        let merged = merge(Some(&base), &ours, Some(&theirs)).unwrap();
        let hidden = definitions::parse_user(&merged.text, "test")
            .unwrap()
            .disabled;

        assert_eq!(hidden, ["new.*"]);
        assert_eq!(entries(&merged.text), pairs(&[("a", "A"), ("m", "M")]));
    }

    #[test]
    fn a_broken_library_is_refused_rather_than_synced() {
        let error = merge(None, "version: 1\ncommands: [oops", None).unwrap_err();
        assert!(format!("{error:#}").contains("this machine's library"));
    }
}

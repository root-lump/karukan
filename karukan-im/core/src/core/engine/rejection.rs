//! Rejected commits: a conversion committed and then immediately erased.
//!
//! Committing 「computer」, BackSpacing it all away and typing the reading
//! again says the commit was wrong. Two things follow:
//!
//! - The learning record that commit made is undone (restored to what it
//!   was before), so the rejected surface does not come back at learning
//!   priority.
//! - The next conversion of the same reading demotes the rejected surface
//!   from the head of the list to second place, so the default selection
//!   is something else while the rejected one stays one step away.
//!
//! Only the engine's own key stream is watched, so it works the same on
//! every frontend: after a commit, BackSpace in the Empty state counts
//! down the committed characters (a modified BackSpace — a word delete —
//! counts as erasing it all), and the next printable key decides. Any
//! other key in between (arrows, Enter, Delete, Space…) means the user
//! moved on, and the commit is forgotten.

use karukan_engine::learning::LearningEntry;

use super::*;

/// The last commit, while it may still be erased.
#[derive(Debug)]
pub(super) struct RecentCommit {
    /// Reading of the whole commit (all segments).
    reading: String,
    /// Text that was committed.
    surface: String,
    /// Committed chars not yet BackSpaced away.
    remaining: usize,
    /// Learning records the commit made, each with its entry as it was
    /// before (`None`: the commit created it), in recording order.
    learned: Vec<LearnedRecord>,
}

/// One learning record made by a commit, with the entry it replaced.
pub(super) type LearnedRecord = (String, String, Option<LearningEntry>);

/// A commit the user erased, waiting for its reading to come back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RejectedCommit {
    pub(super) reading: String,
    pub(super) surface: String,
}

impl InputMethodEngine {
    /// Remember a finished commit so erasing it can be recognized. Takes the
    /// learning records made since the last commit and clears a previous
    /// rejection: whatever the user committed this time answers it.
    pub(super) fn note_commit(&mut self, reading: &str, surface: &str) {
        let learned = std::mem::take(&mut self.commit_learned);
        self.rejected_commit = None;
        self.recent_commit = (!surface.is_empty() && !reading.is_empty()).then(|| RecentCommit {
            reading: reading.to_string(),
            surface: surface.to_string(),
            remaining: surface.chars().count(),
            learned,
        });
    }

    /// A commit that cannot be erased as a unit (focus-out, a partial
    /// commit that resumes composing): forget it and its records.
    pub(super) fn forget_commit(&mut self) {
        self.commit_learned.clear();
        self.recent_commit = None;
    }

    /// Watch a key arriving in the Empty state after a commit.
    pub(super) fn track_key_after_commit(&mut self, key: &KeyEvent) {
        let Some(recent) = &mut self.recent_commit else {
            return;
        };
        if key.keysym == Keysym::BACKSPACE {
            if key.modifiers.control_key || key.modifiers.alt_key {
                recent.remaining = 0;
            } else {
                recent.remaining = recent.remaining.saturating_sub(1);
            }
            return;
        }
        let recent = self.recent_commit.take().expect("checked above");
        let printable = key.keysym != Keysym::SPACE
            && key.to_char().is_some()
            && !key.modifiers.control_key
            && !key.modifiers.alt_key;
        if printable && recent.remaining == 0 {
            self.reject_commit(recent);
        }
    }

    /// The user erased `recent` and is typing again: undo its learning and
    /// arm the demotion for its reading.
    fn reject_commit(&mut self, recent: RecentCommit) {
        if let Some(cache) = &mut self.learning {
            // Newest first, so a pair recorded twice ends at its oldest state.
            for (reading, surface, prior) in recent.learned.into_iter().rev() {
                cache.restore(&reading, &surface, prior);
            }
        }
        self.rejected_commit = Some(RejectedCommit {
            reading: recent.reading,
            surface: recent.surface,
        });
    }

    /// Demote the rejected surface when `reading` is the reading it was
    /// committed for and it heads the list: it moves to second place.
    pub(super) fn demote_rejected(&self, reading: &str, candidates: &mut [AnnotatedCandidate]) {
        let Some(rejected) = &self.rejected_commit else {
            return;
        };
        if rejected.reading == reading
            && candidates.len() > 1
            && candidates[0].text == rejected.surface
        {
            candidates.swap(0, 1);
        }
    }

    /// Whether `text` is the rejected surface for `reading`.
    pub(super) fn is_rejected(&self, reading: &str, text: &str) -> bool {
        self.rejected_commit
            .as_ref()
            .is_some_and(|r| r.reading == reading && r.surface == text)
    }
}

//! Commands, actions and the undo stack.
//!
//! One action is one batch of commands, applied atomically, recorded as one revision and undone
//! in one step (ANN-3, AGT-3). History is linear; a new action clears redo.

use crate::action::{Actor, Revision};
use crate::error::CmError;
use std::fmt;
use time::OffsetDateTime;

/// Revisions kept in the manifest log (ANN-3: last 200).
pub const DEFAULT_REVISION_LIMIT: usize = 200;

/// A reversible edit of a document `D`.
///
/// `apply` may fail and must then leave `doc` unchanged. `undo` is only called after a
/// successful `apply` (or redo), so it must restore the state `apply` replaced; commands capture
/// what they need to do so during `apply`.
pub trait Command<D>: fmt::Debug + Send {
    fn apply(&mut self, doc: &mut D) -> Result<(), CmError>;
    fn undo(&mut self, doc: &mut D);
}

/// A named batch of commands from one actor; one undo step.
pub struct Action<D> {
    name: String,
    by: Actor,
    commands: Vec<Box<dyn Command<D>>>,
}

impl<D> Action<D> {
    pub fn new(name: impl Into<String>, by: Actor) -> Self {
        Action {
            name: name.into(),
            by,
            commands: Vec::new(),
        }
    }

    pub fn with(mut self, command: impl Command<D> + 'static) -> Self {
        self.commands.push(Box::new(command));
        self
    }

    pub fn push(&mut self, command: impl Command<D> + 'static) {
        self.commands.push(Box::new(command));
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn by(&self) -> &Actor {
        &self.by
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl<D> fmt::Debug for Action<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Action")
            .field("name", &self.name)
            .field("by", &self.by)
            .field("commands", &self.commands)
            .finish()
    }
}

/// Unlimited linear undo/redo plus the revision log.
pub struct UndoStack<D> {
    undo: Vec<Action<D>>,
    redo: Vec<Action<D>>,
    revisions: Vec<Revision>,
    next_rev: u64,
    revision_limit: usize,
}

impl<D> Default for UndoStack<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D> fmt::Debug for UndoStack<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UndoStack")
            .field("undo", &self.undo.len())
            .field("redo", &self.redo.len())
            .field("revisions", &self.revisions.len())
            .finish()
    }
}

impl<D> UndoStack<D> {
    pub fn new() -> Self {
        Self::with_revision_limit(DEFAULT_REVISION_LIMIT)
    }

    /// Caps the revision log (oldest dropped first); the undo stack itself is unbounded.
    pub fn with_revision_limit(limit: usize) -> Self {
        UndoStack {
            undo: Vec::new(),
            redo: Vec::new(),
            revisions: Vec::new(),
            next_rev: 1,
            revision_limit: limit,
        }
    }

    /// Continues numbering and the log from revisions loaded from a manifest.
    /// The undo stack starts empty (see cm-library for persisted history).
    pub fn resume(mut revisions: Vec<Revision>, revision_limit: usize) -> Self {
        let next_rev = revisions.iter().map(|r| r.rev).max().map_or(1, |m| m + 1);
        let excess = revisions.len().saturating_sub(revision_limit);
        revisions.drain(..excess);
        UndoStack {
            undo: Vec::new(),
            redo: Vec::new(),
            revisions,
            next_rev,
            revision_limit,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Name of the action the next undo would revert.
    pub fn next_undo_name(&self) -> Option<&str> {
        self.undo.last().map(Action::name)
    }

    pub fn next_redo_name(&self) -> Option<&str> {
        self.redo.last().map(Action::name)
    }

    pub fn revisions(&self) -> &[Revision] {
        &self.revisions
    }

    /// Applies every command in order, all or nothing: on failure the already-applied commands
    /// are undone, nothing is recorded, and the error is returned (AGT-3.AC3).
    /// On success pushes one undo step, clears redo and records one revision.
    pub fn execute(
        &mut self,
        doc: &mut D,
        mut action: Action<D>,
        at: OffsetDateTime,
    ) -> Result<Revision, CmError> {
        if action.commands.is_empty() {
            return Err(CmError::invalid_field(
                "commands",
                "an action needs at least one command",
            ));
        }
        apply_all(&mut action.commands, doc)?;
        self.redo.clear();
        let rev = self.record(&action.name, action.by.clone(), at);
        self.undo.push(action);
        Ok(rev)
    }

    /// Reverts the latest action as one step; `None` when there is nothing to undo.
    /// Records a revision named `undo` by `by`.
    pub fn undo(&mut self, doc: &mut D, by: Actor, at: OffsetDateTime) -> Option<Revision> {
        let mut action = self.undo.pop()?;
        for c in action.commands.iter_mut().rev() {
            c.undo(doc);
        }
        self.redo.push(action);
        Some(self.record("undo", by, at))
    }

    /// Re-applies the latest undone action; `Ok(None)` when there is nothing to redo. If a
    /// command fails, the action is rolled back and stays on the redo stack.
    pub fn redo(
        &mut self,
        doc: &mut D,
        by: Actor,
        at: OffsetDateTime,
    ) -> Result<Option<Revision>, CmError> {
        let Some(mut action) = self.redo.pop() else {
            return Ok(None);
        };
        if let Err(e) = apply_all(&mut action.commands, doc) {
            self.redo.push(action);
            return Err(e);
        }
        self.undo.push(action);
        Ok(Some(self.record("redo", by, at)))
    }

    fn record(&mut self, action: &str, by: Actor, at: OffsetDateTime) -> Revision {
        let rev = Revision {
            rev: self.next_rev,
            action: action.to_owned(),
            by,
            at,
        };
        self.next_rev += 1;
        self.revisions.push(rev.clone());
        let excess = self.revisions.len().saturating_sub(self.revision_limit);
        self.revisions.drain(..excess);
        rev
    }
}

/// Applies commands in order; on error rolls back the applied prefix.
fn apply_all<D>(commands: &mut [Box<dyn Command<D>>], doc: &mut D) -> Result<(), CmError> {
    for i in 0..commands.len() {
        if let Err(e) = commands[i].apply(doc) {
            for done in commands[..i].iter_mut().rev() {
                done.undo(doc);
            }
            return Err(e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{AddItem, Fail, SetTitle, TestDoc};
    use time::macros::datetime;

    const T: OffsetDateTime = datetime!(2026-10-19 09:00 UTC);

    fn user() -> Actor {
        Actor::User
    }
    fn claude() -> Actor {
        Actor::agent("claude").unwrap()
    }
    fn add(n: usize, by: Actor) -> Action<TestDoc> {
        Action::new("add", by).with(AddItem(format!("item{n}")))
    }

    #[test]
    fn ann_3_ac1_five_edits_five_undos_restore_original() {
        let mut doc = TestDoc::default();
        let original = doc.clone();
        let mut st = UndoStack::new();
        for n in 0..5 {
            st.execute(&mut doc, add(n, user()), T).unwrap();
        }
        assert_eq!(doc.items.len(), 5);
        for _ in 0..5 {
            assert!(st.undo(&mut doc, user(), T).is_some());
        }
        assert_eq!(doc, original);
        assert!(st.undo(&mut doc, user(), T).is_none());
    }

    #[test]
    fn ann_3_ac2_agt_3_ac1_batch_is_one_undo_step() {
        let mut doc = TestDoc::default();
        let mut st = UndoStack::new();
        let mut a = Action::new("annotate", claude());
        for n in 0..10 {
            a.push(AddItem(format!("op{n}")));
        }
        let rev = st.execute(&mut doc, a, T).unwrap();
        assert_eq!(rev.by, claude());
        assert_eq!(st.undo_len(), 1);
        assert_eq!(doc.items.len(), 10);
        st.undo(&mut doc, user(), T).unwrap();
        assert!(doc.items.is_empty());
    }

    #[test]
    fn agt_3_ac3_failed_batch_applies_nothing_and_records_nothing() {
        let mut doc = TestDoc::default();
        let mut st = UndoStack::new();
        st.execute(&mut doc, add(0, user()), T).unwrap();
        let before = doc.clone();
        let bad = Action::new("cut", claude())
            .with(AddItem("a".into()))
            .with(SetTitle::new("t"))
            .with(Fail)
            .with(AddItem("never".into()));
        let err = st.execute(&mut doc, bad, T).unwrap_err();
        assert_eq!(err.code(), crate::ErrorCode::InvalidInput);
        assert_eq!(doc, before);
        assert_eq!(st.undo_len(), 1);
        assert_eq!(st.revisions().len(), 1);
    }

    #[test]
    fn redo_restores_and_new_action_clears_redo() {
        let mut doc = TestDoc::default();
        let mut st = UndoStack::new();
        st.execute(&mut doc, add(0, user()), T).unwrap();
        st.execute(
            &mut doc,
            Action::new("title", user()).with(SetTitle::new("A")),
            T,
        )
        .unwrap();
        st.undo(&mut doc, user(), T).unwrap();
        assert_eq!(doc.title, "");
        assert!(st.can_redo());
        assert_eq!(st.next_redo_name(), Some("title"));
        st.redo(&mut doc, user(), T).unwrap().unwrap();
        assert_eq!(doc.title, "A");
        st.undo(&mut doc, user(), T).unwrap();
        st.execute(&mut doc, add(1, user()), T).unwrap();
        assert!(!st.can_redo());
        assert_eq!(st.redo(&mut doc, user(), T).unwrap(), None);
    }

    #[test]
    fn agt_3_linear_undo_reverts_later_user_edits_first() {
        let mut doc = TestDoc::default();
        let mut st = UndoStack::new();
        st.execute(&mut doc, add(0, claude()), T).unwrap();
        st.execute(&mut doc, add(1, user()), T).unwrap();
        st.undo(&mut doc, user(), T).unwrap();
        assert_eq!(doc.items, vec!["item0"]);
        assert_eq!(st.next_undo_name(), Some("add"));
    }

    #[test]
    fn unlimited_undo_depth() {
        let mut doc = TestDoc::default();
        let mut st = UndoStack::with_revision_limit(10);
        for n in 0..1000 {
            st.execute(&mut doc, add(n, user()), T).unwrap();
        }
        assert_eq!(st.undo_len(), 1000);
        for _ in 0..1000 {
            st.undo(&mut doc, user(), T).unwrap();
        }
        assert!(doc.items.is_empty());
    }

    #[test]
    fn revisions_numbered_attributed_and_capped() {
        let mut doc = TestDoc::default();
        let mut st = UndoStack::with_revision_limit(3);
        let r1 = st.execute(&mut doc, add(0, claude()), T).unwrap();
        assert_eq!((r1.rev, r1.action.as_str()), (1, "add"));
        let r2 = st.undo(&mut doc, user(), T).unwrap();
        assert_eq!((r2.rev, r2.action.as_str(), &r2.by), (2, "undo", &user()));
        let r3 = st.redo(&mut doc, claude(), T).unwrap().unwrap();
        assert_eq!(r3.action, "redo");
        st.execute(&mut doc, add(1, user()), T).unwrap();
        let revs: Vec<u64> = st.revisions().iter().map(|r| r.rev).collect();
        assert_eq!(revs, vec![2, 3, 4]);
    }

    #[test]
    fn resume_continues_numbering() {
        let mut doc = TestDoc::default();
        let mut a = UndoStack::new();
        a.execute(&mut doc, add(0, user()), T).unwrap();
        a.execute(&mut doc, add(1, user()), T).unwrap();
        let mut b: UndoStack<TestDoc> = UndoStack::resume(a.revisions().to_vec(), 200);
        assert!(!b.can_undo());
        let r = b.execute(&mut doc, add(2, user()), T).unwrap();
        assert_eq!(r.rev, 3);
    }

    #[test]
    fn empty_action_is_invalid() {
        let mut doc = TestDoc::default();
        let mut st = UndoStack::new();
        let err = st
            .execute(&mut doc, Action::new("noop", user()), T)
            .unwrap_err();
        assert_eq!(err.code(), crate::ErrorCode::InvalidInput);
        assert!(st.revisions().is_empty());
    }

    #[test]
    fn failed_redo_rolls_back_and_keeps_entry() {
        // A command that applies once, then fails on re-apply.
        #[derive(Debug)]
        struct Once(bool);
        impl Command<TestDoc> for Once {
            fn apply(&mut self, doc: &mut TestDoc) -> Result<(), CmError> {
                if self.0 {
                    return Err(CmError::busy("again"));
                }
                self.0 = true;
                doc.items.push("once".into());
                Ok(())
            }
            fn undo(&mut self, doc: &mut TestDoc) {
                doc.items.pop();
            }
        }
        let mut doc = TestDoc::default();
        let mut st = UndoStack::new();
        st.execute(
            &mut doc,
            Action::new("x", user())
                .with(AddItem("a".into()))
                .with(Once(false)),
            T,
        )
        .unwrap();
        st.undo(&mut doc, user(), T).unwrap();
        assert!(st.redo(&mut doc, user(), T).is_err());
        assert!(doc.items.is_empty());
        assert_eq!(st.redo_len(), 1);
    }
}

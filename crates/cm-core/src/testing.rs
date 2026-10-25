//! A simple document and commands for exercising actions and undo (also for downstream crates
//! behind the `testing` feature).

use crate::error::CmError;
use crate::undo::Command;

/// Minimal document: a title and a list of items.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestDoc {
    pub title: String,
    pub items: Vec<String>,
}

/// Appends an item.
#[derive(Debug)]
pub struct AddItem(pub String);

impl Command<TestDoc> for AddItem {
    fn apply(&mut self, doc: &mut TestDoc) -> Result<(), CmError> {
        doc.items.push(self.0.clone());
        Ok(())
    }
    fn undo(&mut self, doc: &mut TestDoc) {
        doc.items.pop();
    }
}

/// Sets the title, remembering the previous one.
#[derive(Debug)]
pub struct SetTitle {
    new: String,
    old: Option<String>,
}

impl SetTitle {
    pub fn new(title: impl Into<String>) -> Self {
        SetTitle {
            new: title.into(),
            old: None,
        }
    }
}

impl Command<TestDoc> for SetTitle {
    fn apply(&mut self, doc: &mut TestDoc) -> Result<(), CmError> {
        self.old = Some(std::mem::replace(&mut doc.title, self.new.clone()));
        Ok(())
    }
    fn undo(&mut self, doc: &mut TestDoc) {
        if let Some(old) = self.old.take() {
            doc.title = old;
        }
    }
}

/// Always fails on apply (injected failure for atomicity tests).
#[derive(Debug)]
pub struct Fail;

impl Command<TestDoc> for Fail {
    fn apply(&mut self, _doc: &mut TestDoc) -> Result<(), CmError> {
        Err(CmError::invalid_input("injected failure"))
    }
    fn undo(&mut self, _doc: &mut TestDoc) {}
}

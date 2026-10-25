//! Capture bundles (`.cmshot` / `.cmrec`), manifests, the annotation op model, the SQLite index
//! with full-text search, and thumbnails.
//!
//! Bundles are the source of truth; the index (`<root>/.capturemann/index.sqlite`) only caches
//! them and is rebuilt by [`Library::rescan`] whenever it is missing, corrupted or from another
//! version (LIB-1.AC3).
//!
//! # Layout of a screenshot bundle
//!
//! ```text
//! <title>.cmshot/
//! ├─ original.png       read-only after capture; SHA-256 recorded in the manifest
//! ├─ manifest.json      schema 1, id, title, created, ops, revisions, history, render info
//! ├─ render.png         flattened render, written by the caller after cm-render ran
//! └─ thumbs/poster.jpg  grid thumbnail
//! ```
//!
//! # Modules
//!
//! * [`ops`]: the typed, serde-tagged annotation op model (`arrow`, `line`, `rect`, `ellipse`,
//!   `text`, `highlight`, `step`, `blur`, `pixelate`, `crop`). `cm-render` consumes
//!   [`ops::AnnotationOp`]; agents and the CLI parse JSON with [`ops::parse_ops`].
//! * [`manifest`]: [`manifest::ShotManifest`] (full) and [`manifest::RecManifest`] (typed
//!   header only; the `.cmrec` track/edit model is M3+).
//! * [`bundle`]: [`ShotBundle`] create/open/edit/save.
//! * [`rec`]: [`RecBundle`] (`.cmrec` create, finalize) and crash recovery (REC-8):
//!   [`Library::recover_incomplete`].
//! * [`history`]: persisted undo data.
//! * [`index`]: [`Library`], the index, search and rescan.
//! * [`thumbs`]: poster generation.
//!
//! # Forward compatibility
//!
//! * `schema` greater than [`manifest::SCHEMA_VERSION`] is refused with an `invalid_input`
//!   error that names both versions; the file is never modified.
//! * Unknown top-level manifest fields are preserved (`extra`) and written back.
//! * Unknown fields inside an individual op are ignored (and dropped on the next save).
//! * Stored ops this build cannot read (a newer build's op types at the same schema version,
//!   or an op that fails validation) are kept as raw JSON in
//!   [`manifest::ShotManifest::unknown_ops`], written back untouched at their place in the
//!   list, never rendered, and reported in [`ShotBundle::warnings`] (R1-4). Input from clients
//!   (`annotate`, `--ops`) still goes through [`ops::parse_ops`], where an unknown op type is
//!   an error naming `ops[<index>]`. Likewise an unreadable `history` (new change kinds) is
//!   kept verbatim until the first edit replaces it; undo is unavailable meanwhile.
//! * A manifest that cannot be parsed at all is moved to `manifest.broken.json` (or
//!   `manifest.broken-<unix_ts>.json` when that exists: a quarantine is never overwritten) and
//!   the bundle opens with no ops plus a warning (ANN-3 edge case).
//! * `original.png` and `render.png` are written by `png`/`image` with only the IHDR, IDAT and
//!   IEND chunks (ANN-2.AC3), so they carry no color profile (no `sRGB` chunk); viewers assume
//!   sRGB. A wide-gamut display capture is stored as the captured RGB values, unconverted.
//!
//! # Undo after restart (resolves the open question from cm-core)
//!
//! `cm_core::UndoStack` keeps its commands only in memory and cannot be seeded from disk, so
//! undo data must be persisted by this crate. Scheme:
//!
//! 1. Every edit is a list of [`history::Change`]s. Each variant stores both the forward and
//!    the inverse data (`ReplaceOp { before, after }`, `RemoveOp { index, op }`,
//!    `ClearOps { ops }`, `ReorderOps { before, after }` (ids only), ...). `annotate { replace:
//!    true }` over existing ops is stored as a diff by op id (untouched ops cost nothing, changed
//!    ones `ReplaceOp`, vanished `RemoveOp`, new `AddOp`, a new order one `ReorderOps`) and is
//!    still one step; an identical list records nothing. `manifest.history` is written as one
//!    compact line while the rest of the manifest stays pretty. A `Change` implements `Command<ShotManifest>`, so the live
//!    session and the stored form are the same thing.
//! 2. `ShotBundle` mirrors the stack in two logs of [`history::StoredAction`]s (undo steps,
//!    oldest first; redo steps, next redo last). [`ShotBundle::save`] writes them to
//!    `manifest.history`, capped at [`bundle::HISTORY_LIMIT`] (200) steps each.
//! 3. On open, the stack is rebuilt by replay: the manifest is cloned, the stored changes are
//!    rewound on the clone, then re-executed forward through a fresh `UndoStack`, then the redo
//!    part is undone again. If the replayed ops differ from `manifest.ops` the history is
//!    dropped with a warning; `ops` itself is always authoritative.
//!
//! `manifest.revisions` (the spec's revision log: number, action, `by`, time) is kept in step,
//! capped at 200, and includes `undo`/`redo` entries. Revision numbers are assigned by
//! `ShotBundle` from the log (monotonic across sessions); the stack's own numbering restarts
//! at 1 each session and is ignored. History cost is roughly the op data per step, so a
//! 200-step history stays in the tens of KB for typical annotation work.
//!
//! # Other decisions
//!
//! * `original.png` is written once with `create_new`, marked read-only, and never opened for
//!   writing again; [`ShotBundle::verify_original`] re-hashes it (ANN-3.AC4).
//! * Copying a bundle folder by hand yields two bundles with one capture id: [`Library::rescan`]
//!   keeps the folder already indexed for the id (else the oldest, then the shortest name) and
//!   lists both in `RescanReport::duplicates`. [`Library::duplicate`] copies with a fresh id.
//! * `render.png` is not produced here (no dependency on cm-render): callers pass the PNG bytes
//!   to [`ShotBundle::write_render`], which records the revision it reflects so
//!   [`ShotBundle::render_is_stale`] works. Writing a render is not an undo step.
//! * Mutations are in memory until `save()`; [`Library::save_shot`] saves, refreshes the poster
//!   and updates the index row in one call (CAP-4).
//! * Index paths are relative to the library root, so relocating the library (LIB-2) only needs
//!   a new `Library::open`.
//! * Errors use `cm_core::CmError`. I/O and SQLite failures map to `not_found`/`disk_full`
//!   where the kind is clear, otherwise `invalid_input` (the shared code table has no generic
//!   internal error; flagged as an open question).
//!
//! # Not implemented here (open questions or out of scope for M1)
//!
//! * Moving a library with verification (LIB-2.AC1/AC4), fallback folders, and Trash deletion
//!   (LIB-1.AC4): they need the daemon/storage layers and a trash dependency decision.
//! * Search over destination/platform names and the Uploaded/Published filters: those tables
//!   (`destinations`, `uploads`, `publications`) belong to cm-storage/cm-publish.
//! * Rebuilding transcript rows from `transcript.json` (its schema is unspecified).
//! * File watching; callers call [`Library::rescan`].
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod bundle;
pub mod history;
pub mod index;
pub mod manifest;
pub mod ops;
pub mod rec;
pub mod rec_edit;
pub mod thumbs;
mod util;

pub use bundle::ShotBundle;
pub use index::Library;
pub use rec::{
    exports_of, publishes_of, read_manifest, record_export, record_publish,
    record_remote_deletions, record_upload, remote_deletions_of, uploads_of, RecBundle,
    RecoveryReport,
};

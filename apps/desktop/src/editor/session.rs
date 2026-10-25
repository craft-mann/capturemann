//! Talking to the embedded daemon for one open capture. Every state change goes through
//! `annotate`, `undo` or `redo` as `Actor::User` (one call = one undo step, ANN-3); the
//! editor never writes a bundle. The daemon is the source of truth: after each job the
//! op list, ids and history flags are read back with `get_capture`.
//!
//! `annotate` appends or replaces the whole list; there is no per-op update action, so
//! move, resize, recolor and delete are `replace: true` calls carrying the full list. They
//! are still one revision each, which is what ANN-3 asks for.

use cm_core::{parse_ops, AnnotationOp};
use cm_daemon::Daemon;
use cm_i18n::tr;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::state::{Edit, EditorState};
use crate::backend::Backend;
use crate::daemon_text::{AppNote, ErrorNote};

/// Everything the editor needs to know about a capture, as the daemon reports it.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub title: String,
    /// The bundle folder.
    pub dir: PathBuf,
    pub ops: Vec<AnnotationOp>,
    pub revision: u64,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Daemon warnings, e.g. a broken manifest that was set aside (ANN-3 edge case).
    pub warnings: Vec<String>,
}

pub async fn fetch_snapshot(daemon: &Daemon, id: &str) -> Result<Snapshot, ErrorNote> {
    let v = Backend::call(
        daemon,
        "get_capture",
        json!({ "capture_id": id, "include_image": false }),
    )
    .await
    .map_err(ErrorNote::plain)?;
    parse_snapshot(&v).map_err(ErrorNote::plain)
}

/// Asks the daemon for Auto-redact proposals (`suggest_redactions`, read-only, ANN-2.AC4). The
/// recognizer runs inside the daemon call's blocking task, never on the UI thread.
pub async fn fetch_proposals(
    daemon: &Daemon,
    id: &str,
) -> Result<Vec<super::redact::Proposal>, ErrorNote> {
    let v = Backend::call(daemon, "suggest_redactions", super::redact::request(id))
        .await
        .map_err(ErrorNote::plain)?;
    super::redact::parse_proposals(&v).map_err(ErrorNote::plain)
}

pub fn parse_snapshot(v: &Value) -> Result<Snapshot, String> {
    let ops = parse_ops(v.get("ops").unwrap_or(&Value::Null)).map_err(|e| e.to_string())?;
    Ok(Snapshot {
        title: v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        dir: v
            .get("path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| "get_capture returned no path".to_owned())?,
        ops,
        revision: v.get("revision").and_then(Value::as_u64).unwrap_or(0),
        can_undo: v.get("can_undo").and_then(Value::as_bool).unwrap_or(false),
        can_redo: v.get("can_redo").and_then(Value::as_bool).unwrap_or(false),
        warnings: v
            .get("warnings")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|w| w.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// Reads `original.png` (read-only; the preview renders annotations over it locally with the
/// same `cm-render` the daemon uses).
pub async fn load_original(dir: &Path) -> Result<image::RgbaImage, ErrorNote> {
    let path = dir.join("original.png");
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| {
            ErrorNote::app_with(AppNote::ReadOriginal(e.to_string()), e.to_string())
        })?;
        image::load_from_memory(&bytes)
            .map(|i| i.to_rgba8())
            .map_err(|e| ErrorNote::app_with(AppNote::DecodeOriginal(e.to_string()), e.to_string()))
    })
    .await
    .map_err(|e| ErrorNote::plain(e.to_string()))?
}

/// A queued daemon call.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    Edit(Edit),
    Undo,
    Redo,
}

/// Outcome of one job.
#[derive(Debug, Clone)]
pub struct JobResult {
    pub snapshot: Snapshot,
    /// Temporary editor ids resolved to daemon ids so far (cumulative).
    pub remap: HashMap<String, String>,
}

fn wire_ops(ops: &[AnnotationOp], remap: &HashMap<String, String>) -> Value {
    let list: Vec<AnnotationOp> = ops
        .iter()
        .map(|o| {
            let mut o = o.clone();
            if let Some(real) = remap.get(&o.id) {
                o.id = real.clone();
            } else if EditorState::is_tmp_id(&o.id) {
                // Unresolved temporary id: let the daemon mint one.
                o.id.clear();
            }
            o
        })
        .collect();
    serde_json::to_value(list).unwrap_or(Value::Null)
}

/// Runs one job and reads the capture back. `remap` carries ids resolved by earlier jobs.
pub async fn run_job(
    daemon: &Daemon,
    id: &str,
    job: &Job,
    mut remap: HashMap<String, String>,
) -> Result<JobResult, String> {
    match job {
        Job::Edit(Edit::Append(ops)) => {
            let cleared: Vec<AnnotationOp> = ops
                .iter()
                .map(|o| {
                    let mut o = o.clone();
                    o.id.clear();
                    o
                })
                .collect();
            Backend::call(
                daemon,
                "annotate",
                json!({ "capture_id": id,
                        "ops": serde_json::to_value(&cleared).map_err(|e| e.to_string())?,
                        "replace": false, "return_image": false }),
            )
            .await?;
            let snapshot = fetch_snapshot(daemon, id).await.map_err(|e| e.text())?;
            // Appended ops are the last N of the list; pair them with our temp ids.
            let n = ops.len();
            let start = snapshot.ops.len().saturating_sub(n);
            for (tmp, real) in ops.iter().zip(snapshot.ops[start..].iter()) {
                remap.insert(tmp.id.clone(), real.id.clone());
            }
            Ok(JobResult { snapshot, remap })
        }
        Job::Edit(Edit::Replace(ops)) => {
            Backend::call(
                daemon,
                "annotate",
                json!({ "capture_id": id, "ops": wire_ops(ops, &remap), "replace": true,
                        "return_image": false }),
            )
            .await?;
            Ok(JobResult {
                snapshot: fetch_snapshot(daemon, id).await.map_err(|e| e.text())?,
                remap,
            })
        }
        Job::Undo | Job::Redo => {
            let method = if *job == Job::Undo { "undo" } else { "redo" };
            Backend::call(daemon, method, json!({ "capture_id": id })).await?;
            Ok(JobResult {
                snapshot: fetch_snapshot(daemon, id).await.map_err(|e| e.text())?,
                remap,
            })
        }
    }
}

/// A file name for "Save as": the capture title made safe for both OSes, plus `.png`.
pub fn suggested_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    let stem = if cleaned.is_empty() {
        "CaptureMann"
    } else {
        cleaned
    };
    let stem = stem.strip_suffix(".png").unwrap_or(stem);
    format!("{stem}.png")
}

/// Writes the rendered PNG the daemon returned. It writes to a temporary sibling first so a
/// failed write never leaves a truncated file.
pub fn write_png(path: &Path, png: &[u8]) -> Result<(), String> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, png)
        .map_err(|e| tr!("cannot write {path}: {e}", path = path.display(), e = e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        tr!("cannot write {path}: {e}", path = path.display(), e = e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Backend, StartConfig};
    use crate::settings::FileSettings;
    use cm_capture::fake::FakeBackend;
    use cm_capture::Capturer;
    use cm_core::{Color, OpKind, Pt, Rc};
    use cm_daemon::testing::RecordingClipboard;
    use cm_daemon::DenyAll;
    use std::sync::Arc;

    struct Fx {
        rt: tokio::runtime::Runtime,
        backend: Backend,
        _dir: tempfile::TempDir,
    }

    fn fx() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let backend = Backend::start(
            &rt,
            StartConfig {
                library_path: Some(dir.path().join("lib")),
                fallback_path: dir.path().join("fb"),
                settings: Arc::new(FileSettings::load(dir.path().join("s.json"))),
                endpoint: None,
                overlay_binary: None,
                recording: crate::backend::RecordingMode::Off,
            },
            Capturer::new(FakeBackend::new().with_display(1, 0, 0, 120, 80, 1.0))
                .with_own_pid(None),
            Arc::new(Arc::new(RecordingClipboard::default())),
            DenyAll,
        );
        Fx {
            rt,
            backend,
            _dir: dir,
        }
    }

    async fn capture(d: &Daemon) -> String {
        let v = Backend::call(
            d,
            "capture_screenshot",
            json!({"target": {"display_id": 1}}),
        )
        .await
        .unwrap();
        v["capture_id"].as_str().unwrap().to_owned()
    }

    fn rect(id: &str, x: f64) -> AnnotationOp {
        AnnotationOp::new(
            id,
            OpKind::Rect {
                rect: Rc::new(x, 10.0, 30.0, 20.0),
                color: Color::rgb(0xFF, 0x3B, 0x3B),
                width: 4.0,
                fill: None,
                radius: 0.0,
            },
        )
    }

    async fn rendered(d: &Daemon, id: &str) -> image::RgbaImage {
        let (_, png) = Backend::capture_png(d, id).await.unwrap();
        image::load_from_memory(&png).unwrap().to_rgba8()
    }

    /// ANN-2.AC4 through the editor's session: the proposals come back parsed and the capture is
    /// untouched; accepting them is one append (one undo step) and undo removes them all.
    #[test]
    fn ann_2_ac4_proposals_then_accept_all_is_one_undo_step() {
        use cm_daemon::testing::{ScriptedRecognizer, TextLine};
        let dir = tempfile::tempdir().unwrap();
        let lib = cm_library::Library::open_or_create(dir.path()).unwrap();
        let rec = ScriptedRecognizer::new(vec![
            TextLine::laid_out("Owner: jane.doe@example.com", 5.0, 5.0, 4.0, 10.0),
            TextLine::laid_out("Billing: ap@vendor.example.org", 5.0, 40.0, 4.0, 10.0),
        ]);
        let d = Daemon::builder(
            Capturer::new(FakeBackend::new().with_display(1, 0, 0, 160, 80, 1.0))
                .with_own_pid(None),
            lib,
        )
        .text_recognizer(rec)
        .build();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let id = capture(&d).await;
            let proposals = fetch_proposals(&d, &id).await.unwrap();
            assert_eq!(proposals.len(), 2, "{proposals:?}");
            assert!(proposals.iter().all(|p| p.kind == "email"));
            assert!(fetch_snapshot(&d, &id).await.unwrap().ops.is_empty());

            let mut st = EditorState::new((160, 80), Vec::new(), Default::default());
            st.set_proposals(proposals);
            let edit = st.accept_proposals(None).unwrap();
            let r = run_job(&d, &id, &Job::Edit(edit), HashMap::new())
                .await
                .unwrap();
            assert_eq!(r.snapshot.ops.len(), 2);
            assert!(r
                .snapshot
                .ops
                .iter()
                .all(|o| matches!(o.kind, OpKind::Blur { .. })));
            let u = run_job(&d, &id, &Job::Undo, r.remap).await.unwrap();
            assert!(u.snapshot.ops.is_empty(), "one undo removes both blurs");
        });
    }

    /// ANN-2 edge: no recognizer means a clear `not_implemented` message.
    #[test]
    fn no_recognizer_gives_a_clear_error() {
        use cm_daemon::testing::ScriptedRecognizer;
        let dir = tempfile::tempdir().unwrap();
        let lib = cm_library::Library::open_or_create(dir.path()).unwrap();
        let d = Daemon::builder(
            Capturer::new(FakeBackend::new().with_display(1, 0, 0, 160, 80, 1.0))
                .with_own_pid(None),
            lib,
        )
        .text_recognizer(ScriptedRecognizer::unavailable())
        .build();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let id = capture(&d).await;
            let e = fetch_proposals(&d, &id).await.unwrap_err();
            assert!(e.message().contains("text recognition"), "{}", e.message());
        });
    }

    /// ANN-3 + AGT-1: an append gets daemon ids, mapped back to the editor's temporary ones;
    /// a later replace that still uses the temporary id lands on the right op.
    #[test]
    fn append_resolves_ids_and_replace_uses_them() {
        let f = fx();
        let d = f.backend.daemon.clone().unwrap();
        f.rt.block_on(async {
            let id = capture(&d).await;
            let r = run_job(
                &d,
                &id,
                &Job::Edit(Edit::Append(vec![rect("~1", 5.0)])),
                HashMap::new(),
            )
            .await
            .unwrap();
            assert_eq!(r.snapshot.ops.len(), 1);
            let real = r.remap["~1"].clone();
            assert!(real.starts_with("op_"), "{real}");
            assert_eq!(r.snapshot.ops[0].id, real);
            assert!(r.snapshot.can_undo && !r.snapshot.can_redo);

            // Replace still carrying the temp id: the remap fixes it.
            let moved = rect("~1", 50.0);
            let r2 = run_job(
                &d,
                &id,
                &Job::Edit(Edit::Replace(vec![moved])),
                r.remap.clone(),
            )
            .await
            .unwrap();
            assert_eq!(r2.snapshot.ops.len(), 1);
            assert_eq!(r2.snapshot.ops[0].id, real);
            assert!(
                matches!(&r2.snapshot.ops[0].kind, OpKind::Rect { rect, .. } if rect.x == 50.0)
            );
        });
    }

    /// ANN-3.AC1: after 5 edits, 5 undos give back the original pixel for pixel; redo
    /// re-applies. ANN-1.AC5: a recolor is exactly one undo step.
    #[test]
    fn five_edits_five_undos_restore_the_original() {
        let f = fx();
        let d = f.backend.daemon.clone().unwrap();
        f.rt.block_on(async {
            let id = capture(&d).await;
            let before = rendered(&d, &id).await;
            let mut remap = HashMap::new();
            let mut ops: Vec<AnnotationOp> = Vec::new();
            for i in 0..3 {
                let op = rect(&format!("~{i}"), 5.0 + 20.0 * f64::from(i));
                ops.push(op.clone());
                let r = run_job(&d, &id, &Job::Edit(Edit::Append(vec![op])), remap)
                    .await
                    .unwrap();
                remap = r.remap;
            }
            // Recolor op 0 (one replace) and move op 2 (one replace).
            if let OpKind::Rect { color, .. } = &mut ops[0].kind {
                *color = Color::rgb(0x2E, 0x8B, 0xFF);
            }
            run_job(
                &d,
                &id,
                &Job::Edit(Edit::Replace(ops.clone())),
                remap.clone(),
            )
            .await
            .unwrap();
            if let OpKind::Rect { rect, .. } = &mut ops[2].kind {
                rect.y += 5.0;
            }
            let last = run_job(&d, &id, &Job::Edit(Edit::Replace(ops)), remap)
                .await
                .unwrap();
            assert_eq!(last.snapshot.ops.len(), 3);
            assert_ne!(rendered(&d, &id).await, before);
            for k in 0..5 {
                let r = run_job(&d, &id, &Job::Undo, HashMap::new()).await.unwrap();
                assert_eq!(r.snapshot.can_undo, k < 4);
            }
            let after = rendered(&d, &id).await;
            assert_eq!(after, before, "ANN-3.AC1: pixel for pixel");
            let r = run_job(&d, &id, &Job::Redo, HashMap::new()).await.unwrap();
            assert_eq!(r.snapshot.ops.len(), 1);
            assert!(r.snapshot.can_redo);
        });
    }

    /// ANN-3: "Revert to original" clears all ops in ONE undoable step, and undo brings them
    /// all back; `original.png` is untouched (ANN-3.AC4).
    #[test]
    fn revert_to_original_is_one_undoable_step() {
        let f = fx();
        let d = f.backend.daemon.clone().unwrap();
        f.rt.block_on(async {
            let id = capture(&d).await;
            let dir = Backend::capture_dir(&d, &id).await.unwrap();
            let hash = std::fs::read(dir.join("original.png")).unwrap();
            let both = vec![rect("~1", 5.0), rect("~2", 60.0)];
            run_job(&d, &id, &Job::Edit(Edit::Replace(both)), HashMap::new())
                .await
                .unwrap();
            let r = run_job(
                &d,
                &id,
                &Job::Edit(Edit::Replace(Vec::new())),
                HashMap::new(),
            )
            .await
            .unwrap();
            assert!(r.snapshot.ops.is_empty());
            let r = run_job(&d, &id, &Job::Undo, HashMap::new()).await.unwrap();
            assert_eq!(r.snapshot.ops.len(), 2, "undo restores both ops at once");
            assert_eq!(std::fs::read(dir.join("original.png")).unwrap(), hash);
            // Nothing to clear is reported, not silently accepted.
            run_job(&d, &id, &Job::Undo, HashMap::new()).await.unwrap();
            let err = run_job(
                &d,
                &id,
                &Job::Edit(Edit::Replace(Vec::new())),
                HashMap::new(),
            )
            .await;
            assert!(err.is_err());
        });
    }

    /// ANN-3.AC3 (reopen): a fresh `fetch_snapshot` carries history flags, so undo works on
    /// reopening the capture.
    #[test]
    fn snapshot_carries_history_after_reopen() {
        let f = fx();
        let d = f.backend.daemon.clone().unwrap();
        f.rt.block_on(async {
            let id = capture(&d).await;
            let fresh = fetch_snapshot(&d, &id).await.unwrap();
            assert!(!fresh.can_undo && fresh.ops.is_empty());
            run_job(
                &d,
                &id,
                &Job::Edit(Edit::Append(vec![rect("~1", 1.0)])),
                HashMap::new(),
            )
            .await
            .unwrap();
            let reopened = fetch_snapshot(&d, &id).await.unwrap();
            assert!(reopened.can_undo && reopened.ops.len() == 1);
            assert!(reopened.dir.join("original.png").is_file());
            let img = load_original(&reopened.dir).await.unwrap();
            assert_eq!(img.dimensions(), (120, 80));
        });
    }

    /// ANN-1.AC4 + preview parity: the local preview render of the daemon's ops equals what
    /// the daemon stores as `render.png` (same deterministic renderer).
    #[test]
    fn local_preview_matches_the_daemon_render() {
        let f = fx();
        let d = f.backend.daemon.clone().unwrap();
        f.rt.block_on(async {
            let id = capture(&d).await;
            let blur = AnnotationOp::new(
                "~b",
                OpKind::Blur {
                    rect: Rc::new(10.0, 10.0, 40.0, 30.0),
                    strength: 12,
                },
            );
            let text = AnnotationOp::new(
                "~t",
                OpKind::Text {
                    at: Pt::new(5.0, 50.0),
                    text: "Hi".into(),
                    size: 24.0,
                    color: Color::WHITE,
                    outline: Some(Color::BLACK),
                    bg: None,
                },
            );
            let r = run_job(
                &d,
                &id,
                &Job::Edit(Edit::Append(vec![rect("~r", 60.0), blur, text])),
                HashMap::new(),
            )
            .await
            .unwrap();
            let base = load_original(&r.snapshot.dir).await.unwrap();
            let local = cm_render::render_annotations(&base, &r.snapshot.ops).unwrap();
            assert_eq!(local, rendered(&d, &id).await);
        });
    }

    #[test]
    fn file_names_are_safe_on_both_platforms() {
        assert_eq!(
            suggested_file_name("Screenshot 2026-10-20 at 10.15.03"),
            "Screenshot 2026-10-20 at 10.15.03.png"
        );
        assert_eq!(suggested_file_name("a/b:c*d?.png"), "a-b-c-d-.png");
        assert_eq!(suggested_file_name("  .. "), "CaptureMann.png");
    }

    #[test]
    fn write_png_replaces_atomically_and_reports_errors() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.png");
        write_png(&p, b"abc").unwrap();
        write_png(&p, b"defg").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"defg");
        assert!(!dir.path().join("x.png.part").exists());
        assert!(write_png(&dir.path().join("no/such/dir.png"), b"x").is_err());
    }
}

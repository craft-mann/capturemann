use super::*;

impl Inner {
    /// The caption lines of a recording: the edited lines when `set_captions` stored some,
    /// otherwise lines built from the transcript. `not_found` when there is neither.
    pub(super) async fn get_captions(&self, capture_id: &str) -> Result<Value, DaemonError> {
        let (library, id, captions) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.captions.clone(),
        );
        blocking(move || {
            let (dir, manifest) = recording_bundle(&library, &id)?;
            if let Some(stored) = cm_library::rec_edit::captions_of(&manifest).cloned() {
                return Ok(json!({"capture_id": id, "lines": stored, "source": "edited"}));
            }
            let path = dir.join(TRANSCRIPT_FILE);
            let text = std::fs::read_to_string(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    CmError::not_found(format!(
                        "`{id}` has no captions yet; run `transcribe` first"
                    ))
                } else {
                    CmError::internal(format!("cannot read `{}`: {e}", path.display()))
                }
            })?;
            let transcript: Transcript = serde_json::from_str(&text).map_err(|e| {
                CmError::internal(format!("`{}` is not a transcript: {e}", path.display()))
            })?;
            let lines = captions.lines(&transcript)?;
            Ok(json!({"capture_id": id, "lines": lines, "source": "generated"}))
        })
        .await
    }

    pub(super) async fn set_captions(
        &self,
        actor: &Actor,
        capture_id: String,
        lines: Vec<CaptionLine>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let (id, count) = (capture_id.clone(), lines.len());
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if rec.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; captions belong to recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "caption")?;
            let json_lines = serde_json::to_value(&lines)
                .map_err(|e| CmError::internal(format!("captions: {e}")))?;
            let out = cm_library::rec_edit::apply_captions(&dir, json_lines, by, now)?;
            lib.index_dir(&dir)?;
            Ok(out)
        })
        .await?;
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({
                "capture_id": capture_id,
                "revision": out.revision.rev,
                "action": out.revision.action,
                "by": out.revision.by,
            }),
        );
        Ok(json!({
            "capture_id": capture_id,
            "revision": out.revision.rev,
            "action": out.revision.action,
            "by": out.revision.by,
            "lines": count,
            "can_undo": out.can_undo,
            "can_redo": out.can_redo,
        }))
    }

    /// `replace_in_captions`: find and replace over every caption line (the edited lines, else
    /// the lines built from the transcript), matched like the editor's find (NFC, Vietnamese
    /// exact), the pairs applied in order. One revision; nothing is written when nothing matched.
    /// `transcript.json` is not touched: the lines are the corrected text layer.
    pub(super) async fn replace_in_captions(
        &self,
        actor: &Actor,
        capture_id: String,
        pairs: Vec<Replacement>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, now, by, captions) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
            self.ai.captions.clone(),
        );
        let id = capture_id.clone();
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if rec.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; captions belong to recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "caption")?;
            let manifest = cm_library::read_manifest(&dir)?;
            let mut lines: Vec<CaptionLine> = match cm_library::rec_edit::captions_of(&manifest) {
                Some(v) => serde_json::from_value(v.clone())
                    .map_err(|e| CmError::internal(format!("stored captions: {e}")))?,
                None => {
                    let text = std::fs::read_to_string(dir.join(TRANSCRIPT_FILE)).map_err(|_| {
                        CmError::not_found(format!(
                            "`{id}` has no captions yet; run `transcribe` first"
                        ))
                    })?;
                    let t: Transcript = serde_json::from_str(&text)
                        .map_err(|e| CmError::internal(format!("transcript: {e}")))?;
                    captions.lines(&t)?
                }
            };
            let (mut total, mut touched) = (0usize, vec![false; lines.len()]);
            for p in &pairs {
                for (i, l) in lines.iter_mut().enumerate() {
                    if let Some((text, n)) = cm_ai::textmatch::replace_in_line(
                        &l.text,
                        &p.find,
                        &p.replace,
                        p.match_case,
                        p.whole_word,
                    ) {
                        l.text = text;
                        total += n;
                        touched[i] = true;
                    }
                }
            }
            let lines_touched = touched.iter().filter(|t| **t).count();
            if total == 0 {
                return Ok(json!({
                    "capture_id": id, "replacements": 0, "lines_touched": 0,
                    "revision": null, "changed": false,
                }));
            }
            // The same limits as `set_captions`: a replacement may not make a line too long.
            if let Some((i, _)) = lines
                .iter()
                .enumerate()
                .find(|(i, l)| touched[*i] && !cm_ai::textmatch::rows_ok(&l.text))
            {
                return Err(CmError::invalid_input(format!(
                    "the replacement would make caption line {} longer than {MAX_CAPTION_ROWS} rows of {MAX_CAPTION_ROW_CHARS} characters; nothing was changed",
                    i + 1
                )));
            }
            let json_lines = serde_json::to_value(&lines)
                .map_err(|e| CmError::internal(format!("captions: {e}")))?;
            let out = cm_library::rec_edit::apply_captions(&dir, json_lines, by, now)?;
            lib.index_dir(&dir)?;
            Ok(json!({
                "capture_id": id, "replacements": total, "lines_touched": lines_touched,
                "revision": out.revision.rev, "action": out.revision.action,
                "by": out.revision.by, "changed": true,
                "can_undo": out.can_undo, "can_redo": out.can_redo,
            }))
        })
        .await?;
        if out["changed"] == json!(true) {
            self.events.publish(
                events::CAPTURE_REVISED,
                json!({ "capture_id": capture_id, "revision": out["revision"],
                        "action": out["action"], "by": out["by"] }),
            );
        }
        Ok(out)
    }

    pub(crate) async fn export_captions(
        &self,
        capture_id: &str,
        format: CaptionFormat,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(capture_id).await;
        let (library, id, captions) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.captions.clone(),
        );
        blocking(move || {
            let (dir, manifest) = recording_bundle(&library, &id)?;
            let title = lock_library(&library).get(&id)?.title;
            let lines: Vec<CaptionLine> = match cm_library::rec_edit::captions_of(&manifest) {
                Some(l) => serde_json::from_value(l.clone())
                    .map_err(|e| CmError::internal(format!("stored captions: {e}")))?,
                None => {
                    let text =
                        std::fs::read_to_string(dir.join(TRANSCRIPT_FILE)).map_err(|_| {
                            CmError::not_found(format!(
                            "`{id}` has no captions yet; run `transcribe` or `set_captions` first"
                        ))
                        })?;
                    let t: Transcript = serde_json::from_str(&text)
                        .map_err(|e| CmError::internal(format!("transcript: {e}")))?;
                    captions.lines(&t)?
                }
            };
            let edits = cm_library::rec_edit::load_edits(&manifest);
            let text = captions.render(&lines, format, &edits)?;
            let out_dir = dir.join("exports");
            std::fs::create_dir_all(&out_dir).map_err(|e| {
                CmError::internal(format!("cannot create `{}`: {e}", out_dir.display()))
            })?;
            let safe_title = title.replace(['/', '\\', ':'], "-");
            // The plain-text transcript is a document in its own right: `<title>.txt`; the
            // subtitle files say what they are.
            let name = if format.is_text() {
                format!("{safe_title}.txt")
            } else {
                format!("{safe_title} - captions.{}", format.ext())
            };
            let path = write_atomic(&out_dir, &name, text.as_bytes())?;
            lock_library(&library).index_dir(&dir)?;
            Ok(json!({
                "capture_id": id,
                "format": format.ext(),
                "path": path,
                "lines": lines.len(),
            }))
        })
        .await
    }
}

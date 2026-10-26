use super::*;

impl Inner {
    pub(super) async fn suggest(
        &self,
        capture_id: &str,
        zooms: bool,
    ) -> Result<Value, DaemonError> {
        let (library, id, svc) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.suggest.clone(),
        );
        blocking(move || {
            let (dir, _) = recording_bundle(&library, &id)?;
            let to_s = |t: cm_core::MediaTime| secs(t);
            let suggestions: Vec<Value> = if zooms {
                svc.zooms(&dir)?
                    .iter()
                    .map(|z| {
                        json!({
                            "start": z.start, "end": z.end,
                            "start_s": to_s(z.start), "end_s": to_s(z.end),
                            "focus": [z.focus.0, z.focus.1], "scale": z.scale,
                            "reason": z.reason,
                        })
                    })
                    .collect()
            } else {
                let transcript = std::fs::read_to_string(dir.join(TRANSCRIPT_FILE))
                    .ok()
                    .and_then(|t| serde_json::from_str::<Transcript>(&t).ok());
                svc.cuts(&dir, transcript.as_ref())?
                    .iter()
                    .map(|c| {
                        json!({
                            "start": c.start, "end": c.end,
                            "start_s": to_s(c.start), "end_s": to_s(c.end),
                            "kind": c.kind, "detail": c.detail,
                        })
                    })
                    .collect()
            };
            Ok(json!({
                "capture_id": id,
                "count": suggestions.len(),
                "suggestions": suggestions,
                "apply_with": if zooms { "set_zooms" } else { "cut_segments" },
            }))
        })
        .await
    }
}

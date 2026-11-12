//! The `import` job for one picture (IMP-6): decode, orient, convert, store as a `.cmshot`.
//! No FFmpeg is involved. The original file is trashed (with `remove_originals`) only after the
//! bundle was written, checked and installed, as for video (IMP-4).

use std::path::PathBuf;

use cm_core::CmError;
use cm_library::import::{
    build_picture_bundle, install_as, new_staging_dir, remove_staging, ImportSource,
};
use cm_library::manifest::CaptureKind;
use cm_media::import::picture::{self, LoadError};
use serde_json::{json, Value};
use time::OffsetDateTime;

use super::{events, file_name, lock_library, FileOutcome, ImportRun, Progress, Stop};

impl ImportRun {
    /// Imports one picture. Blocking.
    pub(super) fn one_picture(
        &self,
        index: usize,
        f: &Value,
        cancel: &dyn Fn() -> bool,
        prog: &mut Progress<'_>,
    ) -> Result<FileOutcome, Stop> {
        let path_s = f["path"].as_str().unwrap_or("").to_owned();
        let path = PathBuf::from(&path_s);
        let capture_id = f["capture_id"].as_str().unwrap_or("").to_owned();
        let title = f["title"].as_str().unwrap_or("Imported").to_owned();
        let fail = |e: CmError| Stop::Failed(FileOutcome::failed(&path_s, e, None, None));

        prog.set(0.0, "Reading the picture");
        let loaded = match picture::load(&path, cancel) {
            Ok(Some(l)) => l,
            Ok(None) => return Err(Stop::Interrupted),
            Err(LoadError::Problem(p)) => {
                return Err(Stop::Failed(FileOutcome::problem(&path_s, &p)))
            }
            Err(LoadError::Other(e)) => return Err(fail(e)),
        };
        prog.set(0.6, "Saving into the library");
        let stage = new_staging_dir(&self.root, &self.job_id, index).map_err(fail)?;
        let now = OffsetDateTime::now_utc();
        let source = ImportSource {
            path: path_s.clone(),
            imported_at: now,
            original_name: file_name(&path),
            original_file: "original.png".to_owned(),
        };
        let built = build_picture_bundle(&stage, &capture_id, &title, &loaded.png, &source);
        let (dir, thumb_warning) = match built {
            Ok(b) => b,
            Err(e) => {
                remove_staging(&stage);
                return Err(fail(e));
            }
        };
        drop(loaded);
        if let Some(w) = thumb_warning {
            log::warn!("imported picture `{path_s}`: thumbnail not created: {w}");
        }
        // Verify: the stored original is the one that was written.
        prog.set(0.9, "Checking the copy");
        let verified = cm_library::ShotBundle::open(&dir).and_then(|b| {
            if b.verify_original()? {
                Ok(())
            } else {
                Err(CmError::internal("the stored picture does not match"))
            }
        });
        if let Err(e) = verified {
            remove_staging(&stage);
            return Err(fail(e));
        }
        if cancel() {
            remove_staging(&stage);
            return Err(Stop::Interrupted);
        }
        let record = {
            let mut lib = lock_library(&self.library);
            install_as(&mut lib, &dir, &title, CaptureKind::Screenshot)
        };
        remove_staging(&stage);
        let record = match record {
            Ok(r) => r,
            Err(e) => return Err(fail(e)),
        };
        self.events.publish(
            events::CAPTURE_CREATED,
            json!({ "capture_id": capture_id, "title": record.title, "by": self.by,
                    "job_id": self.job_id, "import": true, "kind": record.kind.as_str() }),
        );
        let trash = self.remove_originals.then(|| self.trash(&path));
        prog.set(1.0, "Done");
        Ok(FileOutcome {
            path: path_s,
            capture_id: Some(capture_id),
            status: "imported",
            error: None,
            trash,
            editing_copy: None,
            code: None,
        })
    }
}

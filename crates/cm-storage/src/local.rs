//! Local folder, external drive and NAS provider (STO-1). Copies to a temp name in the final
//! folder, verifies size and SHA-256, then renames; never overwrites (adds ` (2)`).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cm_core::CmError;
use sha2::{Digest, Sha256};

use crate::{io_error, FolderClaim, Progress, ResumeState, StorageProvider, Target, Uploaded};

const CHUNK: usize = 1 << 20;

#[derive(Clone)]
pub struct LocalProvider {
    root: PathBuf,
}

impl LocalProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LocalProvider { root: root.into() }
    }

    /// The destination root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// True while the destination root is reachable (STO-1.AC2 polls this).
    pub fn is_online(&self) -> bool {
        self.root.is_dir()
    }

    /// Claims an unused folder `name` (or `name (2)`, ...) below `folder` by creating it, so a
    /// bundle copied there never mixes its files with an earlier copy. Returns the name taken.
    pub fn claim_folder(&self, folder: &str, name: &str) -> Result<String, CmError> {
        if !self.is_online() {
            return Err(CmError::Network {
                message: format!("Destination offline: {}", self.root.display()),
            });
        }
        let parent = self.folder_path(folder);
        fs::create_dir_all(&parent).map_err(|e| io_error("Cannot create", &parent, &e))?;
        for candidate in crate::pattern::numbered_names(name) {
            match fs::create_dir(parent.join(&candidate)) {
                Ok(()) => return Ok(candidate),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error("Cannot create", &parent.join(&candidate), &e)),
            }
        }
        Err(CmError::Internal {
            message: format!("No free folder name for {name}"),
        })
    }

    /// The canonical path of `path`, which must lie below the destination root (symbolic links
    /// are resolved first, so a link cannot lead out of the root).
    fn inside_root(&self, path: &Path) -> Result<PathBuf, CmError> {
        let root =
            fs::canonicalize(&self.root).map_err(|e| io_error("Cannot read", &self.root, &e))?;
        let real = fs::canonicalize(path).map_err(|e| io_error("Cannot read", path, &e))?;
        if real == root || !real.starts_with(&root) {
            return Err(CmError::InvalidInput {
                message: format!(
                    "{} is not inside the destination folder {}; nothing was deleted",
                    path.display(),
                    self.root.display()
                ),
            });
        }
        Ok(real)
    }

    /// Deletes one copied file by the path the upload recorded. Refused (`invalid_input`) for a
    /// relative path, a `..` segment, anything outside the destination root, or anything that is
    /// not a regular file; `not_found` when it is gone; `network` while the root is offline.
    pub fn delete_file(&self, remote_id: &str) -> Result<(), CmError> {
        if !self.is_online() {
            return Err(CmError::Network {
                message: format!("Destination offline: {}", self.root.display()),
            });
        }
        let path = PathBuf::from(remote_id);
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(CmError::InvalidInput {
                message: format!(
                    "`{remote_id}` is not a path this destination wrote; nothing was deleted"
                ),
            });
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| io_error("Cannot read", &path, &e))?;
        if !meta.is_file() {
            return Err(CmError::InvalidInput {
                message: format!("{} is not a file; nothing was deleted", path.display()),
            });
        }
        let real = self.inside_root(&path)?;
        fs::remove_file(&real).map_err(|e| io_error("Cannot delete", &real, &e))
    }

    /// Deletes the files of a bundle copy (`files` are paths relative to `folder`, as the bundle
    /// has them) and then the folders that are empty, bottom up. Files that are not in `files`
    /// (the user's own) stay, and so do the folders that hold them. Returns how many files were
    /// deleted; `not_found` when the folder is gone.
    pub fn delete_bundle_copy(&self, folder: &str, files: &[String]) -> Result<usize, CmError> {
        if !self.is_online() {
            return Err(CmError::Network {
                message: format!("Destination offline: {}", self.root.display()),
            });
        }
        let dir = self.inside_root(&self.folder_path(folder))?;
        let mut removed = 0usize;
        let mut dirs = std::collections::BTreeSet::new();
        for rel in files {
            if rel.starts_with('/') || rel.split('/').any(|p| p.is_empty() || p == "..") {
                return Err(CmError::InvalidInput {
                    message: format!(
                        "`{rel}` is not a path inside a bundle; nothing more was deleted"
                    ),
                });
            }
            let path = dir.join(rel);
            if let Ok(meta) = fs::symlink_metadata(&path) {
                if meta.is_file() {
                    fs::remove_file(&path).map_err(|e| io_error("Cannot delete", &path, &e))?;
                    removed += 1;
                }
            }
            let mut p = path.parent();
            while let Some(d) = p {
                if d == dir || !d.starts_with(&dir) {
                    break;
                }
                dirs.insert(d.to_path_buf());
                p = d.parent();
            }
        }
        // Deepest first; a folder that still holds a foreign file is left alone.
        let mut dirs: Vec<PathBuf> = dirs.into_iter().collect();
        dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
        for d in dirs {
            let _ = fs::remove_dir(&d);
        }
        let _ = fs::remove_dir(&dir);
        Ok(removed)
    }

    /// STO-5: puts a new version of `file` over the copy an earlier upload made, inside the
    /// bundle folder `target.folder` only. The new content is written to a temp name in that
    /// folder, checked, and renamed over the old file (the old file is whole until the rename).
    /// `existing` is the path the first upload returned: it must be a regular file directly in
    /// that folder (a `..` segment, a symbolic link, another folder or anything outside the
    /// destination root is `invalid_input`). Without it the file is `folder/name`, created when
    /// it is not there yet.
    pub fn replace_file(
        &self,
        file: &Path,
        target: &Target,
        existing: Option<&str>,
        progress: &Progress,
    ) -> Result<Uploaded, CmError> {
        if !self.is_online() {
            return Err(CmError::Network {
                message: format!("Destination offline: {}", self.root.display()),
            });
        }
        let refuse = |what: &str| CmError::InvalidInput {
            message: format!("{what}; nothing was replaced"),
        };
        let wanted = self.folder_path(&target.folder);
        // Create the missing folders only after the deepest existing one is known to be inside
        // the root (a link in the middle cannot lead the new folders out of it).
        let mut anchor = wanted.as_path();
        while !anchor.exists() {
            anchor = anchor
                .parent()
                .ok_or_else(|| refuse("the folder has no existing parent"))?;
        }
        self.inside_root_or_root(anchor)?;
        fs::create_dir_all(&wanted).map_err(|e| io_error("Cannot create", &wanted, &e))?;
        let dir = self.inside_root(&wanted)?;

        let name = match existing {
            Some(id) => {
                let path = PathBuf::from(id);
                if !path.is_absolute()
                    || path
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(refuse(&format!(
                        "`{id}` is not a path this destination wrote"
                    )));
                }
                let name = path
                    .file_name()
                    .ok_or_else(|| refuse(&format!("`{id}` has no file name")))?
                    .to_string_lossy()
                    .into_owned();
                let parent = path.parent().ok_or_else(|| refuse("no folder"))?;
                let real_parent =
                    fs::canonicalize(parent).map_err(|e| io_error("Cannot read", parent, &e))?;
                if real_parent != dir {
                    return Err(refuse(&format!(
                        "`{id}` is not in the folder of this bundle copy"
                    )));
                }
                name
            }
            None => crate::pattern::sanitize_segment(&target.file_name),
        };
        let final_path = dir.join(&name);
        if let Ok(meta) = fs::symlink_metadata(&final_path) {
            if !meta.is_file() {
                return Err(refuse(&format!(
                    "{} is a link or a folder, not a file",
                    final_path.display()
                )));
            }
        }
        let temp = dir.join(format!(".{name}.part"));
        let (bytes, _) = match self.copy_verified(file, &temp, progress) {
            Ok(v) => v,
            Err(e) => {
                let _ = fs::remove_file(&temp);
                return Err(e);
            }
        };
        fs::rename(&temp, &final_path).map_err(|e| {
            let _ = fs::remove_file(&temp);
            io_error("Cannot finish", &final_path, &e)
        })?;
        Ok(Uploaded {
            remote_id: final_path.display().to_string(),
            share_link: None,
            bytes,
            note: None,
        })
    }

    /// Like [`inside_root`](Self::inside_root) but the root itself is allowed.
    fn inside_root_or_root(&self, path: &Path) -> Result<PathBuf, CmError> {
        let root =
            fs::canonicalize(&self.root).map_err(|e| io_error("Cannot read", &self.root, &e))?;
        let real = fs::canonicalize(path).map_err(|e| io_error("Cannot read", path, &e))?;
        if !real.starts_with(&root) {
            return Err(CmError::InvalidInput {
                message: format!(
                    "{} is not inside the destination folder {}; nothing was replaced",
                    path.display(),
                    self.root.display()
                ),
            });
        }
        Ok(real)
    }

    fn folder_path(&self, folder: &str) -> PathBuf {
        folder
            .split(['/', '\\'])
            .filter(|s| !s.is_empty() && *s != "." && *s != "..")
            .fold(self.root.clone(), |p, s| p.join(s))
    }

    fn copy(&self, file: &Path, target: &Target, progress: &Progress) -> Result<Uploaded, CmError> {
        if !self.is_online() {
            return Err(CmError::Network {
                message: format!("Destination offline: {}", self.root.display()),
            });
        }
        let dir = self.folder_path(&target.folder);
        fs::create_dir_all(&dir).map_err(|e| io_error("Cannot create", &dir, &e))?;

        let name = crate::pattern::sanitize_segment(&target.file_name);
        let temp = dir.join(format!(".{name}.part"));
        let result = self.copy_verified(file, &temp, progress);
        let (bytes, _) = match result {
            Ok(v) => v,
            Err(e) => {
                let _ = fs::remove_file(&temp);
                return Err(e);
            }
        };
        let final_path = rename_without_overwrite(&temp, &dir, &name)?;
        Ok(Uploaded {
            remote_id: final_path.display().to_string(),
            share_link: None,
            bytes,
            note: None,
        })
    }

    fn copy_verified(
        &self,
        src: &Path,
        temp: &Path,
        progress: &Progress,
    ) -> Result<(u64, String), CmError> {
        let mut input = File::open(src).map_err(|e| io_error("Cannot read", src, &e))?;
        let total = input
            .metadata()
            .map_err(|e| io_error("Cannot read", src, &e))?
            .len();
        let mut out = File::create(temp).map_err(|e| io_error("Cannot write", temp, &e))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; CHUNK];
        let mut done = 0u64;
        loop {
            if progress.is_cancelled() {
                return Err(CmError::Internal {
                    message: "Upload cancelled".into(),
                });
            }
            let n = input
                .read(&mut buf)
                .map_err(|e| io_error("Cannot read", src, &e))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n])
                .map_err(|e| io_error("Cannot write", temp, &e))?;
            done += n as u64;
            progress.report(done, total);
        }
        out.sync_all()
            .map_err(|e| io_error("Cannot write", temp, &e))?;
        drop(out);
        let sent = format!("{:x}", hasher.finalize());

        let written = hash_file(temp)?;
        if written.0 != total || written.1 != sent {
            return Err(CmError::Internal {
                message: format!("Copy verification failed for {}", temp.display()),
            });
        }
        Ok((total, sent))
    }
}

fn hash_file(path: &Path) -> Result<(u64, String), CmError> {
    let mut f = File::open(path).map_err(|e| io_error("Cannot read", path, &e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut len = 0u64;
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| io_error("Cannot read", path, &e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        len += n as u64;
    }
    Ok((len, format!("{:x}", hasher.finalize())))
}

/// Moves `temp` to `dir/name`, or `name (2)`, `name (3)`... if taken. `create_new` on a
/// placeholder makes the claim atomic, so two uploads never pick the same name.
fn rename_without_overwrite(temp: &Path, dir: &Path, name: &str) -> Result<PathBuf, CmError> {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1u32..10_000 {
        let candidate = if n == 1 {
            name.to_string()
        } else {
            format!("{stem} ({n}){ext}")
        };
        let path = dir.join(&candidate);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => {
                fs::rename(temp, &path).map_err(|e| io_error("Cannot finish", &path, &e))?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_error("Cannot write", &path, &e)),
        }
    }
    Err(CmError::Internal {
        message: format!("No free name for {name} in {}", dir.display()),
    })
}

#[async_trait]
impl StorageProvider for LocalProvider {
    fn id(&self) -> &'static str {
        "local"
    }

    async fn upload(
        &self,
        file: &Path,
        target: &Target,
        _resume: Option<ResumeState>,
        progress: Progress,
    ) -> Result<Uploaded, CmError> {
        // Blocking file I/O: keep it off the async worker threads.
        let (this, file, target) = (self.clone(), file.to_owned(), target.clone());
        tokio::task::spawn_blocking(move || this.copy(&file, &target, &progress))
            .await
            .map_err(|e| CmError::Internal {
                message: format!("copy worker failed: {e}"),
            })?
    }

    async fn claim_bundle_folder(&self, base: &str, name: &str) -> Result<FolderClaim, CmError> {
        let (this, base, name) = (self.clone(), base.to_owned(), name.to_owned());
        let taken = tokio::task::spawn_blocking(move || this.claim_folder(&base, &name))
            .await
            .map_err(|e| CmError::Internal {
                message: format!("claim worker failed: {e}"),
            })??;
        Ok(FolderClaim {
            name: taken,
            id: None,
        })
    }

    async fn replace(
        &self,
        file: &Path,
        target: &Target,
        existing_remote_id: Option<&str>,
        progress: Progress,
    ) -> Result<Uploaded, CmError> {
        let (this, file, target) = (self.clone(), file.to_owned(), target.clone());
        let existing = existing_remote_id.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            this.replace_file(&file, &target, existing.as_deref(), &progress)
        })
        .await
        .map_err(|e| CmError::Internal {
            message: format!("replace worker failed: {e}"),
        })?
    }

    async fn delete(&self, remote_id: &str) -> Result<(), CmError> {
        let (this, id) = (self.clone(), remote_id.to_owned());
        tokio::task::spawn_blocking(move || this.delete_file(&id))
            .await
            .map_err(|e| CmError::Internal {
                message: format!("delete worker failed: {e}"),
            })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn target(folder: &str, name: &str) -> Target {
        Target {
            folder: folder.into(),
            file_name: name.into(),
        }
    }

    fn source(dir: &Path, bytes: usize) -> PathBuf {
        let p = dir.join("src.mp4");
        fs::write(&p, (0..bytes).map(|i| (i % 251) as u8).collect::<Vec<_>>()).unwrap();
        p
    }

    // STO-1.AC1
    #[tokio::test]
    async fn copies_to_the_pattern_path_and_hash_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let src = source(tmp.path(), 3 * CHUNK + 17);
        let root = tmp.path().join("nas");
        fs::create_dir(&root).unwrap();
        let up = LocalProvider::new(&root)
            .upload(
                &src,
                &target("CaptureMann/2026/10/Lesson", "Lesson.mp4"),
                None,
                Progress::none(),
            )
            .await
            .unwrap();
        let dest = root.join("CaptureMann/2026/10/Lesson/Lesson.mp4");
        assert_eq!(up.remote_id, dest.display().to_string());
        assert_eq!(hash_file(&dest).unwrap(), hash_file(&src).unwrap());
        assert_eq!(up.bytes, 3 * CHUNK as u64 + 17);
        let leftovers: Vec<_> = fs::read_dir(dest.parent().unwrap()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no .part file remains");
    }

    #[tokio::test]
    async fn never_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let src = source(tmp.path(), 10);
        let root = tmp.path().join("d");
        fs::create_dir(&root).unwrap();
        let p = LocalProvider::new(&root);
        for _ in 0..3 {
            p.upload(&src, &target("", "a.mp4"), None, Progress::none())
                .await
                .unwrap();
        }
        let mut names: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["a (2).mp4", "a (3).mp4", "a.mp4"]);
    }

    // STO-1.AC2 (the waiting itself belongs to the job handler)
    #[tokio::test]
    async fn offline_root_is_a_retryable_network_error() {
        let tmp = tempfile::tempdir().unwrap();
        let src = source(tmp.path(), 10);
        let p = LocalProvider::new(tmp.path().join("missing"));
        assert!(!p.is_online());
        let err = p
            .upload(&src, &target("", "a.mp4"), None, Progress::none())
            .await
            .unwrap_err();
        assert!(matches!(err, CmError::Network { .. }), "{err:?}");
    }

    #[tokio::test]
    async fn cancel_leaves_no_partial_file() {
        let tmp = tempfile::tempdir().unwrap();
        let src = source(tmp.path(), 2 * CHUNK);
        let root = tmp.path().join("d");
        fs::create_dir(&root).unwrap();
        let progress = Progress::new(|_, _| {}, Arc::new(AtomicBool::new(true)));
        let err = LocalProvider::new(&root)
            .upload(&src, &target("", "a.mp4"), None, progress)
            .await
            .unwrap_err();
        assert!(matches!(err, CmError::Internal { .. }));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn folder_cannot_escape_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let src = source(tmp.path(), 10);
        let root = tmp.path().join("d");
        fs::create_dir(&root).unwrap();
        LocalProvider::new(&root)
            .upload(&src, &target("../out", "a.mp4"), None, Progress::none())
            .await
            .unwrap();
        assert!(root.join("out/a.mp4").exists());
        assert!(!tmp.path().join("out").exists());
    }

    #[tokio::test]
    async fn bundle_copies_claim_numbered_folders_through_the_trait() {
        let tmp = tempfile::tempdir().unwrap();
        let p = LocalProvider::new(tmp.path());
        let a = p.claim_bundle_folder("X/Y", "T.cmrec").await.unwrap();
        let b = p.claim_bundle_folder("X/Y", "T.cmrec").await.unwrap();
        assert_eq!(
            (a.name.as_str(), b.name.as_str()),
            ("T.cmrec", "T (2).cmrec")
        );
        assert!(a.id.is_none());
        assert!(tmp.path().join("X/Y/T (2).cmrec").is_dir());
    }

    #[tokio::test]
    async fn delete_removes_the_recorded_file_only() {
        let tmp = tempfile::tempdir().unwrap();
        let src = source(tmp.path(), 10);
        let root = tmp.path().join("nas");
        fs::create_dir(&root).unwrap();
        let p = LocalProvider::new(&root);
        let up = p
            .upload(&src, &target("A", "a.mp4"), None, Progress::none())
            .await
            .unwrap();
        let other = root.join("A/keep.txt");
        fs::write(&other, "mine").unwrap();
        p.delete(&up.remote_id).await.unwrap();
        assert!(!Path::new(&up.remote_id).exists());
        assert!(other.exists());
        // Gone already: not_found, which the caller reads as "nothing left to delete".
        let e = p.delete(&up.remote_id).await.unwrap_err();
        assert!(matches!(e, CmError::NotFound { .. }), "{e:?}");
    }

    #[tokio::test]
    async fn delete_never_leaves_the_destination_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("nas");
        fs::create_dir(&root).unwrap();
        let outside = tmp.path().join("precious.txt");
        fs::write(&outside, "x").unwrap();
        let p = LocalProvider::new(&root);
        for bad in [
            outside.display().to_string(),
            format!("{}/../precious.txt", root.display()),
            "precious.txt".to_owned(),
            root.display().to_string(),
        ] {
            let e = p.delete(&bad).await.unwrap_err();
            assert!(
                matches!(e, CmError::InvalidInput { .. } | CmError::NotFound { .. }),
                "{bad}: {e:?}"
            );
        }
        assert!(outside.exists() && root.exists());
        // A symbolic link inside the root that points out is not followed.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("link.txt")).unwrap();
            let e = p
                .delete(&root.join("link.txt").display().to_string())
                .await
                .unwrap_err();
            assert!(matches!(e, CmError::InvalidInput { .. }), "{e:?}");
            assert!(outside.exists());
        }
    }

    #[test]
    fn delete_bundle_copy_removes_copied_files_and_empty_folders_but_not_the_users_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("nas");
        let copy = root.join("A/T.cmrec");
        fs::create_dir_all(copy.join("tracks")).unwrap();
        fs::create_dir_all(copy.join("exports")).unwrap();
        fs::write(copy.join("manifest.json"), "{}").unwrap();
        fs::write(copy.join("tracks/screen.mp4"), "v").unwrap();
        fs::write(copy.join("exports/mine.txt"), "keep me").unwrap();
        let p = LocalProvider::new(&root);
        let files = vec!["manifest.json".to_owned(), "tracks/screen.mp4".to_owned()];
        assert_eq!(p.delete_bundle_copy("A/T.cmrec", &files).unwrap(), 2);
        assert!(!copy.join("manifest.json").exists());
        assert!(!copy.join("tracks").exists(), "an emptied folder goes");
        assert!(copy.join("exports/mine.txt").exists());
        assert!(root.join("A").is_dir());
        // The same call again finds nothing to delete and is fine.
        assert_eq!(p.delete_bundle_copy("A/T.cmrec", &files).unwrap(), 0);
        // Without the user's file the whole copy folder is removed.
        fs::remove_file(copy.join("exports/mine.txt")).unwrap();
        p.delete_bundle_copy("A/T.cmrec", &["exports/x".to_owned()])
            .unwrap();
        assert!(!copy.exists());
        assert!(root.join("A").is_dir(), "folders above the copy stay");
    }

    #[test]
    fn delete_bundle_copy_refuses_paths_that_leave_the_copy_or_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("nas");
        fs::create_dir_all(root.join("A/T.cmrec")).unwrap();
        fs::write(tmp.path().join("precious.txt"), "x").unwrap();
        let p = LocalProvider::new(&root);
        for bad in ["../../../precious.txt", "/etc/hosts", "a//b"] {
            let e = p
                .delete_bundle_copy("A/T.cmrec", &[bad.to_owned()])
                .unwrap_err();
            assert!(matches!(e, CmError::InvalidInput { .. }), "{bad}: {e:?}");
        }
        // The folder argument cannot climb out either: `..` segments are dropped.
        let e = p.delete_bundle_copy("../", &[]).unwrap_err();
        assert!(matches!(e, CmError::InvalidInput { .. }), "{e:?}");
        assert!(tmp.path().join("precious.txt").exists() && root.exists());
        // Offline root: network, nothing touched.
        let off = LocalProvider::new(tmp.path().join("missing"));
        assert!(matches!(
            off.delete_bundle_copy("A", &[]).unwrap_err(),
            CmError::Network { .. }
        ));
    }

    // ---- STO-5: replace -------------------------------------------------------------------

    /// A NAS with a bundle copy folder holding `manifest.json`; returns `(tmp, root, file)`.
    fn nas_with_copy() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("nas");
        fs::create_dir_all(root.join("A/T.cmrec")).unwrap();
        let file = root.join("A/T.cmrec/manifest.json");
        fs::write(&file, "old").unwrap();
        (tmp, root, file)
    }

    fn new_version(tmp: &Path, text: &str) -> PathBuf {
        let p = tmp.join("new.json");
        fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn replace_overwrites_the_recorded_file_atomically_and_leaves_no_temp() {
        let (tmp, root, file) = nas_with_copy();
        let p = LocalProvider::new(&root);
        let src = new_version(tmp.path(), "newer and longer");
        let up = p
            .replace_file(
                &src,
                &target("A/T.cmrec", "manifest.json"),
                Some(file.to_str().unwrap()),
                &Progress::none(),
            )
            .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "newer and longer");
        assert_eq!(up.bytes, 16);
        assert_eq!(
            fs::canonicalize(&up.remote_id).unwrap(),
            fs::canonicalize(&file).unwrap()
        );
        let names: Vec<_> = fs::read_dir(root.join("A/T.cmrec"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["manifest.json"], "no temp file stays");
    }

    #[test]
    fn replace_without_an_id_uses_folder_and_name_and_creates_a_missing_file() {
        let (tmp, root, file) = nas_with_copy();
        let p = LocalProvider::new(&root);
        let src = new_version(tmp.path(), "v2");
        p.replace_file(
            &src,
            &target("A/T.cmrec", "manifest.json"),
            None,
            &Progress::none(),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "v2");
        // A file that is new, in a folder that does not exist yet.
        p.replace_file(
            &src,
            &target("A/T.cmrec/sub", "added.json"),
            None,
            &Progress::none(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(root.join("A/T.cmrec/sub/added.json")).unwrap(),
            "v2"
        );
    }

    #[test]
    fn replace_refuses_dotdot_other_folders_and_paths_outside_the_root() {
        let (tmp, root, file) = nas_with_copy();
        fs::create_dir_all(root.join("A/Other.cmrec")).unwrap();
        let other = root.join("A/Other.cmrec/manifest.json");
        fs::write(&other, "other").unwrap();
        fs::write(tmp.path().join("precious.txt"), "x").unwrap();
        let p = LocalProvider::new(&root);
        let src = new_version(tmp.path(), "v2");
        let t = target("A/T.cmrec", "manifest.json");
        let dotdot = format!("{}/A/T.cmrec/../../../precious.txt", root.display());
        let outside = tmp.path().join("precious.txt").display().to_string();
        let sibling = other.display().to_string();
        for bad in [
            dotdot.as_str(),
            outside.as_str(),
            sibling.as_str(),
            "relative/path.json",
        ] {
            let e = p
                .replace_file(&src, &t, Some(bad), &Progress::none())
                .unwrap_err();
            assert!(matches!(e, CmError::InvalidInput { .. }), "{bad}: {e:?}");
        }
        // A `..` in the folder is dropped, so it cannot climb out either.
        p.replace_file(
            &src,
            &target("../../A/T.cmrec", "manifest.json"),
            None,
            &Progress::none(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(tmp.path().join("precious.txt")).unwrap(),
            "x"
        );
        assert_eq!(fs::read_to_string(&other).unwrap(), "other");
        assert_eq!(fs::read_to_string(&file).unwrap(), "v2");
    }

    #[cfg(unix)]
    #[test]
    fn replace_refuses_a_symbolic_link_for_the_file_and_for_the_folder() {
        let (tmp, root, file) = nas_with_copy();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("manifest.json"), "outside").unwrap();
        let p = LocalProvider::new(&root);
        let src = new_version(tmp.path(), "v2");
        let t = target("A/T.cmrec", "manifest.json");

        // The recorded file became a link to something outside.
        fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(outside.join("manifest.json"), &file).unwrap();
        let e = p
            .replace_file(&src, &t, Some(file.to_str().unwrap()), &Progress::none())
            .unwrap_err();
        assert!(matches!(e, CmError::InvalidInput { .. }), "{e:?}");
        let e = p
            .replace_file(&src, &t, None, &Progress::none())
            .unwrap_err();
        assert!(matches!(e, CmError::InvalidInput { .. }), "{e:?}");
        assert_eq!(
            fs::read_to_string(outside.join("manifest.json")).unwrap(),
            "outside"
        );

        // The bundle folder (or a folder below it) became a link out of the root.
        std::os::unix::fs::symlink(&outside, root.join("A/Linked.cmrec")).unwrap();
        for folder in ["A/Linked.cmrec", "A/Linked.cmrec/new/deeper"] {
            let e = p
                .replace_file(
                    &src,
                    &target(folder, "manifest.json"),
                    None,
                    &Progress::none(),
                )
                .unwrap_err();
            assert!(matches!(e, CmError::InvalidInput { .. }), "{folder}: {e:?}");
        }
        assert_eq!(
            fs::read_to_string(outside.join("manifest.json")).unwrap(),
            "outside"
        );
        assert!(!outside.join("new").exists(), "nothing was created outside");
    }

    #[test]
    fn replace_waits_for_an_offline_root() {
        let tmp = tempfile::tempdir().unwrap();
        let off = LocalProvider::new(tmp.path().join("missing"));
        let src = new_version(tmp.path(), "v2");
        let e = off
            .replace_file(&src, &target("A", "x.json"), None, &Progress::none())
            .unwrap_err();
        assert!(matches!(e, CmError::Network { .. }), "{e:?}");
    }

    #[tokio::test]
    async fn the_plain_upload_still_never_overwrites() {
        let (tmp, root, file) = nas_with_copy();
        let src = new_version(tmp.path(), "v2");
        let up = LocalProvider::new(&root)
            .upload(
                &src,
                &target("A/T.cmrec", "manifest.json"),
                None,
                Progress::none(),
            )
            .await
            .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "old");
        assert!(
            up.remote_id.ends_with("manifest (2).json"),
            "{}",
            up.remote_id
        );
    }
}

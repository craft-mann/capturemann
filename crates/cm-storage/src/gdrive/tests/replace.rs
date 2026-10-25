//! STO-5: replacing the content of an uploaded file (tested with the fake Drive).

use super::*;

fn rewrite(r: &Rig, name: &str, size: usize) -> PathBuf {
    let path = r.tmp.path().join(name);
    let data: Vec<u8> = (0..size).map(|i| (i * 17 % 241) as u8).collect();
    std::fs::write(&path, data).unwrap();
    path
}

#[tokio::test(start_paused = true)]
async fn replace_by_id_updates_the_same_file_in_place() {
    let r = rig();
    let first = make_file(&r, "m.json", 3000);
    let up = r
        .provider
        .upload(&first, &target(), None, Progress::none())
        .await
        .unwrap();
    let newer = rewrite(&r, "m2.json", 700_000);
    let done = r
        .provider
        .replace(&newer, &target(), Some(&up.remote_id), Progress::none())
        .await
        .unwrap();
    assert_eq!(done.remote_id, up.remote_id);
    assert_eq!(done.bytes, 700_000);
    let files = r.drive.files();
    assert_eq!(files.len(), 1, "no second file: {files:?}");
    assert_eq!(files[0].data, std::fs::read(&newer).unwrap());
    assert!(r.drive.log().iter().any(|l| l.starts_with("PATCH ")));
    assert!(r.drive.violations().is_empty());
}

#[tokio::test(start_paused = true)]
async fn replace_by_name_finds_the_file_in_the_folder() {
    let r = rig();
    let first = make_file(&r, "m.json", 3000);
    let up = r
        .provider
        .upload(&first, &target(), None, Progress::none())
        .await
        .unwrap();
    let newer = rewrite(&r, "m2.json", 1000);
    // No id (a checkpoint without one): the file of that name in the folder is replaced.
    let done = r
        .provider
        .replace(&newer, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(done.remote_id, up.remote_id);
    assert_eq!(r.drive.files().len(), 1);
    assert_eq!(r.drive.files()[0].data, std::fs::read(&newer).unwrap());
}

#[tokio::test(start_paused = true)]
async fn replace_with_a_deleted_id_falls_back_to_the_name_then_to_creating() {
    let r = rig();
    let first = make_file(&r, "m.json", 3000);
    let up = r
        .provider
        .upload(&first, &target(), None, Progress::none())
        .await
        .unwrap();
    r.provider.delete(&up.remote_id).await.unwrap();
    let newer = rewrite(&r, "m2.json", 1000);
    let done = r
        .provider
        .replace(&newer, &target(), Some(&up.remote_id), Progress::none())
        .await
        .unwrap();
    assert_ne!(done.remote_id, up.remote_id);
    assert_eq!(r.drive.files().len(), 1);
    assert_eq!(r.drive.files()[0].data, std::fs::read(&newer).unwrap());
}

#[tokio::test(start_paused = true)]
async fn replace_never_touches_a_file_of_another_name() {
    let r = rig();
    let a = make_file(&r, "a.json", 100);
    let other = Target {
        folder: target().folder,
        file_name: "other.json".into(),
    };
    r.provider
        .upload(&a, &other, None, Progress::none())
        .await
        .unwrap();
    let newer = rewrite(&r, "m2.json", 50);
    let done = r
        .provider
        .replace(&newer, &target(), None, Progress::none())
        .await
        .unwrap();
    let files = r.drive.files();
    assert_eq!(files.len(), 2);
    let kept = files.iter().find(|f| f.name == "other.json").unwrap();
    assert_eq!(kept.data.len(), 100);
    assert_eq!(
        files.iter().find(|f| f.id == done.remote_id).unwrap().name,
        "Lesson.mp4"
    );
}

#[tokio::test(start_paused = true)]
async fn replace_keeps_the_share_state_and_makes_no_new_link() {
    let r = rig();
    let first = make_file(&r, "m.json", 3000);
    let up = r
        .provider
        .upload(&first, &target(), None, Progress::none())
        .await
        .unwrap();
    assert!(up.share_link.is_some());
    let newer = rewrite(&r, "m2.json", 1000);
    let done = r
        .provider
        .replace(&newer, &target(), Some(&up.remote_id), Progress::none())
        .await
        .unwrap();
    assert!(done.share_link.is_none());
    assert_eq!(
        r.drive.permissions().len(),
        1,
        "permissions are not asked again"
    );
}

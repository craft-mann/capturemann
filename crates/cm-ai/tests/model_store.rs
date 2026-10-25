//! The model store against a local fake HTTP server: resumable downloads, checksums, cancel,
//! import, selection. No test touches the network beyond 127.0.0.1.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use cm_ai::model::{error_data, is_no_model, Catalog, CatalogEntry, DownloadOutcome, ModelStore};
use sha2::{Digest, Sha256};

/// Deterministic pseudo-random bytes (so a wrong resume offset changes the checksum).
fn body(len: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}

#[derive(Default)]
struct Behavior {
    /// Close the connection after this many body bytes on the first body response.
    cut_first_at: Option<usize>,
    /// Answer 200 with the whole body even when a Range is asked for.
    ignore_range: bool,
    /// Answer every request with this status instead of the file.
    status: Option<u16>,
}

struct Server {
    port: u16,
    /// `Range` headers received, in order (`None` for a request without one).
    ranges: Arc<Mutex<Vec<Option<String>>>>,
    requests: Arc<AtomicUsize>,
}

fn serve(data: Arc<Vec<u8>>, behavior: Behavior) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(AtomicUsize::new(0));
    let behavior = Arc::new(behavior);
    let cut_used = Arc::new(AtomicBool::new(false));
    let (r2, q2) = (ranges.clone(), requests.clone());
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (data, behavior, cut_used) = (data.clone(), behavior.clone(), cut_used.clone());
            let (ranges, requests) = (r2.clone(), q2.clone());
            thread::spawn(move || handle(stream, &data, &behavior, &cut_used, &ranges, &requests));
        }
    });
    Server {
        port,
        ranges,
        requests,
    }
}

fn handle(
    mut stream: TcpStream,
    data: &[u8],
    b: &Behavior,
    cut_used: &AtomicBool,
    ranges: &Mutex<Vec<Option<String>>>,
    requests: &AtomicUsize,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    let mut range: Option<String> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("range:") {
            range = Some(v.trim().to_owned());
        }
    }
    requests.fetch_add(1, Ordering::SeqCst);
    if path == "/redirect" {
        let _ = stream.write_all(
            b"HTTP/1.1 302 Found\r\nLocation: /model.bin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    }
    ranges.lock().unwrap().push(range.clone());
    if let Some(code) = b.status {
        let _ = stream.write_all(
            format!("HTTP/1.1 {code} Err\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        );
        return;
    }
    let start = if b.ignore_range {
        0
    } else {
        range
            .as_deref()
            .and_then(|r| r.strip_prefix("bytes="))
            .and_then(|r| r.strip_suffix('-'))
            .and_then(|r| r.parse::<usize>().ok())
            .unwrap_or(0)
    };
    let partial = start > 0;
    let payload = &data[start.min(data.len())..];
    let head = if partial {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
            payload.len(),
            data.len() - 1,
            data.len()
        )
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
            payload.len()
        )
    };
    let _ = stream.write_all(head.as_bytes());
    if let Some(cut) = b.cut_first_at {
        if !cut_used.swap(true, Ordering::SeqCst) {
            let _ = stream.write_all(&payload[..cut.min(payload.len())]);
            let _ = stream.flush();
            return; // closes the socket short of Content-Length
        }
    }
    for chunk in payload.chunks(64 * 1024) {
        if stream.write_all(chunk).is_err() {
            return;
        }
    }
}

fn entry(port: u16, path: &str, data: &[u8], sha256: String) -> CatalogEntry {
    CatalogEntry {
        id: "tiny".into(),
        label: "Tiny".into(),
        hint: "test".into(),
        file: "ggml-tiny.bin".into(),
        url: format!("http://127.0.0.1:{port}{path}"),
        sha256,
        bytes: data.len() as u64,
        ram_mb: 0,
    }
}

fn store_with(dir: &Path, e: CatalogEntry) -> ModelStore {
    ModelStore::with_catalog(dir.join("ai"), Catalog { models: vec![e] })
}

fn never(_: cm_ai::model::DownloadProgress) -> bool {
    true
}

#[test]
fn download_installs_a_verified_file_and_makes_it_active() {
    let data = Arc::new(body(3_000_000));
    let server = serve(data.clone(), Behavior::default());
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        entry(server.port, "/model.bin", &data, sha(&data)),
    );
    assert!(!store.is_installed("tiny"));
    let mut last = 0;
    let out = store
        .download("tiny", &mut |p| {
            assert_eq!(p.bytes_total, 3_000_000);
            assert!(p.bytes_done >= last);
            last = p.bytes_done;
            true
        })
        .unwrap();
    let DownloadOutcome::Done(path) = out else {
        panic!("not done")
    };
    assert_eq!(fs::read(&path).unwrap(), *data);
    assert_eq!(last, 3_000_000);
    assert!(!store.models_dir().join("ggml-tiny.bin.part").exists());
    let list = store.list();
    assert!(list.models[0].installed && list.models[0].active);
    assert_eq!(list.active.as_deref(), Some("tiny"));
    // Already installed: a second call does not ask the server again.
    let before = server.requests.load(Ordering::SeqCst);
    assert!(matches!(
        store.download("tiny", &mut never).unwrap(),
        DownloadOutcome::Done(_)
    ));
    assert_eq!(server.requests.load(Ordering::SeqCst), before);
}

#[test]
fn an_interrupted_download_resumes_with_a_range_request() {
    let data = Arc::new(body(3_000_000));
    let server = serve(
        data.clone(),
        Behavior {
            cut_first_at: Some(1_000_000),
            ..Behavior::default()
        },
    );
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        entry(server.port, "/model.bin", &data, sha(&data)),
    );
    let e = store.download("tiny", &mut never).unwrap_err();
    assert_eq!(e.code().as_str(), "network", "{e}");
    assert!(e.is_retryable());
    let part = store.models_dir().join("ggml-tiny.bin.part");
    let have = fs::metadata(&part).unwrap().len();
    assert!(have > 0 && have < 3_000_000, "{have}");
    assert!(!store.is_installed("tiny"));
    // The second call asks for the rest only and ends with the right bytes.
    assert!(matches!(
        store.download("tiny", &mut never).unwrap(),
        DownloadOutcome::Done(_)
    ));
    let ranges = server.ranges.lock().unwrap().clone();
    assert_eq!(ranges[0], None);
    assert_eq!(
        ranges[1].as_deref(),
        Some(format!("bytes={have}-").as_str())
    );
    assert_eq!(fs::read(store.path_of("tiny").unwrap()).unwrap(), *data);
}

#[test]
fn a_server_that_ignores_the_range_gets_a_clean_restart() {
    let data = Arc::new(body(2_000_000));
    let server = serve(
        data.clone(),
        Behavior {
            ignore_range: true,
            ..Behavior::default()
        },
    );
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        entry(server.port, "/model.bin", &data, sha(&data)),
    );
    fs::create_dir_all(store.models_dir()).unwrap();
    fs::write(
        store.models_dir().join("ggml-tiny.bin.part"),
        &data[..500_000],
    )
    .unwrap();
    assert!(matches!(
        store.download("tiny", &mut never).unwrap(),
        DownloadOutcome::Done(_)
    ));
    assert_eq!(fs::read(store.path_of("tiny").unwrap()).unwrap(), *data);
}

#[test]
fn a_wrong_checksum_deletes_the_file_and_fails() {
    let data = Arc::new(body(1_000_000));
    let server = serve(data.clone(), Behavior::default());
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        entry(server.port, "/model.bin", &data, "0".repeat(64)),
    );
    let e = store.download("tiny", &mut never).unwrap_err();
    assert!(e.to_string().contains("checksum"), "{e}");
    assert!(!store.is_installed("tiny"));
    assert!(!store.models_dir().join("ggml-tiny.bin.part").exists());
}

#[test]
fn cancelling_keeps_the_partial_file_for_a_later_resume() {
    let data = Arc::new(body(4_000_000));
    let server = serve(data.clone(), Behavior::default());
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        entry(server.port, "/model.bin", &data, sha(&data)),
    );
    let out = store
        .download("tiny", &mut |p| p.bytes_done < 300_000)
        .unwrap();
    assert_eq!(out, DownloadOutcome::Cancelled);
    assert!(!store.is_installed("tiny"));
    let part = store.models_dir().join("ggml-tiny.bin.part");
    assert!(part.is_file());
    assert!(matches!(
        store.download("tiny", &mut never).unwrap(),
        DownloadOutcome::Done(_)
    ));
    assert_eq!(fs::read(store.path_of("tiny").unwrap()).unwrap(), *data);
}

#[test]
fn redirects_are_followed_and_http_errors_are_network_errors() {
    let data = Arc::new(body(500_000));
    let server = serve(data.clone(), Behavior::default());
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(
        dir.path(),
        entry(server.port, "/redirect", &data, sha(&data)),
    );
    assert!(matches!(
        store.download("tiny", &mut never).unwrap(),
        DownloadOutcome::Done(_)
    ));

    let failing = serve(
        data.clone(),
        Behavior {
            status: Some(404),
            ..Behavior::default()
        },
    );
    let dir2 = tempfile::tempdir().unwrap();
    let store2 = store_with(
        dir2.path(),
        entry(failing.port, "/model.bin", &data, sha(&data)),
    );
    let e = store2.download("tiny", &mut never).unwrap_err();
    assert_eq!(e.code().as_str(), "network");
    assert!(e.to_string().contains("404"), "{e}");
}

#[test]
fn an_unreachable_server_is_a_retryable_network_error() {
    let data = body(1000);
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(dir.path(), entry(1, "/model.bin", &data, sha(&data)));
    let e = store.download("tiny", &mut never).unwrap_err();
    assert_eq!(e.code().as_str(), "network");
}

/// Listing, resolving and selecting never start a download: with a URL nobody serves they still
/// answer at once, and resolve says what to do.
#[test]
fn nothing_downloads_unless_download_is_called() {
    let data = body(1000);
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(dir.path(), entry(1, "/model.bin", &data, sha(&data)));
    assert!(!store.list().models[0].installed);
    assert_eq!(store.list().active, None);
    let e = store.resolve(None).unwrap_err();
    assert!(is_no_model(&e));
    assert_eq!(error_data(&e).unwrap()["reason"], "no_model");
    let e = store.resolve(Some("tiny")).unwrap_err();
    assert!(is_no_model(&e) && e.to_string().contains("`tiny`"));
    assert_eq!(
        store.resolve(Some("huge")).unwrap_err().code().as_str(),
        "invalid_input"
    );
    assert!(store.select("tiny").is_err());
    assert!(
        !store.models_dir().exists(),
        "the store creates nothing until it is used"
    );
}

/// A file with the ggml magic and a plausible size (sparse, so it costs nothing to make).
fn fake_model(path: &Path, len: u64, magic: [u8; 4]) {
    let mut f = fs::File::create(path).unwrap();
    f.write_all(&magic).unwrap();
    f.set_len(len).unwrap();
}

const GGML: [u8; 4] = [0x6c, 0x6d, 0x67, 0x67];

#[test]
fn import_checks_the_header_and_size_then_installs_and_activates() {
    let dir = tempfile::tempdir().unwrap();
    let data = body(10);
    let store = store_with(dir.path(), entry(1, "/x", &data, sha(&data)));
    let src = dir.path().join("mine.bin");

    fake_model(&src, 30_000_000, [1, 2, 3, 4]);
    let e = store.import("tiny", &src).unwrap_err();
    assert!(e.to_string().contains("header"), "{e}");
    fake_model(&src, 1_000, GGML);
    assert!(store
        .import("tiny", &src)
        .unwrap_err()
        .to_string()
        .contains("size"));
    fake_model(&src, 5_000_000_000, GGML);
    assert!(store.import("tiny", &src).is_err());
    assert!(store.import("tiny", &dir.path().join("none.bin")).is_err());
    assert!(store.import("nope", &src).is_err());
    assert!(!store.is_installed("tiny"));

    fake_model(&src, 30_000_000, GGML);
    let dest = store.import("tiny", &src).unwrap();
    assert!(dest.is_file());
    assert!(src.is_file(), "import copies, it does not move");
    assert_eq!(store.active().as_deref(), Some("tiny"));
    assert_eq!(store.resolve(None).unwrap().0, "tiny");
}

#[test]
fn select_remove_and_the_active_choice() {
    let dir = tempfile::tempdir().unwrap();
    let h = sha(b"x");
    let catalog = Catalog {
        models: ["a", "b"]
            .iter()
            .map(|id| CatalogEntry {
                id: (*id).into(),
                label: id.to_uppercase(),
                hint: String::new(),
                file: format!("ggml-{id}.bin"),
                url: "http://127.0.0.1:1/x".into(),
                sha256: h.clone(),
                bytes: 5,
                ram_mb: 0,
            })
            .collect(),
    };
    let store = ModelStore::with_catalog(dir.path().join("ai"), catalog);
    let src = dir.path().join("m.bin");
    fake_model(&src, 30_000_000, GGML);
    store.import("a", &src).unwrap();
    store.import("b", &src).unwrap();
    // The first import became active; select moves it.
    assert_eq!(store.active().as_deref(), Some("a"));
    store.select("b").unwrap();
    assert_eq!(store.resolve(None).unwrap().0, "b");
    assert_eq!(store.resolve(Some("a")).unwrap().0, "a");
    let list = store.list();
    assert!(list.models[1].active && !list.models[0].active);
    // Two installed and no active one is ambiguous: resolve asks for a choice.
    assert!(store.remove("b").unwrap());
    assert_eq!(store.active(), None);
    assert_eq!(
        store.resolve(None).unwrap().0,
        "a",
        "a single installed model is used"
    );
    store.import("b", &src).unwrap();
    store.remove("a").unwrap();
    assert!(
        !store.remove("a").unwrap(),
        "removing twice reports nothing removed"
    );
    // A deleted file clears the choice by itself.
    fs::remove_file(store.path_of("b").unwrap()).unwrap();
    assert_eq!(store.active(), None);
    assert!(store.resolve(None).is_err());
}

#[test]
fn removing_also_deletes_a_partial_download() {
    let dir = tempfile::tempdir().unwrap();
    let data = body(10);
    let store = store_with(dir.path(), entry(1, "/x", &data, sha(&data)));
    fs::create_dir_all(store.models_dir()).unwrap();
    let part = store.models_dir().join("ggml-tiny.bin.part");
    fs::write(&part, b"abc").unwrap();
    assert!(store.remove("tiny").unwrap());
    assert!(!part.exists());
}

/// Reads of the state file are tolerant: a damaged one means "no active model".
#[test]
fn a_damaged_state_file_means_no_active_model() {
    let dir = tempfile::tempdir().unwrap();
    let data = body(10);
    let store = store_with(dir.path(), entry(1, "/x", &data, sha(&data)));
    fs::create_dir_all(dir.path().join("ai")).unwrap();
    fs::write(dir.path().join("ai/state.json"), "{ nope").unwrap();
    assert_eq!(store.active(), None);
    let mut buf = String::new();
    fs::File::open(dir.path().join("ai/state.json"))
        .unwrap()
        .read_to_string(&mut buf)
        .unwrap();
    assert_eq!(buf, "{ nope", "reading never rewrites it");
}

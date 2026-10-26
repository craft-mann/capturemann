//! The client against a fake in-process JSON-RPC server on a Unix socket in a temp dir (never
//! a real daemon socket). Wire format per `cm-daemon/src/rpc.rs` and specs/02-agent-interface.md:
//! one JSON message per line, errors carry the shared code in `error.data.code`.
#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use std::io;
use std::path::Path;
use std::time::Duration;

use cm_ipc::{connect, Endpoint, RpcClient, RpcError};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::time::timeout;

const T: Duration = Duration::from_secs(5);

fn sock(dir: &Path) -> Endpoint {
    Endpoint::Path(dir.join("d.sock"))
}

/// Accepts one connection and runs `script` over the raw stream.
fn serve_once<F, Fut>(ep: &Endpoint, script: F) -> tokio::task::JoinHandle<()>
where
    F: FnOnce(UnixStream) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let Endpoint::Path(p) = ep else { panic!() };
    let l = UnixListener::bind(p).unwrap();
    tokio::spawn(async move {
        let (s, _) = l.accept().await.unwrap();
        script(s).await;
    })
}

async fn read_req(r: &mut BufReader<tokio::net::unix::OwnedReadHalf>) -> Value {
    let mut line = String::new();
    r.read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap()
}

/// Connecting where nothing listens fails fast with a clear io error, never hangs.
#[tokio::test]
async fn connect_without_a_daemon_fails_fast() {
    let t = tempfile::tempdir().unwrap();
    let ep = sock(t.path());
    let e = timeout(T, connect(&ep))
        .await
        .expect("no hang")
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotFound, "{e}");

    // A stale socket file with no listener: connection refused.
    let Endpoint::Path(p) = &ep else { panic!() };
    drop(UnixListener::bind(p).unwrap());
    let e = timeout(T, connect(&ep))
        .await
        .expect("no hang")
        .unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::ConnectionRefused, "{e}");

    // A pipe endpoint is not usable on Unix.
    let e = connect(&Endpoint::Pipe("x".into())).await.unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
}

/// The request line is exactly one JSON-RPC 2.0 object ending in a newline, ids count up from 1.
#[tokio::test]
async fn requests_are_one_line_jsonrpc_with_increasing_ids() {
    let t = tempfile::tempdir().unwrap();
    let ep = sock(t.path());
    let server = serve_once(&ep, |s| async move {
        let (r, mut w) = s.into_split();
        let mut r = BufReader::new(r);
        for expect_id in 1..=3u64 {
            let mut raw = String::new();
            r.read_line(&mut raw).await.unwrap();
            assert!(
                raw.ends_with('\n') && raw.matches('\n').count() == 1,
                "{raw:?}"
            );
            let v: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(v["jsonrpc"], "2.0");
            assert_eq!(v["id"], expect_id);
            assert_eq!(v["method"], "capture_screenshot");
            // Embedded newlines in params stay escaped inside the line.
            assert_eq!(v["params"], json!({"note": "a\nb", "n": expect_id}));
            let resp = json!({"jsonrpc":"2.0","id":expect_id,"result":{"echo":v["params"]}});
            w.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
        }
    });
    let mut c = RpcClient::new(timeout(T, connect(&ep)).await.unwrap().unwrap());
    for n in 1..=3u64 {
        let r = timeout(
            T,
            c.call("capture_screenshot", json!({"note": "a\nb", "n": n})),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(r["echo"]["n"], n);
    }
    server.await.unwrap();
}

type Check = fn(Result<Value, RpcError>);

/// Table: server reply lines and what `call` makes of them. Error mapping keeps the whole
/// error object so callers read `data.code` (shared codes of specs/README.md).
#[tokio::test]
async fn reply_table() {
    let perm = json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"no",
        "data":{"code":"permission_missing","message":"no","data":null}}})
    .to_string();
    let table: Vec<(&str, Vec<String>, Check)> = vec![
        (
            "plain result",
            vec![r#"{"jsonrpc":"2.0","id":1,"result":7}"#.into()],
            |r| assert_eq!(r.unwrap(), json!(7)),
        ),
        (
            "null result",
            vec![r#"{"jsonrpc":"2.0","id":1,"result":null}"#.into()],
            |r| assert_eq!(r.unwrap(), Value::Null),
        ),
        (
            "missing result is null",
            vec![r#"{"jsonrpc":"2.0","id":1}"#.into()],
            |r| assert_eq!(r.unwrap(), Value::Null),
        ),
        (
            "blank lines skipped",
            vec!["".into(), "   ".into(), r#"{"id":1,"result":1}"#.into()],
            |r| assert_eq!(r.unwrap(), json!(1)),
        ),
        ("CRLF line", vec!["{\"id\":1,\"result\":2}\r".into()], |r| {
            assert_eq!(r.unwrap(), json!(2))
        }),
        ("error keeps shared code", vec![perm], |r| match r {
            Err(RpcError::Rpc(v)) => {
                assert_eq!(v["data"]["code"], "permission_missing");
                assert_eq!(v["code"], -32000);
            }
            o => panic!("{o:?}"),
        }),
        (
            "error wins over result",
            vec![r#"{"id":1,"result":1,"error":{"message":"x"}}"#.into()],
            |r| assert!(matches!(r, Err(RpcError::Rpc(_)))),
        ),
        ("not json", vec!["not json".into()], |r| {
            assert!(matches!(r, Err(RpcError::InvalidJson(_))))
        }),
        ("truncated json", vec![r#"{"id":1,"resu"#.into()], |r| {
            assert!(matches!(r, Err(RpcError::InvalidJson(_))))
        }),
        (
            "events and foreign ids skipped",
            vec![
                r#"{"method":"event","params":{"seq":1}}"#.into(),
                r#"{"id":99,"result":"late"}"#.into(),
                r#"{"id":"1","result":"string id is not ours"}"#.into(),
                r#"{"id":null,"error":{"message":"parse"}}"#.into(),
                r#"{"id":1,"result":"ours"}"#.into(),
            ],
            |r| assert_eq!(r.unwrap(), json!("ours")),
        ),
        (
            "non-object message ignored",
            vec!["42".into(), r#"{"id":1,"result":3}"#.into()],
            |r| assert_eq!(r.unwrap(), json!(3)),
        ),
    ];
    for (name, lines, check) in table {
        let t = tempfile::tempdir().unwrap();
        let ep = sock(t.path());
        let server = serve_once(&ep, move |s| async move {
            let (r, mut w) = s.into_split();
            let mut r = BufReader::new(r);
            read_req(&mut r).await;
            for l in lines {
                w.write_all(format!("{l}\n").as_bytes()).await.unwrap();
            }
            // keep the connection open until the client is done
            let mut rest = String::new();
            let _ = r.read_line(&mut rest).await;
        });
        let mut c = RpcClient::new(connect(&ep).await.unwrap());
        let res = timeout(T, c.call("m", json!({})))
            .await
            .unwrap_or_else(|_| panic!("{name}: hang"));
        check(res);
        drop(c);
        let _ = server.await;
    }
}

/// A daemon that stops or closes: `Io(None)` for a clean close, invalid JSON for a cut line;
/// never a hang, and the dead connection keeps failing fast.
#[tokio::test]
async fn closed_and_partial_streams_end_the_call() {
    for (name, tail) in [
        ("clean close", ""),
        ("partial line then close", r#"{"id":1,"res"#),
    ] {
        let t = tempfile::tempdir().unwrap();
        let ep = sock(t.path());
        let tail = tail.to_owned();
        let server = serve_once(&ep, move |s| async move {
            let (r, mut w) = s.into_split();
            let mut r = BufReader::new(r);
            read_req(&mut r).await;
            w.write_all(tail.as_bytes()).await.unwrap();
        });
        let mut c = RpcClient::new(connect(&ep).await.unwrap());
        let res = timeout(T, c.call("m", json!({}))).await.expect(name);
        match (name, res) {
            ("clean close", Err(RpcError::Io(None))) => {}
            ("partial line then close", Err(RpcError::InvalidJson(_))) => {}
            (n, o) => panic!("{n}: {o:?}"),
        }
        let again = timeout(T, c.call("m", json!({}))).await.expect("no hang");
        assert!(again.is_err());
        server.await.unwrap();
    }
}

/// Writing to a daemon that is gone surfaces as an io error, not a panic or hang.
#[tokio::test]
async fn call_after_the_daemon_dropped_the_socket_is_an_io_error() {
    let t = tempfile::tempdir().unwrap();
    let ep = sock(t.path());
    let server = serve_once(&ep, |s| async move { drop(s) });
    let mut c = RpcClient::new(connect(&ep).await.unwrap());
    server.await.unwrap();
    for _ in 0..2 {
        let r = timeout(T, c.call("m", json!({}))).await.expect("no hang");
        assert!(matches!(r, Err(RpcError::Io(_))), "{r:?}");
    }
}

/// Event stream: events seen during a call are queued and come out of `next_event` first, in
/// order, then live ones; `Io(None)` at the end. Non-event notifications are skipped.
#[tokio::test]
async fn event_stream_order_and_end() {
    let t = tempfile::tempdir().unwrap();
    let ep = sock(t.path());
    let server = serve_once(&ep, |s| async move {
        let (r, mut w) = s.into_split();
        let mut r = BufReader::new(r);
        let req = read_req(&mut r).await;
        assert_eq!(req["method"], "events.subscribe");
        let lines = [
            json!({"jsonrpc":"2.0","method":"event","params":{"seq":1,"event":"capture.revised"}}),
            json!({"jsonrpc":"2.0","method":"event","params":{"seq":2,"event":"job.done"}}),
            json!({"jsonrpc":"2.0","id":1,"result":{"subscribed":true}}),
            json!({"jsonrpc":"2.0","method":"other","params":{"seq":3}}),
            json!({"jsonrpc":"2.0","method":"event","params":{"seq":4,"event":"events.dropped"}}),
        ];
        for l in lines {
            w.write_all(format!("{l}\n").as_bytes()).await.unwrap();
        }
    });
    let mut c = RpcClient::new(connect(&ep).await.unwrap());
    let r = timeout(
        T,
        c.call("events.subscribe", json!({"topics": ["capture.*"]})),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(r["subscribed"], true);
    let mut seqs = vec![];
    for _ in 0..3 {
        let ev = timeout(T, c.next_event()).await.unwrap().unwrap();
        seqs.push(ev["seq"].as_u64().unwrap());
    }
    assert_eq!(seqs, vec![1, 2, 4]);
    let end = timeout(T, c.next_event()).await.unwrap();
    assert!(matches!(end, Err(RpcError::Io(None))), "{end:?}");
    server.await.unwrap();
}

/// A very long line (1 MiB, well under `MAX_LINE_BYTES`) is read whole.
#[tokio::test]
async fn long_response_lines_are_read_whole() {
    let t = tempfile::tempdir().unwrap();
    let ep = sock(t.path());
    let big = "x".repeat(1 << 20);
    let big2 = big.clone();
    let server = serve_once(&ep, move |s| async move {
        let (r, mut w) = s.into_split();
        let mut r = BufReader::new(r);
        read_req(&mut r).await;
        let resp = json!({"jsonrpc":"2.0","id":1,"result":big2});
        w.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
    });
    let mut c = RpcClient::new(connect(&ep).await.unwrap());
    let r = timeout(T, c.call("m", json!({}))).await.unwrap().unwrap();
    assert_eq!(r.as_str().unwrap().len(), big.len());
    server.await.unwrap();
}

/// A line over `MAX_LINE_BYTES` ends the call with an `InvalidData` error and the client never
/// buffers more than the limit (a faulty or hostile peer cannot grow it without bound).
#[tokio::test]
async fn an_over_long_response_line_is_an_error() {
    let t = tempfile::tempdir().unwrap();
    let ep = sock(t.path());
    let server = serve_once(&ep, |s| async move {
        let (r, mut w) = s.into_split();
        let mut r = BufReader::new(r);
        read_req(&mut r).await;
        let chunk = vec![b'x'; 1 << 20];
        for _ in 0..(cm_ipc::MAX_LINE_BYTES >> 20) + 1 {
            if w.write_all(&chunk).await.is_err() {
                return;
            }
        }
        let _ = w.write_all(b"\n").await;
    });
    let mut c = RpcClient::new(connect(&ep).await.unwrap());
    let r = timeout(T, c.call("m", json!({}))).await.unwrap();
    match r {
        Err(RpcError::Io(Some(e))) => assert_eq!(e.kind(), io::ErrorKind::InvalidData, "{e}"),
        other => panic!("{other:?}"),
    }
    drop(c);
    let _ = server.await;
}

/// A reconnect after the daemon restarts on the same path works with a fresh client (ids
/// restart at 1); the old client keeps failing.
#[tokio::test]
async fn reconnect_after_daemon_restart() {
    let t = tempfile::tempdir().unwrap();
    let ep = sock(t.path());
    let Endpoint::Path(p) = ep.clone() else {
        panic!()
    };
    let first = serve_once(&ep, |s| async move { drop(s) });
    let mut old = RpcClient::new(connect(&ep).await.unwrap());
    first.await.unwrap();
    std::fs::remove_file(&p).unwrap();
    let second = serve_once(&ep, |s| async move {
        let (r, mut w) = s.into_split();
        let mut r = BufReader::new(r);
        let req = read_req(&mut r).await;
        assert_eq!(req["id"], 1);
        w.write_all(b"{\"id\":1,\"result\":\"back\"}\n")
            .await
            .unwrap();
    });
    assert!(timeout(T, old.call("m", json!({}))).await.unwrap().is_err());
    let mut fresh = RpcClient::new(connect(&ep).await.unwrap());
    assert_eq!(
        timeout(T, fresh.call("m", json!({})))
            .await
            .unwrap()
            .unwrap(),
        json!("back")
    );
    second.await.unwrap();
}

/// The client is sequential, so concurrency is one connection per caller: many clients at once
/// against one server each get their own answer back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_connections_get_their_own_answers() {
    let t = tempfile::tempdir().unwrap();
    let ep = sock(t.path());
    let Endpoint::Path(p) = &ep else { panic!() };
    let l = UnixListener::bind(p).unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..16 {
            let (s, _) = l.accept().await.unwrap();
            tokio::spawn(async move {
                let (r, mut w) = s.into_split();
                let mut r = BufReader::new(r);
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).await.unwrap() == 0 {
                        return;
                    }
                    let v: Value = serde_json::from_str(&line).unwrap();
                    let n = v["params"]["n"].as_u64().unwrap();
                    // stagger replies so completion order differs between connections
                    tokio::time::sleep(Duration::from_millis(20 - n % 16)).await;
                    let resp = json!({"jsonrpc":"2.0","id":v["id"],"result":{"n":n}});
                    w.write_all(format!("{resp}\n").as_bytes()).await.unwrap();
                }
            });
        }
    });
    let mut tasks = vec![];
    for n in 0..16u64 {
        let ep = ep.clone();
        tasks.push(tokio::spawn(async move {
            let mut c = RpcClient::new(connect(&ep).await.unwrap());
            for k in 0..3u64 {
                let r = c.call("m", json!({"n": n * 100 + k})).await.unwrap();
                assert_eq!(r["n"], n * 100 + k);
            }
        }));
    }
    for t in tasks {
        timeout(T, t).await.unwrap().unwrap();
    }
    server.abort();
}

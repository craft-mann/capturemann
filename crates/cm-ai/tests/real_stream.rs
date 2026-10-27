//! A manual check of the streaming text-model path against a real `llama-server` (local
//! inference on loopback; no network, no sensors). Never runs by itself:
//!
//! ```text
//! CM_LLAMA_SERVER=<llama-server> CM_LLAMA_GGUF=<model.gguf> \
//!   cargo test -p cm-ai --test real_stream -- --ignored --nocapture
//! ```
#![allow(clippy::unwrap_used)]

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use cm_ai::text_model::{HttpTextModel, JsonRequest, TextModel as _};
use serde_json::json;

struct Helper(Child);

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "needs a real llama-server and a GGUF model"]
fn streams_a_small_json_answer_from_a_real_llama_server() {
    let (Ok(server), Ok(gguf)) = (
        std::env::var("CM_LLAMA_SERVER"),
        std::env::var("CM_LLAMA_GGUF"),
    ) else {
        println!("SKIPPED: set CM_LLAMA_SERVER and CM_LLAMA_GGUF");
        return;
    };
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = Command::new(server)
        .args(["--model", &gguf, "--host", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .args([
            "--ctx-size",
            "16384",
            "--n-gpu-layers",
            "999",
            "--parallel",
            "1",
            "--no-webui",
            "--reasoning",
            "off",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _helper = Helper(child);
    let health = format!("http://127.0.0.1:{port}/health");
    let t0 = Instant::now();
    loop {
        let ok = ureq::get(&health)
            .call()
            .map(|r| r.status().as_u16() == 200)
            .unwrap_or(false);
        if ok {
            break;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(180),
            "the helper did not load"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
    println!("loaded in {:?}", t0.elapsed());

    let model = HttpTextModel::new(format!("http://127.0.0.1:{port}/v1"), "gemma", 120);
    let mut req = JsonRequest::new(
        "Write a title (three to five words) and four short bullet points about why people \
         keep meeting notes.",
        json!({ "type": "object",
                "properties": { "title": { "type": "string" },
                                "points": { "type": "array", "items": { "type": "string" } } },
                "required": ["title", "points"], "additionalProperties": false }),
    );
    req.schema_name = "notes".into();
    req.max_tokens = 400;
    let mut ticks: Vec<(usize, Duration)> = Vec::new();
    let t1 = Instant::now();
    let reply = model
        .complete_json_streaming(&req, &mut |n| {
            ticks.push((n, t1.elapsed()));
            true
        })
        .unwrap();
    println!("callbacks: {}", ticks.len());
    println!(
        "first at {:?}, last at {:?} (count {})",
        ticks.first().map(|t| t.1),
        ticks.last().map(|t| t.1),
        ticks.last().map_or(0, |t| t.0)
    );
    println!("output: {}", reply.value);
    assert!(ticks.len() > 5, "the answer must arrive in several pieces");
    assert!(ticks.windows(2).all(|w| w[0].0 < w[1].0));
    assert!(reply.value["title"].is_string());
}

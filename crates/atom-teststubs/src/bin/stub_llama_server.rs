//! Stand-in for `llama-server`.
//!
//! Serves only what ATOM talks to: `/health`, `/props`, and
//! `/v1/chat/completions` in both streaming (SSE) and batch mode. Everything is
//! std-only so the stub builds in any test environment.
//!
//! * `ATOM_STUB_LOAD_MS`   artificial startup delay before the socket binds
//! * `ATOM_STUB_TOKENS`    number of SSE token deltas (default 4)
//! * `ATOM_STUB_DELAY_MS`  delay between deltas (default 25)
//! * `ATOM_STUB_HTTP_ERROR` return HTTP 500 from the completions endpoint
//! * `ATOM_STUB_BUILD`     build_info reported by `/props`

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

fn env_num(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

const TOKENS: [&str; 8] = [
    "The ", "answer ", "is ", "four", ", ", "because ", "2 + 2 = 4", ".",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `cargo test` executes this bin with no arguments; that is not a server.
    if args.is_empty() {
        return;
    }
    if args.iter().any(|a| {
        a == "--list"
            || a == "--exact"
            || a == "--ignored"
            || a == "--nocapture"
            || a == "--show-output"
            || a == "--quiet"
            || a.starts_with("--test-threads")
            || a.starts_with("--format")
    }) {
        return;
    }
    // ATOM probes the toolchain with `--version`; without this the stub would
    // start serving and the probe would wait forever for it to exit.
    if args.iter().any(|a| a == "--version") {
        println!("llama-server stub (atom test fixture), build stub-0000");
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage: llama-server -m <model> --port <port> [options]");
        return;
    }

    let port = flag_value(&args, "--port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(0);
    let load_ms = env_num("ATOM_STUB_LOAD_MS", 0);
    std::thread::sleep(Duration::from_millis(load_ms));

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("stub llama-server: cannot bind port {port}: {e}");
            std::process::exit(2);
        }
    };
    println!("stub llama-server listening on 127.0.0.1:{port}");
    let _ = std::io::stdout().flush();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let _ = handle(stream);
            }
            Err(_) => break,
        }
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        if header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body = String::from_utf8_lossy(&body).to_string();

    let mut parts = request_line.split_whitespace();
    let _method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("").to_string();

    match path.as_str() {
        "/health" => respond_json(&mut stream, 200, r#"{"status":"ok"}"#),
        "/props" => {
            let build = std::env::var("ATOM_STUB_BUILD").unwrap_or_else(|_| "stub-0000".into());
            respond_json(
                &mut stream,
                200,
                &format!(r#"{{"build_info":"{build}","model_path":"stub","n_ctx":4096}}"#),
            )
        }
        "/v1/chat/completions" => {
            if std::env::var("ATOM_STUB_HTTP_ERROR").is_ok() {
                return respond_json(
                    &mut stream,
                    500,
                    r#"{"error":{"message":"stub failure: model unloaded"}}"#,
                );
            }
            if body.contains("\"stream\":true") {
                stream_chat(&mut stream, &body)
            } else {
                complete(&mut stream)
            }
        }
        _ => respond_json(&mut stream, 404, r#"{"error":"not found"}"#),
    }
}

fn respond_json(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        500 => "Internal Server Error",
        _ => "Not Found",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn token_list() -> Vec<String> {
    let count = env_num("ATOM_STUB_TOKENS", 4) as usize;
    (0..count).map(|i| TOKENS[i % TOKENS.len()].to_string()).collect()
}

fn stream_chat(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    let delay = Duration::from_millis(env_num("ATOM_STUB_DELAY_MS", 25));
    let tokens = token_list();
    // The prompt is echoed back only as a token count, like llama-server does.
    let prompt_tokens = body.len() / 4;
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
         Cache-Control: no-cache\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()?;
    // Prefill latency: a real server spends time on the prompt before the first
    // token, and ATOM's TTFT must be measured against that, not assumed zero.
    std::thread::sleep(delay);
    for token in &tokens {
        let payload = serde_escape(token);
        let event = format!(
            "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{payload}\"}}}}]}}\n\n"
        );
        // A client that cancelled has already closed the socket; that is a
        // normal outcome for this test fixture.
        if stream.write_all(event.as_bytes()).is_err() {
            return Ok(());
        }
        stream.flush().ok();
        std::thread::sleep(delay);
    }
    let usage = format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{}}}}],\"usage\":{{\"prompt_tokens\":{prompt_tokens},\
         \"completion_tokens\":{},\"total_tokens\":{}}}}}\n\n",
        tokens.len(),
        tokens.len() + prompt_tokens
    );
    let _ = stream.write_all(usage.as_bytes());
    let _ = stream.write_all(b"data: [DONE]\n\n");
    stream.flush()
}

fn complete(stream: &mut TcpStream) -> std::io::Result<()> {
    let tokens = token_list();
    let content = tokens.concat();
    let body = format!(
        r#"{{"choices":[{{"index":0,"message":{{"role":"assistant","content":"{}"}},"finish_reason":"stop"}}],"usage":{{"prompt_tokens":7,"completion_tokens":{}}},"timings":{{"predicted_per_second":42.5,"prompt_ms":5.0}}}}"#,
        serde_escape(&content),
        tokens.len()
    );
    respond_json(stream, 200, &body)
}

/// Minimal JSON string escaping for the stub's own fixed tokens.
fn serde_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

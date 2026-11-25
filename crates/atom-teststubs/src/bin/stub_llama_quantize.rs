//! Stand-in for `llama-quantize`.
//!
//! Behaviour is selected by environment variables so the same binary can drive
//! every test:
//!
//! * `ATOM_STUB_MODE`  `ok` (default) | `hang` | `fail` | `empty` | `missing`
//! * `ATOM_STUB_GGUF`  file copied to the output path in `ok` mode
//! * `ATOM_STUB_TOTAL` number of tensor progress lines (default 6)
//! * `ATOM_STUB_DELAY_MS` delay between progress lines (default 40)

use std::io::Write;
use std::path::Path;
use std::time::Duration;

fn env_num(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `harness = false` bins are still executed by `cargo test` with no
    // arguments, and some Cargo setups hand them libtest flags. Neither is a
    // quantization run.
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

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("llama-quantize stub (atom test fixture)");
        return;
    }

    // Real argv: `[--allow-requantize] -t N <src> <out> <type>`.
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-t" || arg == "-n" || arg == "-p" {
            i += 2;
            continue;
        }
        if arg.starts_with('-') {
            i += 1;
            continue;
        }
        positional.push(arg.clone());
        i += 1;
    }
    if positional.len() < 2 {
        eprintln!("error: expected <source> <output> [type] arguments");
        std::process::exit(2);
    }
    let output = Path::new(&positional[1]);
    if let Some(parent) = output.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mode = env_str("ATOM_STUB_MODE", "ok");
    let total = env_num("ATOM_STUB_TOTAL", 6).max(1);
    let delay = Duration::from_millis(env_num("ATOM_STUB_DELAY_MS", 40));

    // `hang` and `fail` both leave a partial file behind so cancellation and
    // failure cleanup have something real to remove.
    if mode == "hang" || mode == "fail" {
        std::fs::write(output, b"GGUF-partial-stub").ok();
    }

    for step in 1..=total {
        println!(
            "[{step:>4}/{total}]  blk.{step}.attn_q.weight - [  64,  64,     1,     1], \
             type =    f16, converting to q4_K .. size =     8.00 KiB ->     4.50 KiB"
        );
        let _ = std::io::stdout().flush();
        std::thread::sleep(delay);
    }

    match mode.as_str() {
        "hang" => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        "fail" => {
            eprintln!("error: stub could not quantize tensor blk.3.ffn_down.weight");
            std::process::exit(3);
        }
        "empty" => {
            std::fs::write(output, b"").ok();
        }
        "missing" => {
            // Claim success without producing output.
        }
        _ => match std::env::var("ATOM_STUB_GGUF") {
            Ok(template) if Path::new(&template).is_file() => {
                if let Err(e) = std::fs::copy(&template, output) {
                    eprintln!("error: stub could not write output: {e}");
                    std::process::exit(4);
                }
            }
            _ => {
                std::fs::write(output, b"GGUF").ok();
            }
        },
    }
}

/// Mirrors the part of llama.cpp's help that ATOM parses for supported types.
fn print_help() {
    println!(
        "usage: llama-quantize [--allow-requantize] [--pure] <model-f32.gguf> \
         <model-quantized.gguf> [type]\n\
         \n\
         allowed quantization types:\n\
           2  or  Q4_0    :  4.34G, +0.4685 ppl @ Llama-3-8B\n\
           3  or  Q4_1    :  4.78G, +0.4514 ppl @ Llama-3-8B\n\
           8  or  Q5_0    :  5.21G, +0.1319 ppl @ Llama-3-8B\n\
           9  or  Q5_1    :  5.65G, +0.1148 ppl @ Llama-3-8B\n\
          10  or  Q2_K    :  2.96G, +3.5199 ppl @ Llama-3-8B\n\
          11  or  Q3_K_S  :  3.41G, +1.6321 ppl @ Llama-3-8B\n\
          12  or  Q3_K_M  :  3.74G, +0.6566 ppl @ Llama-3-8B\n\
          13  or  Q3_K_L  :  4.05G, +0.3609 ppl @ Llama-3-8B\n\
          14  or  Q4_K_S  :  4.58G, +0.1623 ppl @ Llama-3-8B\n\
          15  or  Q4_K_M  :  4.84G, +0.1077 ppl @ Llama-3-8B\n\
          16  or  Q5_K_S  :  5.21G, +0.0808 ppl @ Llama-3-8B\n\
          17  or  Q5_K_M  :  5.33G, +0.0596 ppl @ Llama-3-8B\n\
          18  or  Q6_K    :  6.14G, +0.0183 ppl @ Llama-3-8B\n\
           7  or  Q8_0    :  7.96G, +0.0026 ppl @ Llama-3-8B\n\
         \n\
         options: --allow-requantize, --pure, -t N, -p N"
    );
}

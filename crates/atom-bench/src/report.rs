//! Markdown comparison report.

use crate::store::Comparison;
use std::fmt::Write;

fn fmt_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.2} {}", UNITS[unit])
}

fn pct(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{v:+.1}%"),
        None => "n/a".into(),
    }
}

pub fn render_markdown(cmp: &Comparison) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# ATOM comparison report — {}\n", cmp.title);
    let _ = writeln!(out, "Generated {} by ATOM {}\n", cmp.created_at, cmp.environment.atom_version);
    let _ = writeln!(
        out,
        "> Quality numbers are the result of automated, rule-checked prompts. They are evidence of \
         behaviour on this small suite, not proof that two models are equivalent.\n"
    );

    let _ = writeln!(out, "## Environment\n");
    let _ = writeln!(out, "| Field | Value |");
    let _ = writeln!(out, "| --- | --- |");
    let _ = writeln!(out, "| OS | {} {} |", cmp.environment.os, cmp.environment.arch);
    let _ = writeln!(out, "| CPUs | {} |", cmp.environment.cpu_count);
    let _ = writeln!(out, "| System RAM | {} |", fmt_bytes(cmp.environment.total_memory));
    let _ = writeln!(
        out,
        "| Backend | {} |",
        cmp.environment.backend_version.clone().unwrap_or_else(|| "not reported".into())
    );
    let _ = writeln!(out, "| Context | {} tokens |", cmp.settings.context_length);
    let _ = writeln!(out, "| Threads | {} |", cmp.settings.threads);
    let _ = writeln!(
        out,
        "| KV cache | {} / {} |",
        cmp.settings.kv_cache_type_k, cmp.settings.kv_cache_type_v
    );
    let _ = writeln!(
        out,
        "| Sampling | temp {:.2}, top_p {:.2}, seed {} |",
        cmp.settings.temperature,
        cmp.settings.top_p,
        cmp.settings.seed.map(|s| s.to_string()).unwrap_or_else(|| "random".into())
    );

    let _ = writeln!(out, "\n## Model metrics\n");
    let _ = writeln!(out, "| Metric | {}", cmp.runs.iter().map(|r| r.label.clone()).collect::<Vec<_>>().join(" | "));
    let _ = writeln!(
        out,
        "| --- |{}",
        cmp.runs.iter().map(|_| " --- |").collect::<String>()
    );
    let row = |name: &str, values: Vec<String>| {
        format!("| {} | {} |", name, values.join(" | "))
    };
    let _ = writeln!(out, "{}", row("Quantization", cmp.runs.iter().map(|r| r.quant.clone()).collect()));
    let _ = writeln!(out, "{}", row("Kind", cmp.runs.iter().map(|r| r.kind.clone()).collect()));
    let _ = writeln!(out, "{}", row("File size", cmp.runs.iter().map(|r| fmt_bytes(r.file_size)).collect()));
    let _ = writeln!(out, "{}", row("Load time", cmp.runs.iter().map(|r| format!("{} ms", r.load_ms)).collect()));
    let _ = writeln!(
        out,
        "{}",
        row("Peak process RSS", cmp.runs.iter().map(|r| fmt_bytes(r.peak_rss_bytes)).collect())
    );
    let _ = writeln!(
        out,
        "{}",
        row(
            "Tokens/s (avg)",
            cmp.runs.iter().map(|r| format!("{:.2}", r.avg_tokens_per_second)).collect()
        )
    );
    let _ = writeln!(
        out,
        "{}",
        row(
            "Time to first token",
            cmp.runs.iter().map(|r| format!("{:.0} ms", r.avg_ttft_ms)).collect()
        )
    );
    let _ = writeln!(
        out,
        "{}",
        row(
            "Automated quality index",
            cmp.runs.iter().map(|r| format!("{:.1}/100", r.quality_index)).collect()
        )
    );
    let baseline = cmp.runs.first();
    let candidate = cmp.runs.get(1);
    if let (Some(b), Some(c)) = (baseline, candidate) {
        let size_saved = if b.file_size > 0 {
            Some((b.file_size as f64 - c.file_size as f64) / b.file_size as f64 * 100.0)
        } else {
            None
        };
        let speed = if b.avg_tokens_per_second > 0.0 {
            Some((c.avg_tokens_per_second - b.avg_tokens_per_second) / b.avg_tokens_per_second * 100.0)
        } else {
            None
        };
        let _ = writeln!(out, "\n## Deltas ({} → {})\n", b.label, c.label);
        let _ = writeln!(out, "- Disk space saved: **{}**", pct(size_saved));
        let _ = writeln!(out, "- Throughput change: **{}**", pct(speed));
        let _ = writeln!(out, "- Quality index change: **{:+.1} points**", c.quality_index - b.quality_index);
        if let (Some(bc), Some(cc)) = (&b.consistency, &c.consistency) {
            let _ = writeln!(
                out,
                "- Consistency: **{:.0}% → {:.0}%** agreement across {} repeats",
                bc.agreement * 100.0,
                cc.agreement * 100.0,
                cmp.settings.repeats
            );
        }
    }

    let _ = writeln!(out, "\n## Suite results\n");
    for run in &cmp.runs {
        let _ = writeln!(out, "### {}\n", run.label);
        let _ = writeln!(out, "| Suite | Passed | Rate |");
        let _ = writeln!(out, "| --- | --- | --- |");
        for suite in &run.suites {
            let _ = writeln!(
                out,
                "| {} | {}/{} | {:.0}% |",
                suite.title,
                suite.passed,
                suite.total,
                suite.pass_rate * 100.0
            );
        }
        if let Some(c) = &run.consistency {
            let _ = writeln!(out, "\nAgreement: {:.0}% — {}", c.agreement * 100.0, c.detail);
        }
        let _ = writeln!(out, "\n| Case | Result | Detail |");
        let _ = writeln!(out, "| --- | --- | --- |");
        for case in &run.cases {
            let _ = writeln!(
                out,
                "| {} | {} | {} |",
                case.title,
                if case.passed { "pass" } else { "fail" },
                case.detail.replace('|', "\\|")
            );
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## Prompt outputs\n");
    for run in &cmp.runs {
        let _ = writeln!(out, "### {}\n", run.label);
        for case in &run.cases {
            let _ = writeln!(out, "**{}** — {}", case.title, case.prompt);
            let _ = writeln!(out, "\n```\n{}\n```\n", case.output.trim());
        }
    }
    out
}

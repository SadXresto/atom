//! Dataset import, analysis and deterministic train/validation split.
//!
//! Supported inputs are plain text and JSON/JSONL records containing a `text`,
//! `prompt`/`completion`, or `input`/`output` field. Nothing is generated: the
//! statistics are computed from the bytes that were read, and the normalized
//! copy stored in the project directory is what training consumes.

use atom_core::{AtomError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MAX_DATASET_BYTES: u64 = 128 * 1024 * 1024;
pub const MIN_CHARS: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreviewItem {
    pub index: usize,
    pub chars: usize,
    pub excerpt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ByteShare {
    pub byte: u8,
    pub display: String,
    pub count: u64,
    pub share: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatasetStats {
    pub documents: usize,
    pub bytes: u64,
    pub chars: usize,
    pub words: usize,
    pub lines: usize,
    pub avg_document_chars: f64,
    pub max_document_chars: usize,
    pub duplicate_documents: usize,
    pub unique_bytes: usize,
    pub train_chars: usize,
    pub val_chars: usize,
    pub val_ratio: f64,
    pub entropy_bits_per_byte: f64,
    pub top_bytes: Vec<ByteShare>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatasetInfo {
    pub name: String,
    pub source_path: String,
    pub stored_path: String,
    pub format: String,
    pub imported_at: String,
    pub split_seed: u64,
    pub stats: DatasetStats,
    pub preview: Vec<PreviewItem>,
}

fn display_byte(b: u8) -> String {
    match b {
        b'\n' => "\\n".into(),
        b'\t' => "\\t".into(),
        b'\r' => "\\r".into(),
        32..=126 => (b as char).to_string(),
        _ => format!("0x{b:02x}"),
    }
}

fn excerpt(text: &str, max: usize) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if out.chars().count() >= max {
            out.push('…');
            break;
        }
        if ch == '\n' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

/// Split raw input into documents, preserving order.
pub fn parse_documents(raw: &str, format: &str) -> Result<Vec<String>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AtomError::Invalid("the dataset file is empty".into()));
    }
    match format {
        "jsonl" => {
            let mut docs = Vec::new();
            for (i, line) in raw.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let value: serde_json::Value = serde_json::from_str(line).map_err(|e| {
                    AtomError::Invalid(format!("line {} is not valid JSON: {e}", i + 1))
                })?;
                if let Some(text) = record_text(&value) {
                    docs.push(text);
                } else {
                    return Err(AtomError::Invalid(format!(
                        "line {} has no text, prompt/completion or input/output field",
                        i + 1
                    )));
                }
            }
            Ok(docs)
        }
        "json" => {
            let value: serde_json::Value = serde_json::from_str(trimmed)
                .map_err(|e| AtomError::Invalid(format!("not valid JSON: {e}")))?;
            let items = match value {
                serde_json::Value::Array(items) => items,
                serde_json::Value::Object(_) => vec![value],
                _ => {
                    return Err(AtomError::Invalid(
                        "JSON datasets must be an array of objects or a single object".into(),
                    ))
                }
            };
            let mut docs = Vec::new();
            for (i, item) in items.into_iter().enumerate() {
                let text = record_text(&item).ok_or_else(|| {
                    AtomError::Invalid(format!(
                        "record {} has no text, prompt/completion or input/output field",
                        i + 1
                    ))
                })?;
                docs.push(text);
            }
            Ok(docs)
        }
        _ => {
            // Plain text: one document per blank-line separated block.
            let blocks: Vec<String> = raw
                .split("\n\n")
                .map(|b| b.trim().to_string())
                .filter(|b| !b.is_empty())
                .collect();
            if blocks.is_empty() {
                Ok(vec![trimmed.to_string()])
            } else {
                Ok(blocks)
            }
        }
    }
}

fn record_text(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
        if !text.trim().is_empty() {
            return Some(text.to_string());
        }
    }
    let prompt = value
        .get("prompt")
        .or_else(|| value.get("instruction"))
        .or_else(|| value.get("input"))
        .and_then(|v| v.as_str());
    let completion = value
        .get("completion")
        .or_else(|| value.get("output"))
        .or_else(|| value.get("response"))
        .and_then(|v| v.as_str());
    match (prompt, completion) {
        (Some(p), Some(c)) => Some(format!("{}\n{}", p.trim(), c.trim())),
        (Some(p), None) => Some(p.to_string()),
        (None, Some(c)) => Some(c.to_string()),
        _ => None,
    }
}

pub fn detect_format(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("jsonl") | Some("ndjson") => "jsonl",
        Some("json") => "json",
        _ => "text",
    }
}

/// Split documents into training and validation text. The split is by document
/// and deterministic for a given seed, so a re-import with the same seed and
/// file produces the same split.
pub fn split_documents(docs: &[String], seed: u64, val_ratio: f64) -> (String, String) {
    // A single plain-text document (no blank-line blocks) still needs a
    // validation split, so cut it at a character boundary instead of returning
    // an empty split.
    if docs.len() == 1 {
        let text = &docs[0];
        let chars = text.chars().count();
        let val_chars = ((chars as f64 * val_ratio.clamp(0.05, 0.4)).round() as usize).max(1);
        let train_chars = chars.saturating_sub(val_chars);
        let cut = text
            .char_indices()
            .nth(train_chars)
            .map(|(index, _)| index)
            .unwrap_or(text.len());
        let (train, val) = text.split_at(cut);
        return (format!("{train}\n"), val.to_string());
    }
    let mut order: Vec<usize> = (0..docs.len()).collect();
    // xorshift shuffle
    let mut state = seed | 1;
    for i in (1..order.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    let val_count = ((docs.len() as f64 * val_ratio).round() as usize)
        .min(docs.len().saturating_sub(1))
        .max(usize::from(docs.len() > 1));
    let mut train = String::new();
    let mut val = String::new();
    for (position, index) in order.iter().enumerate() {
        let target = if position < val_count { &mut val } else { &mut train };
        target.push_str(&docs[*index]);
        target.push('\n');
    }
    (train, val)
}

pub fn analyze(docs: &[String], train: &str, val: &str, val_ratio: f64) -> DatasetStats {
    let mut byte_counts = BTreeMap::<u8, u64>::new();
    let mut total_bytes = 0u64;
    for doc in docs {
        for b in doc.bytes() {
            *byte_counts.entry(b).or_insert(0) += 1;
            total_bytes += 1;
        }
    }
    let chars: usize = docs.iter().map(|d| d.chars().count()).sum();
    let words: usize = docs.iter().map(|d| d.split_whitespace().count()).sum();
    let lines = docs.iter().map(|d| d.lines().count()).sum();
    let mut unique = std::collections::HashSet::new();
    let mut duplicates = 0usize;
    for doc in docs {
        if !unique.insert(doc.clone()) {
            duplicates += 1;
        }
    }
    let entropy = if total_bytes > 0 {
        byte_counts
            .values()
            .map(|count| {
                let p = *count as f64 / total_bytes as f64;
                -p * p.log2()
            })
            .sum()
    } else {
        0.0
    };
    let mut top: Vec<ByteShare> = byte_counts
        .iter()
        .map(|(b, count)| ByteShare {
            byte: *b,
            display: display_byte(*b),
            count: *count,
            share: if total_bytes > 0 {
                *count as f64 / total_bytes as f64
            } else {
                0.0
            },
        })
        .collect();
    top.sort_by(|a, b| b.count.cmp(&a.count));
    top.truncate(12);
    DatasetStats {
        documents: docs.len(),
        bytes: total_bytes,
        chars,
        words,
        lines,
        avg_document_chars: if docs.is_empty() {
            0.0
        } else {
            chars as f64 / docs.len() as f64
        },
        max_document_chars: docs.iter().map(|d| d.chars().count()).max().unwrap_or(0),
        duplicate_documents: duplicates,
        unique_bytes: byte_counts.len(),
        train_chars: train.chars().count(),
        val_chars: val.chars().count(),
        val_ratio,
        entropy_bits_per_byte: entropy,
        top_bytes: top,
    }
}

pub struct ImportedDataset {
    pub info: DatasetInfo,
    pub train: String,
    pub val: String,
}

/// Read `source`, validate it, write a normalized copy into `project_dir`, and
/// return the analysed dataset. The source file is never modified.
pub fn import(source: &Path, project_dir: &Path, name: &str, seed: u64, val_ratio: f64) -> Result<ImportedDataset> {
    let meta = std::fs::metadata(source)?;
    if !meta.is_file() {
        return Err(AtomError::Invalid(format!(
            "{} is not a file",
            source.display()
        )));
    }
    if meta.len() > MAX_DATASET_BYTES {
        return Err(AtomError::Invalid(format!(
            "dataset is {} bytes; the limit is {} bytes",
            meta.len(),
            MAX_DATASET_BYTES
        )));
    }
    let raw = std::fs::read_to_string(source).map_err(|e| {
        AtomError::Invalid(format!(
            "dataset must be UTF-8 text ({}): {e}",
            source.display()
        ))
    })?;
    let format = detect_format(source);
    let docs = parse_documents(&raw, format)?;
    let total_chars: usize = docs.iter().map(|d| d.chars().count()).sum();
    if total_chars < MIN_CHARS {
        return Err(AtomError::Invalid(format!(
            "dataset has {total_chars} characters; at least {MIN_CHARS} are required to train"
        )));
    }
    let (train, val) = split_documents(&docs, seed, val_ratio.clamp(0.05, 0.4));
    if val.trim().is_empty() {
        return Err(AtomError::Invalid(
            "the validation split is empty; add more documents".into(),
        ));
    }
    let stats = analyze(&docs, &train, &val, val_ratio);
    std::fs::create_dir_all(project_dir)?;
    let stored = project_dir.join("dataset.txt");
    std::fs::write(&stored, &train)?;
    std::fs::write(project_dir.join("dataset-val.txt"), &val)?;
    std::fs::write(project_dir.join("dataset.jsonl"), docs.join("\n"))?;
    let preview = docs
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, doc)| PreviewItem {
            index,
            chars: doc.chars().count(),
            excerpt: excerpt(doc, 280),
        })
        .collect();
    let info = DatasetInfo {
        name: name.to_string(),
        source_path: source.display().to_string(),
        stored_path: stored.display().to_string(),
        format: format.to_string(),
        imported_at: chrono::Local::now().to_rfc3339(),
        split_seed: seed,
        stats,
        preview,
    };
    Ok(ImportedDataset { info, train, val })
}

/// Compose teacher completions (distillation corpus) into a dataset file.
pub fn write_text_dataset(dir: &Path, docs: &[String], name: &str) -> Result<DatasetInfo> {
    let source_label = format!("distillation:{name}");
    let mut raw = String::new();
    for (i, doc) in docs.iter().enumerate() {
        if i > 0 {
            raw.push_str("\n\n");
        }
        raw.push_str(doc.trim());
    }
    let docs = parse_documents(&raw, "text")?;
    let total_chars: usize = docs.iter().map(|d| d.chars().count()).sum();
    if total_chars < MIN_CHARS {
        return Err(AtomError::Invalid(format!(
            "the teacher produced only {total_chars} characters; at least {MIN_CHARS} are required"
        )));
    }
    let (train, val) = split_documents(&docs, 42, 0.15);
    let stats = analyze(&docs, &train, &val, 0.15);
    std::fs::create_dir_all(dir)?;
    let stored = dir.join("dataset-distill.txt");
    std::fs::write(&stored, &train)?;
    std::fs::write(dir.join("dataset-distill-val.txt"), &val)?;
    let preview = docs
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, doc)| PreviewItem {
            index,
            chars: doc.chars().count(),
            excerpt: excerpt(doc, 280),
        })
        .collect();
    Ok(DatasetInfo {
        name: format!("Teacher corpus — {name}"),
        source_path: source_label,
        stored_path: stored.display().to_string(),
        format: "teacher".into(),
        imported_at: chrono::Local::now().to_rfc3339(),
        split_seed: 42,
        stats,
        preview,
    })
}

pub fn dataset_files(project_dir: &Path) -> (PathBuf, PathBuf) {
    (project_dir.join("dataset.txt"), project_dir.join("dataset-val.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonl_and_plain_text_are_parsed() {
        let docs = parse_documents(
            "{\"text\":\"alpha beta\"}\n{\"prompt\":\"hi\",\"completion\":\"there\"}",
            "jsonl",
        )
        .unwrap();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[1], "hi\nthere");

        let docs = parse_documents("first block\n\nsecond block", "text").unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn invalid_jsonl_reports_line_number() {
        let err = parse_documents("{\"text\":\"ok\"}\nnot json", "jsonl").unwrap_err();
        assert!(err.to_string().contains("line 2"));
    }

    #[test]
    fn split_is_deterministic_and_never_empty() {
        let docs: Vec<String> = (0..40).map(|i| format!("document number {i} with text")).collect();
        let (a, av) = split_documents(&docs, 7, 0.2);
        let (b, bv) = split_documents(&docs, 7, 0.2);
        assert_eq!(a, b);
        assert_eq!(av, bv);
        assert!(!av.trim().is_empty());
        assert!(!a.trim().is_empty());
    }

    #[test]
    fn single_document_splits_by_characters() {
        let docs = vec!["abcdefghij".repeat(40)];
        let (train, val) = split_documents(&docs, 1, 0.2);
        assert!(train.chars().count() > 300);
        assert!(val.chars().count() >= 40);
    }

    #[test]
    fn stats_count_duplicates_and_entropy() {
        let docs = vec!["aaa".to_string(), "aaa".to_string(), "bbb".to_string()];
        let stats = analyze(&docs, "aaa\nbbb\n", "aaa\n", 0.33);
        assert_eq!(stats.documents, 3);
        assert_eq!(stats.duplicate_documents, 1);
        assert!(stats.entropy_bits_per_byte > 0.0);
    }

    #[test]
    fn import_writes_normalized_copy() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("data.txt");
        std::fs::write(&src, "One two three.\n\nFour five six.\n\nSeven eight nine.".repeat(8)).unwrap();
        let out = import(&src, &dir.path().join("proj"), "demo", 1, 0.2).unwrap();
        assert!(out.info.stats.documents >= 3);
        assert!(Path::new(&out.info.stored_path).exists());
    }
}

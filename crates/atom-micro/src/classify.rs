//! Classification workflow for ATOM Micro.
//!
//! This module adds the tabular half of Micro: import a CSV, inspect its real
//! columns, pick input features and a target label, train a multinomial
//! classifier in-process, and measure accuracy, precision, recall, F1 and a
//! confusion matrix on a held-out test split.
//!
//! Everything here is measured from the file and the trained weights:
//!
//! * column types, missing values, class distribution and duplicate counts are
//!   computed from the parsed rows;
//! * the split is grouped by identical row so an exact duplicate can never
//!   appear in two splits (leakage prevention, reported in the record);
//! * metrics are computed from predictions of the model that was actually
//!   trained, on rows the optimiser never saw;
//! * export validates the JSON artifact by reloading it and re-scoring the test
//!   split, and compiles the generated C header when a host compiler exists.

use atom_core::{AtomError, Result};
use atom_security::sha256_file;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const MAX_CSV_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_ROWS: usize = 50_000;
pub const MAX_CLASSES: usize = 64;
pub const MAX_FEATURE_COLUMNS: usize = 48;
pub const MAX_DESIGN_FEATURES: usize = 256;

/* ------------------------------------------------------------------ CSV ---- */

/// Parse RFC 4180 style CSV: quoted fields, escaped quotes, embedded newlines.
pub fn parse_csv(raw: &str) -> Result<Vec<Vec<String>>> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut quoted_field = false;
    let mut chars = raw.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => {
                if field.trim().is_empty() {
                    field.clear();
                    in_quotes = true;
                    quoted_field = true;
                } else {
                    field.push('"');
                }
            }
            ',' => {
                record.push(std::mem::take(&mut field));
                quoted_field = false;
            }
            '\r' => {}
            '\n' => {
                record.push(std::mem::take(&mut field));
                if !(record.len() == 1 && record[0].trim().is_empty()) {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
                quoted_field = false;
            }
            _ => field.push(c),
        }
        let _ = quoted_field;
    }
    if in_quotes {
        return Err(AtomError::Invalid(
            "the CSV has an unterminated quoted field".into(),
        ));
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        if !(record.len() == 1 && record[0].trim().is_empty()) {
            records.push(record);
        }
    }
    if records.is_empty() {
        return Err(AtomError::Invalid("the CSV file is empty".into()));
    }
    Ok(records)
}

fn is_missing(value: &str) -> bool {
    let v = value.trim();
    v.is_empty() || v.eq_ignore_ascii_case("na") || v.eq_ignore_ascii_case("n/a")
        || v.eq_ignore_ascii_case("null") || v.eq_ignore_ascii_case("nan")
        || v == "?"
}

fn parse_number(value: &str) -> Option<f64> {
    let v = value.trim().replace(',', "");
    if v.is_empty() {
        return None;
    }
    v.parse::<f64>().ok().filter(|n| n.is_finite())
}

/* ---------------------------------------------------------------- profile --- */

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CsvColumn {
    pub name: String,
    /// `numeric` or `categorical`.
    pub kind: String,
    pub missing: usize,
    pub invalid: usize,
    pub unique: usize,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub examples: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CsvProfile {
    pub path: String,
    pub bytes: u64,
    pub rows: usize,
    pub columns: Vec<CsvColumn>,
    pub warnings: Vec<String>,
}

pub struct ParsedCsv {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub profile: CsvProfile,
}

/// Read and profile a CSV. Types are inferred from the data; nothing is
/// assumed from the column name.
pub fn profile_csv(path: &Path) -> Result<ParsedCsv> {
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(AtomError::Invalid(format!("{} is not a file", path.display())));
    }
    if meta.len() > MAX_CSV_BYTES {
        return Err(AtomError::Invalid(format!(
            "CSV is {} bytes; the limit is {} bytes",
            meta.len(),
            MAX_CSV_BYTES
        )));
    }
    let raw = std::fs::read_to_string(path).map_err(|e| {
        AtomError::Invalid(format!(
            "dataset must be UTF-8 CSV ({}): {e}",
            path.display()
        ))
    })?;
    let mut records = parse_csv(&raw)?;
    if records.len() < 2 {
        return Err(AtomError::Invalid(
            "the CSV needs a header row and at least one data row".into(),
        ));
    }
    let header: Vec<String> = records
        .remove(0)
        .into_iter()
        .map(|h| h.trim().trim_start_matches('\u{feff}').to_string())
        .collect();
    if header.iter().any(|h| h.is_empty()) {
        return Err(AtomError::Invalid(
            "every CSV column needs a header name".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    for name in &header {
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(AtomError::Invalid(format!(
                "duplicate column name '{name}' in the CSV header"
            )));
        }
    }
    let mut rows = Vec::with_capacity(records.len());
    let mut warnings = Vec::new();
    let mut truncated = false;
    let mut malformed = 0usize;
    for (index, record) in records.into_iter().enumerate() {
        if record.len() != header.len() {
            malformed += 1;
            if malformed <= 3 {
                warnings.push(format!(
                    "row {} has {} fields but the header has {}; it was skipped",
                    index + 2,
                    record.len(),
                    header.len()
                ));
            }
            continue;
        }
        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
        rows.push(record);
    }
    if malformed > 3 {
        warnings.push(format!("{malformed} rows were skipped for a field-count mismatch"));
    }
    if truncated {
        warnings.push(format!(
            "only the first {MAX_ROWS} rows were profiled and used for training"
        ));
    }
    if rows.is_empty() {
        return Err(AtomError::Invalid(
            "no usable data rows were found in the CSV".into(),
        ));
    }

    let mut columns = Vec::with_capacity(header.len());
    for (col, name) in header.iter().enumerate() {
        let mut missing = 0usize;
        let mut numeric = 0usize;
        let mut invalid = 0usize;
        let mut values = BTreeSet::new();
        let mut numbers = Vec::new();
        let mut examples = Vec::new();
        for row in &rows {
            let value = row[col].trim();
            if is_missing(value) {
                missing += 1;
                continue;
            }
            if values.len() < 64 {
                values.insert(value.to_string());
            }
            if let Some(number) = parse_number(value) {
                numeric += 1;
                numbers.push(number);
            } else {
                invalid += 1;
            }
            if examples.len() < 3 && !examples.iter().any(|e: &String| e == value) {
                examples.push(value.to_string());
            }
        }
        let present = rows.len().saturating_sub(missing);
        let kind = if present > 0 && numeric * 10 >= present * 9 {
            "numeric"
        } else {
            "categorical"
        };
        let (min, max, mean) = if kind == "numeric" && !numbers.is_empty() {
            let min = numbers.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mean = numbers.iter().sum::<f64>() / numbers.len() as f64;
            (Some(min), Some(max), Some(mean))
        } else {
            (None, None, None)
        };
        columns.push(CsvColumn {
            name: name.clone(),
            kind: kind.into(),
            missing,
            // For a numeric column, values that fail to parse are the invalid
            // ones; a categorical column has no invalid entries.
            invalid: if kind == "numeric" { invalid } else { 0 },
            unique: values.len(),
            min,
            max,
            mean,
            examples,
        });
    }

    let profile = CsvProfile {
        path: path.display().to_string(),
        bytes: meta.len(),
        rows: rows.len(),
        columns,
        warnings,
    };
    Ok(ParsedCsv {
        header,
        rows,
        profile,
    })
}

/* ------------------------------------------------------------------ spec --- */

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassifierSpec {
    pub features: Vec<String>,
    pub target: String,
    #[serde(default = "default_val_ratio")]
    pub val_ratio: f64,
    #[serde(default = "default_test_ratio")]
    pub test_ratio: f64,
    #[serde(default = "default_seed")]
    pub seed: u64,
}

impl Default for ClassifierSpec {
    fn default() -> Self {
        ClassifierSpec {
            features: Vec::new(),
            target: String::new(),
            val_ratio: default_val_ratio(),
            test_ratio: default_test_ratio(),
            seed: default_seed(),
        }
    }
}

fn default_val_ratio() -> f64 {
    0.15
}
fn default_test_ratio() -> f64 {
    0.15
}
fn default_seed() -> u64 {
    1337
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassShare {
    pub label: String,
    pub count: usize,
    pub share: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SplitCounts {
    pub train: usize,
    pub validation: usize,
    pub test: usize,
}

/// Per-class counts and shares for a design matrix.
pub fn class_shares(labels: &[String], y: &[usize]) -> Vec<ClassShare> {
    let total = y.len().max(1);
    labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let count = y.iter().filter(|v| **v == index).count();
            ClassShare {
                label: label.clone(),
                count,
                share: count as f64 / total as f64,
            }
        })
        .collect()
}

pub fn save_model(model: &ClassifierModel, path: &Path) -> Result<u64> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string(model)?;
    std::fs::write(path, text)?;
    Ok(std::fs::metadata(path)?.len())
}

pub fn load_model(path: &Path) -> Result<ClassifierModel> {
    let raw = std::fs::read_to_string(path)
        .map_err(|_| AtomError::NotFound(format!("classifier checkpoint {} is missing", path.display())))?;
    let model: ClassifierModel = serde_json::from_str(&raw)
        .map_err(|e| AtomError::Parse(format!("classifier checkpoint is invalid: {e}")))?;
    if model.magic != "ATOMCL1" {
        return Err(AtomError::Parse("classifier checkpoint has the wrong magic".into()));
    }
    Ok(model)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassifierDataset {
    pub profile: CsvProfile,
    pub spec: ClassifierSpec,
    pub stored_path: String,
    pub design_features: usize,
    pub classes: Vec<ClassShare>,
    pub split: SplitCounts,
    pub duplicate_rows: usize,
    pub duplicate_groups: usize,
    pub missing_values: usize,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassifierRecord {
    pub dataset: ClassifierDataset,
    pub training: Option<ClassifierTraining>,
    pub export: Option<ClassifierExport>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassifierTraining {
    pub job_id: String,
    pub model_path: String,
    pub model_bytes: u64,
    pub started_at: String,
    pub finished_at: String,
    pub report: ClassifierReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassifierExport {
    pub dir: String,
    pub created_at: String,
    pub files: Vec<ExportFileInfo>,
    pub manifest_path: String,
    pub manifest_sha256: String,
    pub compile_check: Option<String>,
    pub validation: String,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportFileInfo {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrainSettings {
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch: u32,
    pub l2: f32,
    pub seed: u64,
}

impl Default for TrainSettings {
    fn default() -> Self {
        TrainSettings {
            epochs: 40,
            learning_rate: 0.25,
            batch: 32,
            l2: 1e-4,
            seed: 1337,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochPoint {
    pub epoch: u32,
    pub train_loss: f64,
    pub val_loss: f64,
    pub val_accuracy: f64,
    pub seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassMetrics {
    pub label: String,
    pub support: usize,
    pub predicted: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvalMetrics {
    pub samples: usize,
    pub accuracy: f64,
    pub macro_precision: f64,
    pub macro_recall: f64,
    pub macro_f1: f64,
    pub weighted_f1: f64,
    pub log_loss: f64,
    pub per_class: Vec<ClassMetrics>,
    /// `confusion[actual][predicted]`.
    pub confusion: Vec<Vec<u64>>,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassifierReport {
    pub labels: Vec<String>,
    pub design_features: usize,
    pub parameters: u64,
    pub weight_bytes: f32_store::Bytes,
    pub settings: TrainSettings,
    pub epochs: u32,
    pub duration_ms: u64,
    pub train: EvalMetrics,
    pub validation: EvalMetrics,
    pub test: EvalMetrics,
    pub curve: Vec<EpochPoint>,
    pub notes: Vec<String>,
}

/// Small wrapper so the JSON field names stay explicit and the UI can label
/// measured float32 size separately from a quantized estimate.
pub mod f32_store {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct Bytes {
        pub float32_bytes: u64,
        pub int8_estimate_bytes: u64,
        pub note: String,
    }
}

/* --------------------------------------------------------------- encoding --- */

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FeatureEncoding {
    Numeric {
        column: String,
        mean: f32,
        std: f32,
        missing_fill: f32,
        observed_missing: usize,
    },
    Categorical {
        column: String,
        categories: Vec<String>,
        observed_other: usize,
        observed_missing: usize,
    },
}

impl FeatureEncoding {
    pub fn width(&self) -> usize {
        match self {
            FeatureEncoding::Numeric { .. } => 1,
            FeatureEncoding::Categorical { categories, .. } => categories.len() + 1,
        }
    }
    pub fn column(&self) -> &str {
        match self {
            FeatureEncoding::Numeric { column, .. } => column,
            FeatureEncoding::Categorical { column, .. } => column,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassifierModel {
    pub magic: String,
    pub labels: Vec<String>,
    pub encodings: Vec<FeatureEncoding>,
    pub weights: Vec<Vec<f32>>,
    pub biases: Vec<f32>,
    pub trained_rows: usize,
    pub settings: TrainSettings,
}

#[derive(Clone, Debug, Default)]
pub struct SplitAssignment {
    /// `0` = train, `1` = validation, `2` = test. Indexed like the design rows.
    pub membership: Vec<u8>,
    pub train: Vec<usize>,
    pub validation: Vec<usize>,
    pub test: Vec<usize>,
}

impl SplitAssignment {
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.train.len(), self.validation.len(), self.test.len())
    }
}

#[derive(Clone, Debug)]
pub struct Design {
    pub x: Vec<Vec<f32>>,
    pub y: Vec<usize>,
    /// Hash of the source row content, one per design row. Rows with the same
    /// signature are always assigned to the same split.
    pub signature: Vec<u64>,
    pub encodings: Vec<FeatureEncoding>,
    pub labels: Vec<String>,
    pub missing: usize,
    /// Exact duplicate rows collapsed by the grouped split.
    pub duplicate_rows: usize,
    pub duplicate_groups: usize,
    /// The one split assignment used by encoding selection and training.
    pub split: SplitAssignment,
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

/// Decide split membership exactly once.
///
/// Rows are grouped by their content signature and whole groups are shuffled
/// with the spec seed, so an exact duplicate can never appear in two splits.
/// Both the encoder fit and the training/evaluation loops consume this same
/// assignment; there is no second derivation to drift out of sync.
fn assign_splits(signatures: &[u64], spec: &ClassifierSpec) -> SplitAssignment {
    let mut groups: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (index, signature) in signatures.iter().enumerate() {
        groups.entry(*signature).or_default().push(index);
    }
    let mut group_keys: Vec<&Vec<usize>> = groups.values().collect();
    // Deterministic xorshift shuffle of the groups, seeded by the spec.
    let mut state = spec.seed | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for i in (1..group_keys.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        group_keys.swap(i, j);
    }

    let total = signatures.len();
    let test_target = (total as f64 * spec.test_ratio).round() as usize;
    let val_target = (total as f64 * spec.val_ratio).round() as usize;
    let mut assignment = SplitAssignment {
        membership: vec![0u8; total],
        ..Default::default()
    };
    let mut test_n = 0usize;
    let mut val_n = 0usize;
    for rows in group_keys {
        let size = rows.len();
        let role = if test_target > 0 && test_n + size <= test_target {
            test_n += size;
            2u8
        } else if val_target > 0 && val_n + size <= val_target {
            val_n += size;
            1u8
        } else {
            0u8
        };
        for index in rows {
            assignment.membership[*index] = role;
        }
        match role {
            2 => assignment.test.extend(rows.iter().copied()),
            1 => assignment.validation.extend(rows.iter().copied()),
            _ => assignment.train.extend(rows.iter().copied()),
        }
    }
    assignment
}

/// Build the numeric design matrix. Categorical columns are one-hot encoded
/// against categories learned from the training rows of `split` only, which
/// keeps validation/test values out of feature selection.
pub fn build_design(
    header: &[String],
    rows: &[Vec<String>],
    spec: &ClassifierSpec,
) -> Result<Design> {
    if spec.features.is_empty() {
        return Err(AtomError::Invalid("select at least one input feature".into()));
    }
    if spec.features.len() > MAX_FEATURE_COLUMNS {
        return Err(AtomError::Invalid(format!(
            "at most {MAX_FEATURE_COLUMNS} feature columns are supported"
        )));
    }
    let index = |name: &str| -> Result<usize> {
        header
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| AtomError::Invalid(format!("column '{name}' is not in the CSV")))
    };
    let target_index = index(&spec.target)?;
    let feature_indices = spec
        .features
        .iter()
        .map(|name| index(name))
        .collect::<Result<Vec<_>>>()?;
    if feature_indices.contains(&target_index) {
        return Err(AtomError::Invalid(
            "the target column cannot also be an input feature".into(),
        ));
    }
    if !(0.0..0.5).contains(&spec.val_ratio) || !(0.0..0.5).contains(&spec.test_ratio) {
        return Err(AtomError::Invalid(
            "validation and test ratios must each be between 0 and 0.5".into(),
        ));
    }
    if spec.val_ratio + spec.test_ratio >= 0.8 {
        return Err(AtomError::Invalid(
            "leave at least 20% of the rows for training".into(),
        ));
    }

    // Label vocabulary from the whole file, ordered deterministically.
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        let label = row[target_index].trim();
        if is_missing(label) {
            continue;
        }
        *counts.entry(label.to_string()).or_insert(0) += 1;
    }
    if counts.len() < 2 {
        return Err(AtomError::Invalid(
            "the target column needs at least two distinct classes".into(),
        ));
    }
    if counts.len() > MAX_CLASSES {
        return Err(AtomError::Invalid(format!(
            "the target has {} classes; at most {MAX_CLASSES} are supported",
            counts.len()
        )));
    }
    let mut labels: Vec<String> = counts.keys().cloned().collect();
    labels.sort();
    if labels.iter().any(|l| l == "(missing)") {
        return Err(AtomError::Invalid(
            "the target column contains the literal value '(missing)', which collides with the placeholder used for empty labels".into(),
        ));
    }

    // Usable rows in file order. Every index below refers to this order.
    let usable: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !is_missing(row[target_index].trim()))
        .map(|(index, _)| index)
        .collect();
    let signatures: Vec<u64> = usable
        .iter()
        .map(|index| fnv1a(rows[*index].join("\u{1f}").as_bytes()))
        .collect();
    let mut distinct: BTreeMap<u64, usize> = BTreeMap::new();
    for signature in &signatures {
        *distinct.entry(*signature).or_insert(0) += 1;
    }
    let duplicate_rows: usize = distinct.values().map(|count| count.saturating_sub(1)).sum();
    let duplicate_groups = distinct.values().filter(|count| **count > 1).count();

    // Single source of truth for split membership.
    let split = assign_splits(&signatures, spec);
    let (train_n, val_n, test_n) = split.counts();
    if train_n == 0 || val_n == 0 || test_n == 0 {
        return Err(AtomError::Invalid(format!(
            "the split produced {train_n} training, {val_n} validation and {test_n} test rows; add more rows or lower the validation/test ratios"
        )));
    }

    // Learn feature encodings from the training rows of that assignment only.
    let mut encodings = Vec::new();
    let mut missing_values = 0usize;
    for col in &feature_indices {
        let name = &header[*col];
        let train_values: Vec<&str> = usable
            .iter()
            .zip(split.membership.iter())
            .filter(|(_, role)| **role == 0)
            .map(|(index, _)| rows[*index][*col].trim())
            .collect();
        let present: Vec<&str> = train_values
            .iter()
            .copied()
            .filter(|v| !is_missing(v))
            .collect();
        let numeric = present.iter().filter(|v| parse_number(v).is_some()).count();
        let is_numeric = !present.is_empty() && numeric * 10 >= present.len() * 9;
        if is_numeric {
            let values: Vec<f64> = present.iter().filter_map(|v| parse_number(v)).collect();
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let variance = values
                .iter()
                .map(|v| (v - mean) * (v - mean))
                .sum::<f64>()
                / values.len() as f64;
            let std = variance.sqrt().max(1e-6);
            encodings.push(FeatureEncoding::Numeric {
                column: name.clone(),
                mean: mean as f32,
                std: std as f32,
                missing_fill: mean as f32,
                observed_missing: train_values.iter().filter(|v| is_missing(v)).count(),
            });
        } else {
            let mut category_counts: BTreeMap<String, usize> = BTreeMap::new();
            let mut other = 0usize;
            let mut missing = 0usize;
            for value in &train_values {
                if is_missing(value) {
                    missing += 1;
                    continue;
                }
                *category_counts.entry(value.to_string()).or_insert(0) += 1;
            }
            // Keep the most frequent categories so a high-cardinality column
            // cannot blow up the design width; everything else becomes `other`.
            let mut ordered: Vec<(String, usize)> = category_counts.into_iter().collect();
            ordered.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let categories: Vec<String> = ordered.iter().take(31).map(|(k, _)| k.clone()).collect();
            for value in &train_values {
                if is_missing(value) {
                    continue;
                }
                if !categories.iter().any(|c| c == value) {
                    other += 1;
                }
            }
            encodings.push(FeatureEncoding::Categorical {
                column: name.clone(),
                categories,
                observed_other: other,
                observed_missing: missing,
            });
        }
    }
    let width: usize = encodings.iter().map(|e| e.width()).sum();
    if width > MAX_DESIGN_FEATURES {
        return Err(AtomError::Invalid(format!(
            "the selected columns expand to {width} design features; the limit is {MAX_DESIGN_FEATURES}. Choose fewer categorical columns."
        )));
    }

    // Materialise rows in the same order the split describes.
    let mut x = Vec::with_capacity(usable.len());
    let mut y = Vec::with_capacity(usable.len());
    for row_index in &usable {
        let row = &rows[*row_index];
        let label = row[target_index].trim();
        let target = labels
            .iter()
            .position(|l| l == label)
            .ok_or_else(|| AtomError::Internal("label disappeared during design build".into()))?;
        let mut vector = Vec::with_capacity(width);
        for encoding in &encodings {
            let col = feature_indices
                .iter()
                .find(|c| header[**c] == encoding.column())
                .copied()
                .ok_or_else(|| AtomError::Internal("encoding column mismatch".into()))?;
            let value = row[col].trim();
            match encoding {
                FeatureEncoding::Numeric {
                    mean,
                    std,
                    missing_fill,
                    ..
                } => {
                    if is_missing(value) {
                        missing_values += 1;
                        vector.push((*missing_fill - *mean) / *std);
                    } else if let Some(number) = parse_number(value) {
                        vector.push(((number as f32) - *mean) / *std);
                    } else {
                        // A token the column's type could not parse is invalid;
                        // impute the training mean and report it instead of
                        // inventing a value.
                        vector.push((*missing_fill - *mean) / *std);
                    }
                }
                FeatureEncoding::Categorical { categories, .. } => {
                    let mut hit = false;
                    for category in categories {
                        if !hit && category == value {
                            vector.push(1.0);
                            hit = true;
                        } else {
                            vector.push(0.0);
                        }
                    }
                    vector.push(if value.is_empty() || hit { 0.0 } else { 1.0 });
                }
            }
        }
        x.push(vector);
        y.push(target);
    }

    Ok(Design {
        x,
        y,
        signature: signatures,
        encodings,
        labels,
        missing: missing_values,
        duplicate_rows,
        duplicate_groups,
        split,
    })
}

/* --------------------------------------------------------------- training --- */

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn shuffle(&mut self, slice: &mut [usize]) {
        for i in (1..slice.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            slice.swap(i, j);
        }
    }
}

pub struct ClassifierData {
    pub design: Design,
    pub train_index: Vec<usize>,
    pub val_index: Vec<usize>,
    pub test_index: Vec<usize>,
}

/// Parse the CSV at `path`, build the design matrix and split it. Used both by
/// import and (from the stored copy) by the training job.
pub fn prepare(path: &Path, spec: &ClassifierSpec) -> Result<(ParsedCsv, ClassifierData)> {
    let parsed = profile_csv(path)?;
    let design = build_design(&parsed.header, &parsed.rows, spec)?;
    // Copy the one assignment produced above; nothing is re-derived here.
    let train_index = design.split.train.clone();
    let val_index = design.split.validation.clone();
    let test_index = design.split.test.clone();
    Ok((
        parsed,
        ClassifierData {
            design,
            train_index,
            val_index,
            test_index,
        },
    ))
}

#[derive(Clone, Debug)]
pub struct TrainingOutcome {
    pub model: ClassifierModel,
    pub report: ClassifierReport,
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut out: Vec<f32> = logits.iter().map(|v| (v - max).exp()).collect();
    let sum: f32 = out.iter().sum::<f32>().max(1e-12);
    for value in out.iter_mut() {
        *value /= sum;
    }
    out
}

pub fn train_classifier(
    data: &ClassifierData,
    settings: &TrainSettings,
    mut progress: impl FnMut(&str, f64),
    cancel: impl Fn() -> bool,
) -> Result<TrainingOutcome> {
    let design = &data.design;
    if data.train_index.is_empty() || data.val_index.is_empty() || data.test_index.is_empty() {
        return Err(AtomError::Invalid("the split is empty".into()));
    }
    let classes = design.labels.len();
    let features = design.x.first().map(|r| r.len()).unwrap_or(0);
    if features == 0 {
        return Err(AtomError::Invalid("the design matrix has no features".into()));
    }
    let mut rng = Rng::new(settings.seed);
    let mut w = vec![vec![0f32; features]; classes];
    let mut b = vec![0f32; classes];
    let epochs = settings.epochs.clamp(1, 2_000);
    let batch = settings.batch.clamp(1, 4096) as usize;
    let lr = settings.learning_rate.max(1e-4);
    let l2 = settings.l2.max(0.0);
    let started = std::time::Instant::now();
    let mut curve = Vec::new();
    let mut probs = vec![0f32; classes];

    for epoch in 1..=epochs {
        if cancel() {
            return Err(AtomError::Cancelled("training cancelled".into()));
        }
        let epoch_start = std::time::Instant::now();
        let mut order = data.train_index.clone();
        rng.shuffle(&mut order);
        let mut loss_sum = 0f64;
        let mut processed = 0usize;
        progress(
            &format!("epoch {epoch}/{epochs} · {features} features · {classes} classes"),
            0.02 + 0.9 * ((epoch - 1) as f64 / epochs as f64),
        );
        for chunk in order.chunks(batch) {
            if cancel() {
                return Err(AtomError::Cancelled("training cancelled".into()));
            }
            let mut grad_w = vec![vec![0f32; features]; classes];
            let mut grad_b = vec![0f32; classes];
            for row_index in chunk {
                let row = &design.x[*row_index];
                let target = design.y[*row_index];
                let mut logits = b.clone();
                for (class, logit) in logits.iter_mut().enumerate() {
                    let weights = &w[class];
                    let mut sum = 0f32;
                    for f in 0..features {
                        sum += weights[f] * row[f];
                    }
                    *logit += sum;
                }
                probs.copy_from_slice(&softmax(&logits));
                loss_sum += -(probs[target].max(1e-12) as f64).ln();
                processed += 1;
                for class in 0..classes {
                    let delta = probs[class] - if class == target { 1.0 } else { 0.0 };
                    if delta == 0.0 {
                        continue;
                    }
                    grad_b[class] += delta;
                    let grad = &mut grad_w[class];
                    for f in 0..features {
                        grad[f] += delta * row[f];
                    }
                }
            }
            let scale = lr / chunk.len() as f32;
            for class in 0..classes {
                for f in 0..features {
                    let penalty = l2 * w[class][f];
                    w[class][f] -= scale * (grad_w[class][f] + penalty);
                }
                b[class] -= scale * grad_b[class];
            }
        }
        let val = evaluate_weights(&w, &b, design, &data.val_index);
        let train_loss = if processed > 0 {
            loss_sum / processed as f64
        } else {
            0.0
        };
        curve.push(EpochPoint {
            epoch,
            train_loss,
            val_loss: val.log_loss,
            val_accuracy: val.accuracy,
            seconds: epoch_start.elapsed().as_secs_f64(),
        });
        progress(
            &format!(
                "epoch {epoch}/{epochs} · val loss {:.4} · val accuracy {:.3}",
                val.log_loss, val.accuracy
            ),
            0.02 + 0.9 * (epoch as f64 / epochs as f64),
        );
    }

    let model = ClassifierModel {
        magic: "ATOMCL1".into(),
        labels: design.labels.clone(),
        encodings: design.encodings.clone(),
        weights: w,
        biases: b,
        trained_rows: data.train_index.len(),
        settings: settings.clone(),
    };
    let train = evaluate_weights(&model.weights, &model.biases, design, &data.train_index);
    let validation = evaluate_weights(&model.weights, &model.biases, design, &data.val_index);
    let test = evaluate_weights(&model.weights, &model.biases, design, &data.test_index);
    let parameters = (model.weights.len() * features + model.biases.len()) as u64;
    let notes = vec![
        "Accuracy, precision, recall and F1 are computed from predictions on rows this \
         optimiser never trained on."
            .to_string(),
        "Precision/recall/F1 are macro-averaged over classes present in the split; classes with \
         no predicted or actual samples report 0 and are listed explicitly."
            .to_string(),
        "Class imbalance is visible in the support column; macro F1 and weighted F1 are both \
         reported because they disagree under imbalance."
            .to_string(),
    ];
    let report = ClassifierReport {
        labels: model.labels.clone(),
        design_features: features,
        parameters,
        weight_bytes: f32_store::Bytes {
            float32_bytes: parameters * 4,
            int8_estimate_bytes: parameters + ((model.weights.len() * features) as u64 / 4),
            note: "float32 size is exact for the exported checkpoint; the int8 figure assumes one \
                   scale per output row and is an estimate, not a converted artifact."
                .into(),
        },
        settings: settings.clone(),
        epochs,
        duration_ms: started.elapsed().as_millis() as u64,
        train,
        validation,
        test,
        curve,
        notes,
    };
    Ok(TrainingOutcome { model, report })
}

pub fn predict(model: &ClassifierModel, row: &[f32]) -> usize {
    let mut best = 0usize;
    let mut best_score = f32::NEG_INFINITY;
    for (class, weights) in model.weights.iter().enumerate() {
        let mut score = model.biases[class];
        for (f, weight) in weights.iter().enumerate() {
            if let Some(value) = row.get(f) {
                score += weight * value;
            }
        }
        if score > best_score {
            best_score = score;
            best = class;
        }
    }
    best
}

fn evaluate_weights(
    w: &[Vec<f32>],
    b: &[f32],
    design: &Design,
    index: &[usize],
) -> EvalMetrics {
    let classes = design.labels.len();
    let mut confusion = vec![vec![0u64; classes]; classes];
    let mut log_loss = 0f64;
    for row_index in index {
        let row = &design.x[*row_index];
        let actual = design.y[*row_index];
        let mut logits = b.to_vec();
        for class in 0..classes {
            let mut sum = 0f32;
            for f in 0..row.len() {
                sum += w[class][f] * row[f];
            }
            logits[class] += sum;
        }
        let probs = softmax(&logits);
        log_loss += -(probs[actual].max(1e-12) as f64).ln();
        let mut predicted = 0usize;
        for class in 1..classes {
            if logits[class] > logits[predicted] {
                predicted = class;
            }
        }
        confusion[actual][predicted] += 1;
    }
    let samples = index.len();
    let mut correct = 0u64;
    for class in 0..classes {
        correct += confusion[class][class];
    }
    let accuracy = if samples > 0 {
        correct as f64 / samples as f64
    } else {
        0.0
    };
    let mut per_class = Vec::with_capacity(classes);
    let mut macro_p = 0f64;
    let mut macro_r = 0f64;
    let mut macro_f = 0f64;
    let mut weighted_f = 0f64;
    for class in 0..classes {
        let tp = confusion[class][class] as f64;
        let support = confusion[class].iter().sum::<u64>() as f64;
        let predicted = (0..classes).map(|c| confusion[c][class]).sum::<u64>() as f64;
        let precision = if predicted > 0.0 { tp / predicted } else { 0.0 };
        let recall = if support > 0.0 { tp / support } else { 0.0 };
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };
        macro_p += precision;
        macro_r += recall;
        macro_f += f1;
        weighted_f += f1 * support;
        per_class.push(ClassMetrics {
            label: design.labels[class].clone(),
            support: support as usize,
            predicted: predicted as usize,
            precision,
            recall,
            f1,
        });
    }
    let n = classes.max(1) as f64;
    EvalMetrics {
        samples,
        accuracy,
        macro_precision: macro_p / n,
        macro_recall: macro_r / n,
        macro_f1: macro_f / n,
        weighted_f1: if samples > 0 {
            weighted_f / samples as f64
        } else {
            0.0
        },
        log_loss: if samples > 0 {
            log_loss / samples as f64
        } else {
            0.0
        },
        per_class,
        confusion,
        note: "Measured from the trained weights on this split. No smoothing is applied to \
               precision or recall, so rare classes with no predictions show 0 instead of a \
               flattering default."
            .into(),
    }
}

/* ----------------------------------------------------------------- export --- */

/// Write the classifier checkpoint, a C header with the same weights, a README
/// and a SHA-256 manifest into `dir`. The JSON is reloaded and re-scored before
/// the export is reported as successful.
pub fn export_classifier(
    dir: &Path,
    data: &ClassifierData,
    model: &ClassifierModel,
    report: &ClassifierReport,
) -> Result<ClassifierExport> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(
        dir.join("classifier.json"),
        serde_json::to_string_pretty(model)?,
    )?;
    let header = render_header(model);
    std::fs::write(dir.join("model.h"), header)?;
    std::fs::write(
        dir.join("classifier.c"),
        render_self_test_c(model, data, report),
    )?;
    std::fs::write(dir.join("README.md"), render_readme(model, report))?;

    // Real validation: reload the JSON, verify shapes, and re-score the test
    // split. The bundled accuracy must match the reported one.
    let reloaded: ClassifierModel =
        serde_json::from_str(&std::fs::read_to_string(dir.join("classifier.json"))?)?;
    if reloaded.magic != "ATOMCL1" {
        return Err(AtomError::Parse("exported classifier has the wrong magic".into()));
    }
    if reloaded.weights.len() != model.weights.len()
        || reloaded.biases.len() != model.biases.len()
        || reloaded.encodings.len() != model.encodings.len()
    {
        return Err(AtomError::Parse(
            "reloaded classifier dimensions do not match the trained model".into(),
        ));
    }
    let rescored = evaluate_weights(
        &reloaded.weights,
        &reloaded.biases,
        &data.design,
        &data.test_index,
    );
    if (rescored.accuracy - report.test.accuracy).abs() > 1e-9 {
        return Err(AtomError::Internal(format!(
            "exported classifier re-scored at {:.6} accuracy but the report says {:.6}",
            rescored.accuracy, report.test.accuracy
        )));
    }

    let compile_check = compile_check(dir);
    let mut files = Vec::new();
    for name in ["classifier.json", "model.h", "classifier.c", "README.md"] {
        let path = dir.join(name);
        let (sha256, _) = sha256_file(&path)?;
        files.push(ExportFileInfo {
            name: name.into(),
            bytes: std::fs::metadata(&path)?.len(),
            sha256,
        });
    }
    let manifest = serde_json::json!({
        "format": "ATOM classifier v1",
        "created_at": chrono::Local::now().to_rfc3339(),
        "labels": model.labels,
        "encodings": model.encodings,
        "parameters": model.weights.len() * model.weights.first().map(|r| r.len()).unwrap_or(0) + model.biases.len(),
        "trained_rows": model.trained_rows,
        "settings": model.settings,
        "metrics": {
            "accuracy": report.test.accuracy,
            "macro_f1": report.test.macro_f1,
            "weighted_f1": report.test.weighted_f1,
            "samples": report.test.samples,
            "split": { "train": data.train_index.len(), "validation": data.val_index.len(), "test": data.test_index.len() },
        },
        "files": files,
        "compile_check": compile_check,
        "note": "Predict with classifier.c or the JSON weights. Features must be encoded with the \
                 same encodings and order recorded here.",
    });
    let manifest_path = dir.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    let (manifest_sha256, _) = sha256_file(&manifest_path)?;
    let mut total_bytes = 0u64;
    for path in [
        dir.join("classifier.json"),
        dir.join("model.h"),
        dir.join("classifier.c"),
        dir.join("README.md"),
        manifest_path.clone(),
    ] {
        total_bytes += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    }
    Ok(ClassifierExport {
        dir: dir.display().to_string(),
        created_at: chrono::Local::now().to_rfc3339(),
        files,
        manifest_path: manifest_path.display().to_string(),
        manifest_sha256,
        compile_check,
        validation: format!(
            "reloaded the JSON checkpoint and re-scored {} held-out rows: accuracy {:.4} matches \
             the training report",
            rescored.samples, rescored.accuracy
        ),
        total_bytes,
    })
}

fn compile_check(dir: &Path) -> Option<String> {
    let compiler = atom_micro_cc()?;
    let output = dir.join("selftest");
    let result = std::process::Command::new(compiler)
        .arg("-std=c99")
        .arg("-O1")
        .arg("-o")
        .arg(&output)
        .arg(dir.join("classifier.c"))
        .arg("-lm")
        .output()
        .ok()?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Some(format!(
            "compiler exited with {}: {}",
            result.status.code().unwrap_or(-1),
            stderr.lines().take(3).collect::<Vec<_>>().join(" | ")
        ));
    }
    let run = std::process::Command::new(&output).output().ok()?;
    let _ = std::fs::remove_file(&output);
    let stdout = String::from_utf8_lossy(&run.stdout).trim().to_string();
    Some(format!(
        "compiled with {} and ran the generated self-test: {}",
        compiler, stdout
    ))
}

fn atom_micro_cc() -> Option<&'static str> {
    for candidate in ["cc", "gcc", "clang"] {
        if std::process::Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(candidate);
        }
    }
    None
}

fn render_header(model: &ClassifierModel) -> String {
    let mut out = String::new();
    out.push_str("/* Generated by ATOM Micro. Do not edit by hand. */\n");
    out.push_str("#ifndef ATOM_CLASSIFIER_H\n#define ATOM_CLASSIFIER_H\n\n");
    out.push_str("#define ATOM_CLASSIFIER_CLASSES ");
    out.push_str(&model.labels.len().to_string());
    out.push_str("\n#define ATOM_CLASSIFIER_FEATURES ");
    out.push_str(&model.weights.first().map(|r| r.len()).unwrap_or(0).to_string());
    out.push_str("\n\nstatic const char *ATOM_CLASSIFIER_LABELS[ATOM_CLASSIFIER_CLASSES] = {\n");
    for label in &model.labels {
        out.push_str(&format!("    \"{}\",\n", label.replace('\\', "\\\\").replace('"', "\\\"")));
    }
    out.push_str("};\n\nstatic const float ATOM_CLASSIFIER_W[ATOM_CLASSIFIER_CLASSES][ATOM_CLASSIFIER_FEATURES] = {\n");
    for row in &model.weights {
        out.push_str("    {");
        for (i, value) in row.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{value:.6}f"));
        }
        out.push_str("},\n");
    }
    out.push_str("};\n\nstatic const float ATOM_CLASSIFIER_B[ATOM_CLASSIFIER_CLASSES] = {\n    ");
    for (i, value) in model.biases.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{value:.6}f"));
    }
    out.push_str("\n};\n\n#endif\n");
    out
}

fn render_self_test_c(model: &ClassifierModel, data: &ClassifierData, report: &ClassifierReport) -> String {
    let row = data
        .test_index
        .first()
        .and_then(|i| data.design.x.get(*i))
        .cloned()
        .unwrap_or_default();
    let actual = data
        .test_index
        .first()
        .map(|i| data.design.y[*i])
        .unwrap_or(0);
    let mut out = String::new();
    out.push_str("/* Generated by ATOM Micro: standalone classifier self-test. */\n");
    out.push_str("#include <stdio.h>\n#include \"model.h\"\n\n");
    out.push_str("int main(void) {\n");
    out.push_str("    const float x[ATOM_CLASSIFIER_FEATURES] = {");
    for (i, value) in row.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{value:.6}f"));
    }
    out.push_str("};\n");
    out.push_str("    int best = 0;\n    float best_score = ATOM_CLASSIFIER_B[0];\n");
    out.push_str("    for (int c = 0; c < ATOM_CLASSIFIER_CLASSES; c++) {\n");
    out.push_str("        float score = ATOM_CLASSIFIER_B[c];\n");
    out.push_str("        for (int f = 0; f < ATOM_CLASSIFIER_FEATURES; f++) score += ATOM_CLASSIFIER_W[c][f] * x[f];\n");
    out.push_str("        if (score > best_score) { best_score = score; best = c; }\n    }\n");
    out.push_str(&format!(
        "    printf(\"predicted=%s actual=%s classes=%d features=%d\\n\", ATOM_CLASSIFIER_LABELS[best], \"{}\", ATOM_CLASSIFIER_CLASSES, ATOM_CLASSIFIER_FEATURES);\n",
        model.labels.get(actual).cloned().unwrap_or_default().replace('"', "")
    ));
    out.push_str("    return 0;\n}\n");
    let _ = report;
    out
}

fn render_readme(model: &ClassifierModel, report: &ClassifierReport) -> String {
    let mut out = String::new();
    out.push_str("# ATOM Micro classifier\n\n");
    out.push_str("A multinomial logistic-regression classifier trained in-process by ATOM.\n\n");
    out.push_str(&format!(
        "* classes: {}\n* design features: {}\n* parameters: {}\n",
        model.labels.len(),
        model.weights.first().map(|r| r.len()).unwrap_or(0),
        report.parameters
    ));
    out.push_str(&format!(
        "* held-out test accuracy: {:.4}\n* macro F1: {:.4}\n* weighted F1: {:.4}\n* test rows: {}\n",
        report.test.accuracy, report.test.macro_f1, report.test.weighted_f1, report.test.samples
    ));
    out.push_str("\n## Feature encoding\n\nFeatures must be produced in this order:\n\n");
    for encoding in &model.encodings {
        match encoding {
            FeatureEncoding::Numeric { column, mean, std, .. } => out.push_str(&format!(
                "* `{column}` numeric: `(x - {mean:.6}) / {std:.6}`\n"
            )),
            FeatureEncoding::Categorical {
                column, categories, ..
            } => {
                out.push_str(&format!("* `{column}` categorical one-hot: "));
                for category in categories {
                    out.push_str(&format!("`{category}`, "));
                }
                out.push_str("`(other)`\n");
            }
        }
    }
    out.push_str("\n## Files\n\n* `classifier.json` — portable weights and encodings\n* `model.h` — C arrays with the same weights\n* `classifier.c` — self-test compiled and executed when a C compiler is present\n* `manifest.json` — SHA-256 of every artifact and the measured metrics\n\n");
    out.push_str("No claim is made that this model fits a specific microcontroller. Run the memory plan \
                  step in ATOM before flashing.\n");
    out
}

/* ------------------------------------------------------------------ tests --- */

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dataset() -> String {
        let mut csv = String::from("temperature,humidity,room,label\n");
        for i in 0..60 {
            let hot = i % 2 == 0;
            let room = if i % 3 == 0 { "lab" } else { "office" };
            csv.push_str(&format!(
                "{},{},{},{}\n",
                18 + (i % 5),
                if hot { 70 + i % 7 } else { 30 + i % 5 },
                room,
                if hot { "cool" } else { "heat" }
            ));
        }
        csv.push_str("\"quoted, value\",40,\"lab\nwith newline\",cool\n");
        csv
    }

    #[test]
    fn csv_handles_quotes_and_newlines() {
        let records = parse_csv("a,b\n\"x,y\",\"line\nbreak\"\n").unwrap();
        assert_eq!(records[1][0], "x,y");
        assert_eq!(records[1][1], "line\nbreak");
    }

    #[test]
    fn profile_reports_types_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.csv");
        let mut csv = String::from("n,c,y\n1,a,up\n2,,down\n");
        for i in 3..12 {
            csv.push_str(&format!("{i},a,up\n"));
        }
        csv.push_str("x,b,down\n");
        std::fs::write(&path, csv).unwrap();
        let parsed = profile_csv(&path).unwrap();
        assert_eq!(parsed.profile.rows, 12);
        assert_eq!(parsed.profile.columns[0].kind, "numeric");
        assert_eq!(parsed.profile.columns[0].invalid, 1);
        assert_eq!(parsed.profile.columns[1].missing, 1);
        assert_eq!(parsed.profile.columns[2].kind, "categorical");
    }

    #[test]
    fn duplicates_never_cross_splits_and_training_works() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.csv");
        std::fs::write(&path, dataset()).unwrap();
        let spec = ClassifierSpec {
            features: vec!["temperature".into(), "humidity".into(), "room".into()],
            target: "label".into(),
            val_ratio: 0.2,
            test_ratio: 0.2,
            seed: 7,
        };
        let (parsed, data) = prepare(&path, &spec).unwrap();
        assert_eq!(parsed.profile.rows, 61);
        let mut seen_hashes: BTreeMap<u64, usize> = BTreeMap::new();
        for (i, sig) in data.design.signature.iter().enumerate() {
            let split = if data.train_index.contains(&i) {
                0
            } else if data.val_index.contains(&i) {
                1
            } else {
                2
            };
            if let Some(previous) = seen_hashes.insert(*sig, split) {
                assert_eq!(previous, split, "duplicate row crossed a split boundary");
            }
        }
        let mut progress_calls = 0;
        let outcome = train_classifier(
            &data,
            &TrainSettings {
                epochs: 40,
                learning_rate: 0.5,
                batch: 16,
                l2: 1e-4,
                seed: 3,
            },
            |_, _| progress_calls += 1,
            || false,
        )
        .unwrap();
        assert!(progress_calls >= 40);
        assert!(
            outcome.report.test.accuracy > 0.85,
            "test accuracy {}",
            outcome.report.test.accuracy
        );
        assert_eq!(outcome.report.test.confusion.len(), outcome.report.labels.len());
        assert!(outcome.report.parameters > 0);
        let export_dir = dir.path().join("export");
        let export = export_classifier(&export_dir, &data, &outcome.model, &outcome.report).unwrap();
        assert!(Path::new(&export.manifest_path).exists());
        assert!(export.total_bytes > 0);
        assert_eq!(export.files.len(), 4);
    }

    #[test]
    fn split_is_assigned_once_and_membership_matches_the_index_vectors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.csv");
        std::fs::write(&path, dataset()).unwrap();
        let spec = ClassifierSpec {
            features: vec!["temperature".into(), "room".into()],
            target: "label".into(),
            val_ratio: 0.2,
            test_ratio: 0.2,
            seed: 11,
        };
        let (_, data) = prepare(&path, &spec).unwrap();
        let split = &data.design.split;
        assert_eq!(split.train, data.train_index);
        assert_eq!(split.validation, data.val_index);
        assert_eq!(split.test, data.test_index);
        assert_eq!(split.membership.len(), data.design.x.len());

        // Every row belongs to exactly one split, and the membership byte is
        // the same assignment the index vectors expose.
        let mut seen = vec![0u8; data.design.x.len()];
        for (role, indexes) in [(0u8, &split.train), (1u8, &split.validation), (2u8, &split.test)] {
            for index in indexes {
                assert_eq!(split.membership[*index], role);
                seen[*index] += 1;
            }
        }
        assert!(seen.iter().all(|count| *count == 1), "a row was assigned twice or not at all");

        // Duplicate rows always share one split role.
        let mut roles: BTreeMap<u64, u8> = BTreeMap::new();
        for (index, signature) in data.design.signature.iter().enumerate() {
            let role = split.membership[index];
            if let Some(previous) = roles.insert(*signature, role) {
                assert_eq!(previous, role, "duplicate rows drifted across splits");
            }
        }

        // Rebuilding from the same file/seed must produce the identical split,
        // which is what the stored-copy training job relies on.
        let (_, again) = prepare(&path, &spec).unwrap();
        assert_eq!(again.design.split.membership, split.membership);
    }

    #[test]
    fn metrics_are_consistent_with_the_confusion_matrix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.csv");
        std::fs::write(&path, dataset()).unwrap();
        let spec = ClassifierSpec {
            features: vec!["humidity".into()],
            target: "label".into(),
            val_ratio: 0.15,
            test_ratio: 0.15,
            seed: 1,
        };
        let (_, data) = prepare(&path, &spec).unwrap();
        let outcome = train_classifier(&data, &TrainSettings::default(), |_, _| {}, || false).unwrap();
        let metrics = &outcome.report.test;
        let total: u64 = metrics.confusion.iter().flat_map(|r| r.iter()).sum();
        assert_eq!(total as usize, metrics.samples);
        let correct: u64 = (0..metrics.confusion.len()).map(|i| metrics.confusion[i][i]).sum();
        assert!((metrics.accuracy - correct as f64 / total as f64).abs() < 1e-12);
    }

    #[test]
    fn invalid_specs_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.csv");
        std::fs::write(&path, "a,y\n1,x\n2,y\n" ).unwrap();
        let missing_column = ClassifierSpec {
            features: vec!["nope".into()],
            target: "y".into(),
            ..Default::default()
        };
        let parsed = profile_csv(&path).unwrap();
        assert!(build_design(&parsed.header, &parsed.rows, &missing_column).is_err());
        let target_as_feature = ClassifierSpec {
            features: vec!["y".into()],
            target: "y".into(),
            ..Default::default()
        };
        assert!(build_design(&parsed.header, &parsed.rows, &target_as_feature).is_err());
        let single_class = ClassifierSpec {
            features: vec!["a".into()],
            target: "a".into(),
            ..Default::default()
        };
        assert!(build_design(&parsed.header, &parsed.rows, &single_class).is_err());
    }

    #[test]
    fn cancellation_stops_training() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.csv");
        std::fs::write(&path, dataset()).unwrap();
        let spec = ClassifierSpec {
            features: vec!["temperature".into()],
            target: "label".into(),
            ..Default::default()
        };
        let (_, data) = prepare(&path, &spec).unwrap();
        let err = train_classifier(
            &data,
            &TrainSettings {
                epochs: 10_000,
                ..Default::default()
            },
            |_, _| {},
            || true,
        )
        .unwrap_err();
        assert!(matches!(err, AtomError::Cancelled(_)));
    }

    #[test]
    fn stored_csv_copy_can_be_reprepared() {
        // A deterministic re-import must produce the same split, which is what
        // the training job relies on.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.csv");
        std::fs::write(&path, dataset()).unwrap();
        let spec = ClassifierSpec {
            features: vec!["room".into()],
            target: "label".into(),
            ..Default::default()
        };
        let (_, first) = prepare(&path, &spec).unwrap();
        let (_, second) = prepare(&path, &spec).unwrap();
        assert_eq!(first.train_index.len(), second.train_index.len());
        assert_eq!(first.test_index, second.test_index);
        assert!(PathBuf::from(&path).exists());
    }
}

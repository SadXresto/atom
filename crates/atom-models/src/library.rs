//! Persistent model library: records, import, verification and variant links.

use atom_core::{AtomError, Result};
use atom_security::{sanitize_label, sha256_file, validate_file_name, PathGuard};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::gguf::GgufFile;
use crate::summary::ModelSummary;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelKind {
    Original,
    Compressed {
        source_model_id: String,
        source_sha256: Option<String>,
        format: String,
        job_id: String,
        output_sha256: String,
        source_size_bytes: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Verification {
    Verified { checked_at: String },
    Unverified,
    Failed { reason: String, checked_at: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelRecord {
    pub id: String,
    /// User-facing name. Derived from the file, editable.
    pub label: String,
    pub file_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub verification: Verification,
    pub imported_at: String,
    pub kind: ModelKind,
    pub summary: ModelSummary,
    pub notes: String,
}

impl ModelRecord {
    pub fn is_compressed(&self) -> bool {
        matches!(self.kind, ModelKind::Compressed { .. })
    }

    pub fn quant_label(&self) -> String {
        self.summary.label_quant()
    }

    pub fn display_kind(&self) -> &'static str {
        if self.is_compressed() {
            "Compressed"
        } else {
            "Original"
        }
    }

    pub fn short_sha(&self) -> Option<String> {
        self.sha256.as_ref().map(|s| s.chars().take(12).collect())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Library {
    #[serde(skip)]
    path: PathBuf,
    pub models: Vec<ModelRecord>,
}

impl Library {
    pub fn empty(path: PathBuf) -> Self {
        Library { path, models: Vec::new() }
    }

    pub fn load(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Library::empty(path));
        }
        let raw = std::fs::read_to_string(&path)?;
        // The file holds just the record array; older builds wrapped it in a
        // { "models": [...] } object, which is still accepted.
        let models: Vec<ModelRecord> = serde_json::from_str(&raw)
            .or_else(|_| {
                serde_json::from_str::<Library>(&raw).map(|lib| lib.models)
            })
            .unwrap_or_default();
        Ok(Library { path, models })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&self.models)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn list(&self) -> &[ModelRecord] {
        &self.models
    }

    pub fn get(&self, id: &str) -> Option<&ModelRecord> {
        self.models.iter().find(|m| m.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut ModelRecord> {
        self.models.iter_mut().find(|m| m.id == id)
    }

    pub fn insert(&mut self, record: ModelRecord) -> Result<()> {
        if self.models.iter().any(|m| m.id == record.id) {
            return Err(AtomError::Conflict("a model with that id already exists".into()));
        }
        self.models.push(record);
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> Result<ModelRecord> {
        let idx = self
            .models
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| AtomError::NotFound(format!("model {id} is not in the library")))?;
        let record = self.models.remove(idx);
        // Variants claim a source; mark them orphaned rather than silently lying
        // about their provenance.
        for variant in self.models.iter_mut() {
            if let ModelKind::Compressed { source_model_id, .. } = &mut variant.kind {
                if *source_model_id == id {
                    *source_model_id = format!("removed:{id}");
                }
            }
        }
        self.save()?;
        Ok(record)
    }

    /// Compressed variants derived from a given original model.
    pub fn variants_of(&self, id: &str) -> Vec<&ModelRecord> {
        self.models
            .iter()
            .filter(|m| matches!(&m.kind, ModelKind::Compressed { source_model_id, .. } if source_model_id == id))
            .collect()
    }

    /// Walk to the original model for a variant, if it is still present.
    pub fn source_of(&self, model: &ModelRecord) -> Option<&ModelRecord> {
        match &model.kind {
            ModelKind::Compressed { source_model_id, .. } => self.get(source_model_id),
            ModelKind::Original => None,
        }
    }

    pub fn in_use_paths(&self) -> Vec<&str> {
        self.models.iter().map(|m| m.path.as_str()).collect()
    }
}

/// Copy a file into the library directory while reporting real byte progress.
fn copy_into(src: &Path, dest_dir: &Path, mut on_progress: impl FnMut(u64, u64)) -> Result<PathBuf> {
    use std::io::{Read, Write};
    let total = std::fs::metadata(src)?.len();
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| AtomError::Invalid("source file has no name".into()))?;
    validate_file_name(&name)?;
    std::fs::create_dir_all(dest_dir)?;
    let mut dest = dest_dir.join(&name);
    let mut counter = 1;
    while dest.exists() {
        let stem = Path::new(&name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        dest = dest_dir.join(format!("{stem}-{counter}.gguf"));
        counter += 1;
        if counter > 999 {
            return Err(AtomError::Conflict("too many files with the same name".into()));
        }
    }
    let mut reader = std::fs::File::open(src)?;
    let mut writer = std::fs::File::create(&dest)?;
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut done: u64 = 0;
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buf[..read])?;
        done += read as u64;
        on_progress(done, total);
    }
    writer.sync_all()?;
    Ok(dest)
}

#[derive(Debug)]
pub struct ImportOutcome {
    pub record: ModelRecord,
    pub copied: bool,
}

/// Import a GGUF file into the library. The original is copied, never moved and
/// never modified. If the source already lives inside the models directory it is
/// adopted in place.
pub fn import_model(
    guard: &PathGuard,
    models_dir: &Path,
    source: &Path,
    verify: bool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<ImportOutcome> {
    let source = guard.resolve_existing(source)?;
    let ext_ok = source
        .extension()
        .map(|e| e.eq_ignore_ascii_case("gguf"))
        .unwrap_or(false);
    if !ext_ok {
        return Err(AtomError::Invalid(
            "ATOM imports GGUF files. Convert other formats to GGUF first (llama.cpp convert_hf_to_gguf.py).".into(),
        ));
    }

    let models_dir_canonical = {
        std::fs::create_dir_all(models_dir)?;
        models_dir.canonicalize()?
    };
    let already_inside = source.starts_with(&models_dir_canonical);
    let path = if already_inside {
        on_progress(1, 1);
        source.clone()
    } else {
        copy_into(&source, &models_dir_canonical, &mut on_progress)?
    };

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "model.gguf".into());
    let size_bytes = std::fs::metadata(&path)?.len();

    let gguf = GgufFile::open(&path)?;
    let summary = ModelSummary::from_gguf(&gguf, gguf.get_u64("general.file_type").unwrap_or(0) as u32);

    // The checksum is recorded on every import: it is what makes duplicate
    // detection and later tamper checks meaningful. `verify` is retained for
    // API compatibility and no longer disables hashing.
    let _ = verify;
    on_progress(size_bytes, size_bytes);
    let sha256 = Some(sha256_file(&path)?.0);

    let label = summary
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            Path::new(&file_name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| file_name.clone())
        });

    let record = ModelRecord {
        id: uuid::Uuid::new_v4().to_string(),
        label: sanitize_label(&label, 120),
        file_name,
        path: path.display().to_string(),
        size_bytes,
        sha256: sha256.clone(),
        verification: match &sha256 {
            Some(_) => Verification::Verified { checked_at: chrono::Local::now().to_rfc3339() },
            None => Verification::Unverified,
        },
        imported_at: chrono::Local::now().to_rfc3339(),
        kind: ModelKind::Original,
        summary,
        notes: String::new(),
    };
    Ok(ImportOutcome { record, copied: !already_inside })
}

/// Re-check a record's file against the stored checksum.
pub fn verify_record(record: &mut ModelRecord) -> Result<()> {
    let path = Path::new(&record.path);
    if !path.exists() {
        record.verification = Verification::Failed {
            reason: "file is missing from disk".into(),
            checked_at: chrono::Local::now().to_rfc3339(),
        };
        return Ok(());
    }
    let size = std::fs::metadata(path)?.len();
    if size != record.size_bytes {
        record.verification = Verification::Failed {
            reason: format!("size changed: {size} bytes on disk, {} bytes recorded", record.size_bytes),
            checked_at: chrono::Local::now().to_rfc3339(),
        };
        return Ok(());
    }
    match &record.sha256 {
        Some(expected) => {
            let (actual, _) = sha256_file(path)?;
            if &actual == expected {
                record.verification = Verification::Verified { checked_at: chrono::Local::now().to_rfc3339() };
            } else {
                record.verification = Verification::Failed {
                    reason: format!("checksum mismatch: expected {expected}, found {actual}"),
                    checked_at: chrono::Local::now().to_rfc3339(),
                };
            }
        }
        None => {
            record.sha256 = Some(sha256_file(path)?.0);
            record.verification = Verification::Verified { checked_at: chrono::Local::now().to_rfc3339() };
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gguf::fixture::tiny_model;
    use crate::quant::GGML_TYPE_F16;

    fn setup() -> (tempfile::TempDir, PathGuard, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let models = dir.path().join("models");
        let incoming = dir.path().join("incoming");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::create_dir_all(&incoming).unwrap();
        let guard = PathGuard::new(vec![models.clone(), incoming.clone()]);
        (dir, guard, models, incoming)
    }

    #[test]
    fn imports_copy_and_summarizes() {
        let (_dir, guard, models, incoming) = setup();
        let src = incoming.join("tiny.gguf");
        std::fs::write(&src, tiny_model(GGML_TYPE_F16)).unwrap();
        let outcome = import_model(&guard, &models, &src, true, |_, _| {}).unwrap();
        assert!(outcome.copied);
        assert_eq!(outcome.record.summary.architecture.as_deref(), Some("llama"));
        assert!(outcome.record.sha256.is_some());
        assert!(std::path::Path::new(&outcome.record.path).exists());
        // Source must still exist untouched.
        assert!(src.exists());
    }

    #[test]
    fn adopts_files_already_in_models_dir() {
        let (_dir, guard, models, _incoming) = setup();
        let src = models.join("inplace.gguf");
        std::fs::write(&src, tiny_model(GGML_TYPE_F16)).unwrap();
        let outcome = import_model(&guard, &models, &src, false, |_, _| {}).unwrap();
        assert!(!outcome.copied);
        assert_eq!(outcome.record.path, src.canonicalize().unwrap().display().to_string());
    }

    #[test]
    fn rejects_non_gguf_and_outside_paths() {
        let (dir, guard, models, incoming) = setup();
        let txt = incoming.join("notes.txt");
        std::fs::write(&txt, b"hi").unwrap();
        assert_eq!(
            import_model(&guard, &models, &txt, false, |_, _| {}).unwrap_err().code(),
            "invalid_request"
        );

        let outside = dir.path().join("elsewhere.gguf");
        std::fs::write(&outside, tiny_model(GGML_TYPE_F16)).unwrap();
        assert_eq!(
            import_model(&guard, &models, &outside, false, |_, _| {}).unwrap_err().code(),
            "forbidden"
        );
    }

    #[test]
    fn rejects_corrupt_gguf() {
        let (_dir, guard, models, incoming) = setup();
        let bad = incoming.join("bad.gguf");
        std::fs::write(&bad, b"GGUF\x03\x00\x00\x00garbage").unwrap();
        assert!(import_model(&guard, &models, &bad, false, |_, _| {}).is_err());
    }

    #[test]
    fn detects_tampering() {
        let (_dir, guard, models, incoming) = setup();
        let src = incoming.join("tiny.gguf");
        std::fs::write(&src, tiny_model(GGML_TYPE_F16)).unwrap();
        let outcome = import_model(&guard, &models, &src, true, |_, _| {}).unwrap();
        let mut record = outcome.record;
        verify_record(&mut record).unwrap();
        assert!(matches!(record.verification, Verification::Verified { .. }));

        let path = PathBuf::from(&record.path);
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&path, &bytes).unwrap();
        verify_record(&mut record).unwrap();
        assert!(matches!(record.verification, Verification::Failed { .. }), "{:?}", record.verification);
    }

    #[test]
    fn library_persists_and_links_variants() {
        let (dir, guard, models, incoming) = setup();
        let src = incoming.join("tiny.gguf");
        std::fs::write(&src, tiny_model(GGML_TYPE_F16)).unwrap();
        let mut lib = Library::empty(dir.path().join("library.json"));
        let base = import_model(&guard, &models, &src, true, |_, _| {}).unwrap().record;
        let base_id = base.id.clone();
        lib.insert(base).unwrap();

        let mut variant = lib.get(&base_id).unwrap().clone();
        variant.id = "variant-1".into();
        variant.kind = ModelKind::Compressed {
            source_model_id: base_id.clone(),
            source_sha256: None,
            format: "Q4_K_M".into(),
            job_id: "job-1".into(),
            output_sha256: "abc".into(),
            source_size_bytes: 42,
        };
        lib.insert(variant).unwrap();
        assert_eq!(lib.variants_of(&base_id).len(), 1);

        let reloaded = Library::load(dir.path().join("library.json")).unwrap();
        assert_eq!(reloaded.models.len(), 2);
        lib.remove(&base_id).unwrap();
        assert_eq!(lib.models.len(), 1);
        assert!(matches!(&lib.models[0].kind, ModelKind::Compressed { source_model_id, .. } if source_model_id.starts_with("removed:")));
    }
}

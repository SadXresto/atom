//! Path containment, checksums, input validation and process invocation rules.
//!
//! ATOM never builds a shell command string. Every external tool is invoked
//! with an explicit argv, and every file path is resolved against an allowlist
//! of roots before it is opened for read or write.

use atom_core::{AtomError, Result};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

/// Allowlist of directories ATOM may touch. Paths are canonicalized so `..`
/// and symlink escapes are rejected after resolution, not before.
#[derive(Clone, Debug, Default)]
pub struct PathGuard {
    roots: Vec<PathBuf>,
}

impl PathGuard {
    pub fn new<I: IntoIterator<Item = PathBuf>>(roots: I) -> Self {
        // Roots may not exist yet (an empty models directory on first run), so
        // keep the raw path when canonicalization fails instead of creating it.
        let resolved = roots
            .into_iter()
            .map(|root| root.canonicalize().unwrap_or(root))
            .collect();
        PathGuard { roots: resolved }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Resolve an existing path, rejecting anything outside the allowlist.
    pub fn resolve_existing(&self, candidate: impl AsRef<Path>) -> Result<PathBuf> {
        let path = candidate.as_ref();
        reject_suspicious(path)?;
        let canonical = path
            .canonicalize()
            .map_err(|_| AtomError::NotFound(format!("{} does not exist", path.display())))?;
        if !self.contains(&canonical) {
            return Err(AtomError::Forbidden(format!(
                "{} is outside the approved model and workspace directories",
                canonical.display()
            )));
        }
        Ok(canonical)
    }

    /// Resolve a not-yet-existing output path. The parent must exist inside the
    /// allowlist and the file name must be a plain file name.
    pub fn resolve_new(&self, candidate: impl AsRef<Path>) -> Result<PathBuf> {
        let path = candidate.as_ref();
        reject_suspicious(path)?;
        let name = path
            .file_name()
            .ok_or_else(|| AtomError::Invalid("output path has no file name".into()))?;
        validate_file_name(&name.to_string_lossy())?;
        let parent = path
            .parent()
            .ok_or_else(|| AtomError::Invalid("output path has no parent directory".into()))?;
        std::fs::create_dir_all(parent)?;
        let parent = parent.canonicalize()?;
        if !self.contains(&parent) {
            return Err(AtomError::Forbidden(format!(
                "{} is outside the approved model and workspace directories",
                parent.display()
            )));
        }
        Ok(parent.join(name))
    }

    pub fn contains(&self, canonical: &Path) -> bool {
        self.roots.iter().any(|root| {
            if canonical.starts_with(root) {
                return true;
            }
            // A root that did not exist at construction time may have appeared
            // since; re-canonicalize it before rejecting.
            match root.canonicalize() {
                Ok(fresh) => canonical.starts_with(&fresh),
                Err(_) => false,
            }
        })
    }
}

fn reject_suspicious(path: &Path) -> Result<()> {
    let text = path.to_string_lossy();
    if text.contains('\0') {
        return Err(AtomError::Invalid("path contains a null byte".into()));
    }
    for comp in path.components() {
        if let Component::ParentDir = comp {
            // Allowed only if canonicalization still lands inside a root.
            continue;
        }
    }
    Ok(())
}

/// File names ATOM is willing to create: no separators, no shell metacharacters,
/// no leading dots, bounded length.
pub fn validate_file_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 200 {
        return Err(AtomError::Invalid("file name must be 1-200 characters".into()));
    }
    if trimmed.starts_with('.') {
        return Err(AtomError::Invalid("file name may not start with a dot".into()));
    }
    let banned = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0', '\n', '\r', ';', '&', '`', '$'];
    if trimmed.chars().any(|c| banned.contains(&c) || c.is_control()) {
        return Err(AtomError::Invalid(format!("'{trimmed}' contains characters that are not allowed in a file name")));
    }
    Ok(())
}

/// Human display name for a model: trimmed, control characters removed, bounded.
pub fn sanitize_label(text: &str, max: usize) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect();
    cleaned.trim().to_string()
}

/// Streaming SHA-256 with byte count. Reported to the UI as a verification
/// fact, never as a progress guess.
pub fn sha256_file(path: &Path) -> Result<(String, u64)> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut total: u64 = 0;
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
        total += read as u64;
    }
    Ok((hex::encode(hasher.finalize()), total))
}

/// Resolve an executable from an explicit path or the PATH environment.
pub fn find_executable(name: &str, explicit: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
        return None;
    }
    let exe_names: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.exe"), name.to_string()]
    } else {
        vec![name.to_string()]
    };
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for exe in &exe_names {
            let candidate = dir.join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_outside_roots() {
        let dir = tempfile::tempdir().unwrap();
        let inside = dir.path().join("models");
        std::fs::create_dir_all(&inside).unwrap();
        let guard = PathGuard::new(vec![inside.clone()]);
        let file = inside.join("a.gguf");
        std::fs::write(&file, b"x").unwrap();
        assert!(guard.resolve_existing(&file).is_ok());

        let outside = dir.path().join("secret.txt");
        std::fs::write(&outside, b"x").unwrap();
        let err = guard.resolve_existing(&outside).unwrap_err();
        assert_eq!(err.code(), "forbidden");
    }

    #[test]
    fn rejects_traversal_after_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let models = dir.path().join("models");
        let nested = models.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let guard = PathGuard::new(vec![models.clone()]);
        let sneaky = nested.join("../../outside");
        std::fs::write(dir.path().join("outside"), b"x").unwrap();
        let via = nested.join("../../outside");
        let res = guard.resolve_existing(&via);
        assert!(res.is_err(), "{sneaky:?} should be rejected");
    }

    #[test]
    fn validates_file_names() {
        assert!(validate_file_name("qwen2.5-7b-q4_k_m.gguf").is_ok());
        assert!(validate_file_name("../escape.gguf").is_err());
        assert!(validate_file_name("a;rm -rf.gguf").is_err());
        assert!(validate_file_name("").is_err());
        assert!(validate_file_name(".hidden").is_err());
    }

    #[test]
    fn checksum_matches_known_value() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"abc").unwrap();
        let (hash, len) = sha256_file(&file).unwrap();
        assert_eq!(len, 3);
        assert_eq!(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}

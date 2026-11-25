//! Model summary: the metadata ATOM shows and reasons about. Everything here is
//! read from the file; nothing is guessed.

use crate::gguf::GgufFile;
use crate::quant::{ftype_name, ggml_type, compatibility, QuantizeCompatibility};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelSummary {
    pub architecture: Option<String>,
    pub name: Option<String>,
    pub license: Option<String>,
    pub author: Option<String>,
    pub tokenizer: Option<String>,
    pub file_type: Option<u32>,
    pub declared_quant: Option<String>,
    /// Display label: the declared file type when present, else the dominant type.
    pub label_quant: String,
    pub dominant_type_id: u32,
    pub dominant_quant: String,
    pub tensor_count: usize,
    pub parameter_count: u64,
    pub weights_bytes: u64,
    pub context_length: Option<u64>,
    pub embedding_length: Option<u64>,
    pub block_count: Option<u64>,
    pub head_count: Option<u64>,
    pub head_count_kv: Option<u64>,
    pub vocab_size: Option<u64>,
    pub kv_bytes_per_token: Option<u64>,
    pub gguf_version: u32,
    pub alignment: u64,
    pub compatibility: QuantizeCompatibility,
    pub tensor_types: Vec<TensorTypeShare>,
    /// A short, factual description of the memory the file implies at runtime.
    pub memory_note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TensorTypeShare {
    pub id: u32,
    pub name: String,
    pub elements: u64,
    pub share: f64,
}

impl ModelSummary {
    pub fn from_gguf(gguf: &GgufFile, _file_type_hint: u32) -> Self {
        let arch = gguf.architecture().map(|s| s.to_string());
        let prefix = arch.clone().unwrap_or_else(|| "llama".into());
        let get = |suffix: &str| gguf.get_u64(&format!("{prefix}.{suffix}"));

        let context_length = get("context_length");
        let embedding_length = get("embedding_length");
        let block_count = get("block_count");
        let head_count = get("attention.head_count");
        let head_count_kv = get("attention.head_count_kv");

        let file_type = gguf.get_u64("general.file_type").map(|v| v as u32);
        let declared_quant = file_type.and_then(ftype_name).map(|s| s.to_string());

        let by_type = gguf.elements_per_type();
        let dominant_type_id = by_type.first().map(|(id, _)| *id).unwrap_or(0);
        let dominant_quant = ggml_type(dominant_type_id)
            .map(|t| t.name.to_string())
            .unwrap_or_else(|| format!("type_{dominant_type_id}"));

        let total_elements: u64 = by_type.iter().map(|(_, e)| *e).sum();
        let tensor_types = by_type
            .iter()
            .map(|(id, elements)| TensorTypeShare {
                id: *id,
                name: ggml_type(*id).map(|t| t.name.to_string()).unwrap_or_else(|| format!("type_{id}")),
                elements: *elements,
                share: if total_elements > 0 { *elements as f64 / total_elements as f64 } else { 0.0 },
            })
            .collect();

        let vocab_size = gguf
            .get("tokenizer.ggml.tokens")
            .and_then(|v| match v {
                crate::gguf::GgufValue::Array(items) => Some(items.len() as u64),
                _ => None,
            });

        let kv_bytes_per_token = match (block_count, head_count, embedding_length) {
            (Some(layers), Some(heads), Some(emb)) if heads > 0 => {
                let head_dim = emb / heads;
                let kv_heads = head_count_kv.unwrap_or(heads);
                // K and V, f16 (2 bytes) per element.
                Some(layers * kv_heads * head_dim * 2 * 2)
            }
            _ => None,
        };

        let weights_bytes = gguf.weight_bytes();
        let compat = compatibility(dominant_type_id, declared_quant.as_deref().unwrap_or(&dominant_quant));

        let memory_note = match (weights_bytes, kv_bytes_per_token) {
            (0, _) => "Tensor byte layout is unknown for this file; runtime memory cannot be estimated from metadata.".into(),
            (w, Some(kv)) => format!(
                "Weights ≈ {:.2} GiB on disk. KV cache ≈ {:.1} MiB per 1k tokens at f16.",
                w as f64 / 1024.0 / 1024.0 / 1024.0,
                kv as f64 * 1000.0 / 1024.0 / 1024.0
            ),
            (w, None) => format!(
                "Weights ≈ {:.2} GiB on disk. KV cache size cannot be estimated: attention head metadata is missing.",
                w as f64 / 1024.0 / 1024.0 / 1024.0
            ),
        };

        let label_quant = declared_quant.clone().unwrap_or_else(|| dominant_quant.clone());

        ModelSummary {
            architecture: arch,
            name: gguf.get_str("general.name").map(|s| s.to_string()),
            license: gguf.get_str("general.license").map(|s| s.to_string()),
            author: gguf.get_str("general.author").map(|s| s.to_string()),
            tokenizer: gguf.get_str("tokenizer.ggml.model").map(|s| s.to_string()),
            file_type,
            declared_quant,
            label_quant,
            dominant_type_id,
            dominant_quant,
            tensor_count: gguf.tensors.len(),
            parameter_count: total_elements,
            weights_bytes,
            context_length,
            embedding_length,
            block_count,
            head_count,
            head_count_kv,
            vocab_size,
            kv_bytes_per_token,
            gguf_version: gguf.version,
            alignment: gguf.alignment,
            compatibility: compat,
            tensor_types,
            memory_note,
        }
    }

    /// Quantization label for display: declared file type wins, else the
    /// dominant tensor type.
    pub fn label_quant(&self) -> String {
        self.label_quant.clone()
    }

    pub fn requires_requantize(&self) -> bool {
        self.compatibility.requires_requantize_flag
    }

    /// Estimated weight bytes after quantizing to `bpw` bits per weight.
    /// Per-tensor element counts are used, so this tracks llama-quantize's real
    /// output closely without pretending to be exact.
    pub fn estimate_quantized_bytes(&self, bpw: f64) -> u64 {
        let elements = self.parameter_count as f64;
        let bytes = elements * bpw / 8.0;
        (bytes as u64).saturating_add(self.tensor_count as u64 * 32)
    }

    /// Approximate KV cache bytes for a context length and per-element size.
    pub fn estimate_kv_bytes(&self, ctx: u64, bytes_per_element: f64) -> Option<u64> {
        let per_token = self.kv_bytes_per_token?;
        // kv_bytes_per_token is expressed for f16 (2 bytes per element).
        let scale = bytes_per_element / 2.0;
        Some((per_token as f64 * scale * ctx as f64) as u64)
    }
}

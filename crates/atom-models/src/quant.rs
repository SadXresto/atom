//! GGML tensor type table, llama.cpp file-type mapping, and the quantization
//! format catalog ATOM offers.

use serde::{Deserialize, Serialize};

/// One ggml tensor type: id, display name, block element count, bytes per block.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GgmlType {
    pub id: u32,
    pub name: &'static str,
    pub block: u64,
    pub size: u64,
}

impl GgmlType {
    pub fn bits_per_weight(&self) -> f64 {
        (self.size as f64 * 8.0) / self.block as f64
    }

    pub fn is_float(&self) -> bool {
        matches!(self.name, "F32" | "F16" | "BF16" | "F64")
    }
}

pub const GGML_TYPE_F32: u32 = 0;
pub const GGML_TYPE_F16: u32 = 1;
pub const GGML_TYPE_Q8_0: u32 = 8;
pub const GGML_TYPE_Q4_K: u32 = 12;
pub const GGML_TYPE_BF16: u32 = 30;

const TABLE: &[GgmlType] = &[
    GgmlType { id: 0, name: "F32", block: 1, size: 4 },
    GgmlType { id: 1, name: "F16", block: 1, size: 2 },
    GgmlType { id: 2, name: "Q4_0", block: 32, size: 18 },
    GgmlType { id: 3, name: "Q4_1", block: 32, size: 20 },
    GgmlType { id: 6, name: "Q5_0", block: 32, size: 22 },
    GgmlType { id: 7, name: "Q5_1", block: 32, size: 24 },
    GgmlType { id: 8, name: "Q8_0", block: 32, size: 34 },
    GgmlType { id: 9, name: "Q8_1", block: 32, size: 36 },
    GgmlType { id: 10, name: "Q2_K", block: 256, size: 84 },
    GgmlType { id: 11, name: "Q3_K", block: 256, size: 110 },
    GgmlType { id: 12, name: "Q4_K", block: 256, size: 144 },
    GgmlType { id: 13, name: "Q5_K", block: 256, size: 176 },
    GgmlType { id: 14, name: "Q6_K", block: 256, size: 210 },
    GgmlType { id: 15, name: "Q8_K", block: 256, size: 292 },
    GgmlType { id: 20, name: "IQ4_NL", block: 32, size: 18 },
    GgmlType { id: 23, name: "IQ4_XS", block: 256, size: 136 },
    GgmlType { id: 24, name: "I8", block: 1, size: 1 },
    GgmlType { id: 25, name: "I16", block: 1, size: 2 },
    GgmlType { id: 26, name: "I32", block: 1, size: 4 },
    GgmlType { id: 27, name: "I64", block: 1, size: 8 },
    GgmlType { id: 28, name: "F64", block: 1, size: 8 },
    GgmlType { id: 30, name: "BF16", block: 1, size: 2 },
];

pub fn ggml_type(id: u32) -> Option<GgmlType> {
    TABLE.iter().copied().find(|t| t.id == id)
}

pub fn ggml_type_by_name(name: &str) -> Option<GgmlType> {
    let upper = name.to_uppercase();
    TABLE.iter().copied().find(|t| t.name == upper)
}

/// `general.file_type` values from llama.cpp's `llama_ftype`.
pub const FTYPE_F32: u32 = 0;
pub const FTYPE_F16: u32 = 1;
pub const FTYPE_Q4_0: u32 = 2;
pub const FTYPE_Q4_1: u32 = 3;
pub const FTYPE_Q8_0: u32 = 7;
pub const FTYPE_Q5_0: u32 = 8;
pub const FTYPE_Q5_1: u32 = 9;
pub const FTYPE_Q2_K: u32 = 10;
pub const FTYPE_Q3_K_S: u32 = 11;
pub const FTYPE_Q3_K_M: u32 = 12;
pub const FTYPE_Q3_K_L: u32 = 13;
pub const FTYPE_Q4_K_S: u32 = 14;
pub const FTYPE_Q4_K_M: u32 = 15;
pub const FTYPE_Q5_K_S: u32 = 16;
pub const FTYPE_Q5_K_M: u32 = 17;
pub const FTYPE_Q6_K: u32 = 18;
pub const FTYPE_IQ2_XXS: u32 = 19;
pub const FTYPE_IQ2_XS: u32 = 20;
pub const FTYPE_Q2_K_S: u32 = 21;
pub const FTYPE_IQ3_XS: u32 = 22;
pub const FTYPE_IQ3_S: u32 = 23;
pub const FTYPE_IQ2_S: u32 = 24;
pub const FTYPE_IQ4_NL: u32 = 25;
pub const FTYPE_IQ3_M: u32 = 26;
pub const FTYPE_BF16: u32 = 32;
pub const FTYPE_F16_F32: u32 = 33;

pub fn ftype_name(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "IQ2_XXS",
        20 => "IQ2_XS",
        21 => "Q2_K_S",
        22 => "IQ3_XS",
        23 => "IQ3_S",
        24 => "IQ2_S",
        25 => "IQ4_NL",
        26 => "IQ3_M",
        32 => "BF16",
        33 => "F16_F32",
        _ => return None,
    })
}

/// A quantization target ATOM can request from llama-quantize.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantFormat {
    pub name: &'static str,
    /// Bits per weight used for the size estimate (llama.cpp block layouts).
    pub bpw: f64,
    /// Base type family used to match the installed tool's help output.
    pub family: &'static str,
    pub label: &'static str,
    pub tradeoff: &'static str,
    pub quality_tier: &'static str,
}

/// Ordered from highest fidelity to smallest output. Formats the installed tool
/// does not advertise are filtered out by `atom-compression`.
pub const CATALOG: &[QuantFormat] = &[
    QuantFormat {
        name: "Q8_0",
        bpw: 8.5,
        family: "Q8_0",
        label: "8-bit (near-lossless)",
        tradeoff: "Largest output, smallest quality change. Good archive format.",
        quality_tier: "highest",
    },
    QuantFormat {
        name: "Q6_K",
        bpw: 6.5625,
        family: "Q6_K",
        label: "6-bit K-quant",
        tradeoff: "Very close to fp16 quality at ~40% of the size.",
        quality_tier: "very high",
    },
    QuantFormat {
        name: "Q5_K_M",
        bpw: 5.6875,
        family: "Q5_K",
        label: "5-bit K-quant, medium",
        tradeoff: "Well-balanced; larger tensors keep higher precision.",
        quality_tier: "high",
    },
    QuantFormat {
        name: "Q5_K_S",
        bpw: 5.4375,
        family: "Q5_K",
        label: "5-bit K-quant, small",
        tradeoff: "Slightly smaller than Q5_K_M with a small quality cost.",
        quality_tier: "high",
    },
    QuantFormat {
        name: "Q4_K_M",
        bpw: 4.84375,
        family: "Q4_K",
        label: "4-bit K-quant, medium",
        tradeoff: "The usual default: ~4x smaller than fp16 with modest degradation.",
        quality_tier: "good",
    },
    QuantFormat {
        name: "Q4_K_S",
        bpw: 4.5625,
        family: "Q4_K",
        label: "4-bit K-quant, small",
        tradeoff: "Smaller than Q4_K_M; expect a visible quality drop on small models.",
        quality_tier: "good",
    },
    QuantFormat {
        name: "Q3_K_M",
        bpw: 3.90625,
        family: "Q3_K",
        label: "3-bit K-quant, medium",
        tradeoff: "Aggressive. Noticeable degradation, common for very large models.",
        quality_tier: "fair",
    },
    QuantFormat {
        name: "Q3_K_S",
        bpw: 3.4375,
        family: "Q3_K",
        label: "3-bit K-quant, small",
        tradeoff: "Very aggressive; run the comparison lab before relying on it.",
        quality_tier: "fair",
    },
    QuantFormat {
        name: "Q2_K",
        bpw: 2.5625,
        family: "Q2_K",
        label: "2-bit K-quant",
        tradeoff: "Extreme compression. Reasoning and math degrade substantially.",
        quality_tier: "low",
    },
    QuantFormat {
        name: "Q4_0",
        bpw: 4.5,
        family: "Q4_0",
        label: "4-bit legacy",
        tradeoff: "Legacy layout; slower than K-quants on modern CPUs.",
        quality_tier: "fair",
    },
    QuantFormat {
        name: "Q5_0",
        bpw: 5.5,
        family: "Q5_0",
        label: "5-bit legacy",
        tradeoff: "Legacy layout, superseded by Q5_K variants.",
        quality_tier: "fair",
    },
];

pub fn format_catalog() -> &'static [QuantFormat] {
    CATALOG
}

pub fn format_by_name(name: &str) -> Option<&'static QuantFormat> {
    CATALOG.iter().find(|f| f.name.eq_ignore_ascii_case(name))
}

/// Why a source model can or cannot be handed straight to llama-quantize.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QuantizeCompatibility {
    pub quantizable: bool,
    pub requires_requantize_flag: bool,
    pub source_quant: String,
    pub reason: String,
}

pub fn compatibility(dominant_type: u32, source_quant: &str) -> QuantizeCompatibility {
    let name = ggml_type(dominant_type).map(|t| t.name).unwrap_or("unknown");
    match dominant_type {
        GGML_TYPE_F32 | GGML_TYPE_F16 | GGML_TYPE_BF16 => QuantizeCompatibility {
            quantizable: true,
            requires_requantize_flag: false,
            source_quant: source_quant.to_string(),
            reason: format!("{name} source tensors can be quantized directly by llama-quantize."),
        },
        GGML_TYPE_Q8_0 => QuantizeCompatibility {
            quantizable: true,
            requires_requantize_flag: true,
            source_quant: source_quant.to_string(),
            reason: "Already Q8_0. llama.cpp can re-quantize this, but quality loss compounds; \
                     dequantizing to F16 first is safer."
                .into(),
        },
        other => QuantizeCompatibility {
            quantizable: false,
            requires_requantize_flag: false,
            source_quant: source_quant.to_string(),
            reason: format!(
                "Source tensors are {}. llama-quantize needs F32, F16 or BF16 input (or Q8_0 with \
                 re-quantization). Converting {} back to F16 with llama.cpp's `convert_hf_to_gguf.py` \
                 from the original weights is the lossless path; re-quantizing this file would \
                 compound quantization error.",
                ggml_type(other).map(|t| t.name).unwrap_or("an unknown type"),
                name
            ),
        },
    }
}

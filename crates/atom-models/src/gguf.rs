//! Read-only GGUF header/metadata/tensor-information parser.
//!
//! This deliberately parses only the header, metadata and tensor descriptors.
//! Tensor payloads are never read, so inspecting an 8 GiB file is instant and
//! ATOM can never be tricked into executing data from a model file.

use atom_core::{AtomError, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufReader, Read};
use std::path::Path;

pub const GGUF_MAGIC: &[u8; 4] = b"GGUF";
const MAX_STRING: u64 = 64 * 1024 * 1024;
const MAX_COUNT: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum GgufValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
    Bool(bool),
    Str(String),
    Array(Vec<GgufValue>),
    U64(u64),
    I64(i64),
    F64(f64),
}

impl GgufValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            GgufValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            GgufValue::U8(v) => Some(*v as u64),
            GgufValue::U16(v) => Some(*v as u64),
            GgufValue::U32(v) => Some(*v as u64),
            GgufValue::U64(v) => Some(*v),
            GgufValue::I32(v) if *v >= 0 => Some(*v as u64),
            GgufValue::I64(v) if *v >= 0 => Some(*v as u64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            GgufValue::F32(v) => Some(*v as f64),
            GgufValue::F64(v) => Some(*v),
            other => other.as_u64().map(|v| v as f64),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            GgufValue::U8(_) => "uint8",
            GgufValue::I8(_) => "int8",
            GgufValue::U16(_) => "uint16",
            GgufValue::I16(_) => "int16",
            GgufValue::U32(_) => "uint32",
            GgufValue::I32(_) => "int32",
            GgufValue::F32(_) => "float32",
            GgufValue::Bool(_) => "bool",
            GgufValue::Str(_) => "string",
            GgufValue::Array(_) => "array",
            GgufValue::U64(_) => "uint64",
            GgufValue::I64(_) => "int64",
            GgufValue::F64(_) => "float64",
        }
    }

    /// Arrays are summarized as `<type>[n]` so the UI never tries to render a
    /// 128k-token vocabulary inline.
    pub fn preview(&self, max_chars: usize) -> String {
        match self {
            GgufValue::Str(s) => {
                let truncated: String = s.chars().take(max_chars).collect();
                if s.chars().count() > max_chars {
                    format!("{truncated}…")
                } else {
                    truncated
                }
            }
            GgufValue::Array(items) => {
                let kind = items.first().map(|v| v.type_name()).unwrap_or("empty");
                format!("{kind}[{}]", items.len())
            }
            GgufValue::Bool(v) => v.to_string(),
            other => other
                .as_f64()
                .map(|v| {
                    if v.fract() == 0.0 {
                        format!("{}", v as i64)
                    } else {
                        format!("{v:.4}")
                    }
                })
                .or_else(|| other.as_u64().map(|v| v.to_string()))
                .unwrap_or_else(|| format!("{:?}", other)),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TensorInfo {
    pub name: String,
    pub dims: Vec<u64>,
    pub ggml_type: u32,
    pub offset: u64,
}

impl TensorInfo {
    pub fn elements(&self) -> u64 {
        self.dims.iter().product::<u64>().max(1)
    }

    pub fn type_name(&self) -> String {
        match crate::quant::ggml_type(self.ggml_type) {
            Some(t) => t.name.to_string(),
            None => format!("type_{}", self.ggml_type),
        }
    }

    /// Byte size when the ggml block layout is known to ATOM.
    pub fn nbytes(&self) -> Option<u64> {
        let t = crate::quant::ggml_type(self.ggml_type)?;
        let elements = self.elements();
        Some(elements / t.block.max(1) * t.size)
    }
}

#[derive(Clone, Debug)]
pub struct GgufFile {
    pub version: u32,
    pub metadata: Vec<(String, GgufValue)>,
    pub tensors: Vec<TensorInfo>,
    pub alignment: u64,
    pub data_offset: u64,
    pub file_len: u64,
}

impl GgufFile {
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut reader = BufReader::with_capacity(256 * 1024, file);
        let mut parsed = parse(&mut reader)?;
        parsed.file_len = file_len;
        Ok(parsed)
    }

    pub fn get(&self, key: &str) -> Option<&GgufValue> {
        self.metadata.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.get(key).and_then(|v| v.as_u64())
    }

    pub fn architecture(&self) -> Option<&str> {
        self.get_str("general.architecture")
    }

    /// Byte sum of known-layout tensors. Used for the quantization size estimate.
    pub fn weight_bytes(&self) -> u64 {
        self.tensors.iter().filter_map(|t| t.nbytes()).sum()
    }

    pub fn elements_per_type(&self) -> Vec<(u32, u64)> {
        let mut acc: std::collections::BTreeMap<u32, u64> = std::collections::BTreeMap::new();
        for t in &self.tensors {
            *acc.entry(t.ggml_type).or_default() += t.elements();
        }
        let mut v: Vec<(u32, u64)> = acc.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    }
}

fn err(msg: impl Into<String>) -> AtomError {
    AtomError::Parse(msg.into())
}

fn read_u8<R: Read>(r: &mut R) -> Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_exact<R: Read, const N: usize>(r: &mut R) -> Result<[u8; N]> {
    let mut b = [0u8; N];
    r.read_exact(&mut b)?;
    Ok(b)
}

fn read_u32<R: Read>(r: &mut R) -> Result<u32> {
    Ok(u32::from_le_bytes(read_exact::<R, 4>(r)?))
}

fn read_u64<R: Read>(r: &mut R) -> Result<u64> {
    Ok(u64::from_le_bytes(read_exact::<R, 8>(r)?))
}

fn read_string<R: Read>(r: &mut R) -> Result<String> {
    let len = read_u64(r)?;
    if len > MAX_STRING {
        return Err(err(format!("metadata string length {len} exceeds the safety limit")));
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|_| err("metadata string is not valid UTF-8"))
}

fn read_value<R: Read>(r: &mut R, vtype: u32, depth: usize) -> Result<GgufValue> {
    if depth > 4 {
        return Err(err("metadata arrays are nested too deeply"));
    }
    Ok(match vtype {
        0 => GgufValue::U8(read_u8(r)?),
        1 => GgufValue::I8(read_u8(r)? as i8),
        2 => GgufValue::U16(u16::from_le_bytes(read_exact::<R, 2>(r)?)),
        3 => GgufValue::I16(i16::from_le_bytes(read_exact::<R, 2>(r)?)),
        4 => GgufValue::U32(read_u32(r)?),
        5 => GgufValue::I32(read_u32(r)? as i32),
        6 => GgufValue::F32(f32::from_bits(read_u32(r)?)),
        7 => GgufValue::Bool(read_u8(r)? != 0),
        8 => GgufValue::Str(read_string(r)?),
        9 => {
            let elem_type = read_u32(r)?;
            let count = read_u64(r)?;
            if count > MAX_COUNT {
                return Err(err(format!("metadata array of {count} elements exceeds the safety limit")));
            }
            let mut items = Vec::with_capacity(count.min(4096) as usize);
            for _ in 0..count {
                items.push(read_value(r, elem_type, depth + 1)?);
            }
            GgufValue::Array(items)
        }
        10 => GgufValue::U64(read_u64(r)?),
        11 => GgufValue::I64(read_u64(r)? as i64),
        12 => GgufValue::F64(f64::from_bits(read_u64(r)?)),
        other => return Err(err(format!("unknown GGUF metadata value type {other}"))),
    })
}

fn parse<R: Read>(r: &mut R) -> Result<GgufFile> {
    let magic = read_exact::<R, 4>(r)?;
    if &magic != GGUF_MAGIC {
        return Err(err("not a GGUF file (missing GGUF magic)"));
    }
    let version = read_u32(r)?;
    if !(1..=3).contains(&version) {
        return Err(err(format!("unsupported GGUF version {version} (ATOM reads versions 1-3)")));
    }
    let tensor_count = read_u64(r)?;
    let kv_count = read_u64(r)?;
    if tensor_count > MAX_COUNT || kv_count > MAX_COUNT {
        return Err(err("GGUF header declares an implausible tensor/metadata count"));
    }

    let mut metadata = Vec::with_capacity(kv_count.min(2048) as usize);
    for _ in 0..kv_count {
        let key = read_string(r)?;
        if key.is_empty() || key.len() > 1024 {
            return Err(err("metadata key is empty or too long"));
        }
        let vtype = read_u32(r)?;
        let value = read_value(r, vtype, 0)?;
        metadata.push((key, value));
    }

    let mut tensors = Vec::with_capacity(tensor_count.min(8192) as usize);
    for _ in 0..tensor_count {
        let name = read_string(r)?;
        let n_dims = read_u32(r)?;
        if n_dims > 8 {
            return Err(err(format!("tensor '{name}' declares {n_dims} dimensions")));
        }
        let mut dims = Vec::with_capacity(n_dims as usize);
        for _ in 0..n_dims {
            dims.push(read_u64(r)?);
        }
        let ggml_type = read_u32(r)?;
        let offset = read_u64(r)?;
        tensors.push(TensorInfo { name, dims, ggml_type, offset });
    }

    let alignment = metadata
        .iter()
        .find(|(k, _)| k == "general.alignment")
        .and_then(|(_, v)| v.as_u64())
        .filter(|a| a.is_power_of_two() && *a >= 8 && *a <= 4096)
        .unwrap_or(32);

    // The data section starts after the descriptor table, aligned up.
    let mut consumed: u64 = 4 + 4 + 8 + 8;
    consumed += metadata
        .iter()
        .map(|(k, v)| 8 + k.len() as u64 + 4 + value_size(v))
        .sum::<u64>();
    consumed += tensors
        .iter()
        .map(|t| 8 + t.name.len() as u64 + 4 + 8 * t.dims.len() as u64 + 4 + 8)
        .sum::<u64>();
    let data_offset = consumed.div_ceil(alignment) * alignment;

    Ok(GgufFile { version, metadata, tensors, alignment, data_offset, file_len: 0 })
}

/// Serialized size of a metadata value, used to locate the tensor data section.
fn value_size(v: &GgufValue) -> u64 {
    match v {
        GgufValue::U8(_) | GgufValue::I8(_) | GgufValue::Bool(_) => 1,
        GgufValue::U16(_) | GgufValue::I16(_) => 2,
        GgufValue::U32(_) | GgufValue::I32(_) | GgufValue::F32(_) => 4,
        GgufValue::U64(_) | GgufValue::I64(_) | GgufValue::F64(_) => 8,
        GgufValue::Str(s) => 8 + s.len() as u64,
        GgufValue::Array(items) => 12 + items.iter().map(value_size).sum::<u64>(),
    }
}

pub mod fixture {
    //! Builds a tiny but structurally valid GGUF file for parser tests and for
    //! downstream crates that need a real GGUF on disk without a model download.

    use super::*;

    pub struct Builder {
        buf: Vec<u8>,
        metadata: Vec<(String, GgufValue)>,
        tensors: Vec<TensorInfo>,
    }

    impl Builder {
        pub fn new() -> Self {
            Builder { buf: Vec::new(), metadata: Vec::new(), tensors: Vec::new() }
        }

        fn u32v(&mut self, v: u32) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }

        fn u64v(&mut self, v: u64) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }

        fn strv(&mut self, s: &str) {
            self.u64v(s.len() as u64);
            self.buf.extend_from_slice(s.as_bytes());
        }

        fn value(&mut self, v: &GgufValue) {
            match v {
                GgufValue::U32(x) => {
                    self.u32v(4);
                    self.u32v(*x);
                }
                GgufValue::U64(x) => {
                    self.u32v(10);
                    self.u64v(*x);
                }
                GgufValue::Str(x) => {
                    self.u32v(8);
                    self.strv(x);
                }
                GgufValue::Array(items) => {
                    self.u32v(9);
                    self.u32v(8);
                    self.u64v(items.len() as u64);
                    for it in items {
                        if let GgufValue::Str(s) = it {
                            self.strv(s);
                        }
                    }
                }
                other => panic!("fixture builder does not support {:?}", other),
            }
        }

        pub fn meta(mut self, key: &str, value: GgufValue) -> Self {
            self.metadata.push((key.to_string(), value));
            self
        }

        pub fn tensor(mut self, name: &str, dims: &[u64], ggml_type: u32) -> Self {
            self.tensors.push(TensorInfo {
                name: name.to_string(),
                dims: dims.to_vec(),
                ggml_type,
                offset: 0,
            });
            self
        }

        pub fn build(self) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(GGUF_MAGIC);
            out.extend_from_slice(&3u32.to_le_bytes());
            out.extend_from_slice(&(self.tensors.len() as u64).to_le_bytes());
            out.extend_from_slice(&(self.metadata.len() as u64).to_le_bytes());
            for (k, v) in &self.metadata {
                out.extend_from_slice(&(k.len() as u64).to_le_bytes());
                out.extend_from_slice(k.as_bytes());
                let mut b = Builder::new();
                b.value(v);
                out.extend_from_slice(&b.buf);
            }
            for t in &self.tensors {
                out.extend_from_slice(&(t.name.len() as u64).to_le_bytes());
                out.extend_from_slice(t.name.as_bytes());
                out.extend_from_slice(&(t.dims.len() as u32).to_le_bytes());
                for d in &t.dims {
                    out.extend_from_slice(&d.to_le_bytes());
                }
                out.extend_from_slice(&t.ggml_type.to_le_bytes());
                out.extend_from_slice(&t.offset.to_le_bytes());
            }
            while out.len() % 32 != 0 {
                out.push(0);
            }
            out.extend_from_slice(&vec![0u8; 64]);
            out
        }
    }

    pub fn tiny_model(ggml_type: u32) -> Vec<u8> {
        Builder::new()
            .meta("general.architecture", GgufValue::Str("llama".into()))
            .meta("general.name", GgufValue::Str("atom-test-tiny".into()))
            .meta("general.license", GgufValue::Str("apache-2.0".into()))
            .meta("llama.context_length", GgufValue::U32(2048))
            .meta("llama.embedding_length", GgufValue::U32(64))
            .meta("llama.block_count", GgufValue::U32(2))
            .meta("llama.attention.head_count", GgufValue::U32(4))
            .meta("llama.attention.head_count_kv", GgufValue::U32(2))
            .meta("tokenizer.ggml.model", GgufValue::Str("llama".into()))
            .meta(
                "tokenizer.ggml.tokens",
                GgufValue::Array(vec![
                    GgufValue::Str("<unk>".into()),
                    GgufValue::Str("hello".into()),
                    GgufValue::Str("world".into()),
                ]),
            )
            .tensor("token_embd.weight", &[64, 3], ggml_type)
            .tensor("blk.0.attn_q.weight", &[64, 64], ggml_type)
            .tensor("blk.0.attn_k.weight", &[32, 64], ggml_type)
            .tensor("output.weight", &[64, 3], ggml_type)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{tiny_model, Builder};
    use super::*;
    use crate::quant::{FTYPE_F16, GGML_TYPE_F16, GGML_TYPE_Q4_K};

    #[test]
    fn parses_header_metadata_and_tensors() {
        let bytes = tiny_model(GGML_TYPE_F16);
        let mut cur = std::io::Cursor::new(bytes);
        let gguf = parse(&mut cur).unwrap();
        assert_eq!(gguf.version, 3);
        assert_eq!(gguf.tensors.len(), 4);
        assert_eq!(gguf.architecture(), Some("llama"));
        assert_eq!(gguf.get_str("general.name"), Some("atom-test-tiny"));
        assert_eq!(gguf.get_u64("llama.context_length"), Some(2048));
        assert_eq!(gguf.alignment, 32);
        // 64*3*2 + 64*64*2 + 32*64*2 + 64*3*2
        let expected = (64u64 * 3 + 64 * 64 + 32 * 64 + 64 * 3) * 2;
        assert_eq!(gguf.weight_bytes(), expected);
    }

    #[test]
    fn rejects_non_gguf() {
        let mut cur = std::io::Cursor::new(b"NOTG".to_vec());
        let err = parse(&mut cur).unwrap_err();
        assert_eq!(err.code(), "parse_error");
    }

    #[test]
    fn rejects_absurd_metadata_count() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(GGUF_MAGIC);
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&u64::MAX.to_le_bytes());
        let mut cur = std::io::Cursor::new(bytes);
        assert!(parse(&mut cur).is_err());
    }

    #[test]
    fn summary_reads_architecture_details() {
        let bytes = tiny_model(GGML_TYPE_F16);
        let mut cur = std::io::Cursor::new(bytes);
        let gguf = parse(&mut cur).unwrap();
        let summary = crate::summary::ModelSummary::from_gguf(&gguf, GGML_TYPE_F16 as u32);
        assert_eq!(summary.architecture.as_deref(), Some("llama"));
        assert_eq!(summary.tensor_count, 4);
        assert_eq!(summary.parameter_count, 64 * 3 + 64 * 64 + 32 * 64 + 64 * 3);
        assert_eq!(summary.label_quant(), "F16");
        assert!(!summary.requires_requantize());
        let _ = FTYPE_F16;
        let _ = GGML_TYPE_Q4_K;
    }

    #[test]
    fn kv_cache_estimate_is_computed_from_metadata() {
        let bytes = tiny_model(GGML_TYPE_F16);
        let mut cur = std::io::Cursor::new(bytes);
        let gguf = parse(&mut cur).unwrap();
        let summary = crate::summary::ModelSummary::from_gguf(&gguf, 0);
        // layers 2 * 2 kv heads * (64/4 head_dim) * 2 (k and v) * 2 bytes
        assert_eq!(summary.kv_bytes_per_token, Some(2 * 2 * 16 * 2 * 2));
    }

    #[test]
    fn handles_empty_tensor_list() {
        let bytes = Builder::new()
            .meta("general.architecture", GgufValue::Str("llama".into()))
            .build();
        let mut cur = std::io::Cursor::new(bytes);
        let gguf = parse(&mut cur).unwrap();
        assert!(gguf.tensors.is_empty());
        let summary = crate::summary::ModelSummary::from_gguf(&gguf, 0);
        assert_eq!(summary.tensor_count, 0);
        assert_eq!(summary.parameter_count, 0);
    }
}

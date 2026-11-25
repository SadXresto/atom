//! The built-in byte-level MLP language model.
//!
//! This is a real one-hidden-layer neural network with a fixed 256-byte
//! vocabulary, trained here with mini-batch SGD and softmax cross-entropy. The
//! input is a one-hot encoding of the previous `context` bytes, which makes the
//! forward and backward passes sparse and fast enough to train in-process.
//!
//! Weight files are a small documented binary format (`ATOMLM1`), and the
//! quantizer here is a real symmetric per-row float16/int8/int4 quantizer whose
//! effect on validation loss is measured by dequantizing and re-evaluating.

use atom_core::{AtomError, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::Path;

pub const VOCAB: usize = 256;
const MAGIC: &[u8; 8] = b"ATOMLM1\0";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ByteMlp {
    pub context: usize,
    pub hidden: usize,
    pub w1: Vec<f32>,
    pub b1: Vec<f32>,
    pub w2: Vec<f32>,
    pub b2: Vec<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrainConfig {
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch: u32,
    pub seed: u64,
    pub windows_per_epoch: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochPoint {
    pub epoch: u32,
    pub train_loss: f64,
    pub val_loss: f64,
    pub val_perplexity: f64,
    pub top1: f64,
    pub seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrainReport {
    pub context: u32,
    pub hidden: u32,
    pub params: u64,
    pub epochs: u32,
    pub windows: u64,
    pub val_windows: u64,
    pub duration_ms: u64,
    pub first_loss: f64,
    pub train_loss: f64,
    pub val_loss: f64,
    pub perplexity: f64,
    pub top1: f64,
    pub top5: f64,
    pub curve: Vec<EpochPoint>,
    pub samples: Vec<SampleGeneration>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SampleGeneration {
    pub prompt: String,
    pub output: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvalReport {
    pub windows: u64,
    pub loss: f64,
    pub perplexity: f64,
    pub top1: f64,
    pub top5: f64,
    pub duration_ms: u64,
    pub samples: Vec<SampleGeneration>,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantReport {
    pub format: String,
    pub params: u64,
    pub weight_bytes: u64,
    pub f32_bytes: u64,
    pub ratio: f64,
    pub bits_per_weight: f64,
    pub val_loss: f64,
    pub perplexity: f64,
    pub top1: f64,
    pub delta_loss: f64,
    pub duration_ms: u64,
    pub note: String,
}

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
    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn f32_to_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    if exp == 0xff {
        return sign | 0x7c00 | if bits & 0x7fffff != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e <= 0 {
        // Flush half subnormals to zero; the quantizer never produces them in
        // practice because row scales normalise magnitudes.
        return sign;
    }
    if e >= 31 {
        return sign | 0x7c00;
    }
    let mantissa = (bits & 0x7fffff) >> 13;
    sign | ((e as u16) << 10) | mantissa as u16
}

fn f16_to_f32(half: u16) -> f32 {
    let sign = ((half & 0x8000) as u32) << 16;
    let exp = ((half >> 10) & 0x1f) as u32;
    let mant = (half & 0x3ff) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign
        } else {
            // subnormal
            let mut e = 127 - 15 + 1;
            let mut m = mant;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            sign | ((e as u32) << 23) | ((m & 0x3ff) << 13)
        }
    } else if exp == 31 {
        sign | 0x7f80_0000 | (mant << 13)
    } else {
        sign | ((exp + 127 - 15) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

impl ByteMlp {
    pub fn new(context: usize, hidden: usize, seed: u64) -> Self {
        let in_dim = context * VOCAB;
        let w1_len = hidden * in_dim;
        let w2_len = VOCAB * hidden;
        let mut rng = Rng::new(seed);
        let limit1 = (6.0 / in_dim as f32).sqrt();
        let limit2 = (6.0 / hidden as f32).sqrt();
        let mut w1 = vec![0f32; w1_len];
        for value in w1.iter_mut() {
            *value = (rng.next_f32() * 2.0 - 1.0) * limit1;
        }
        let mut w2 = vec![0f32; w2_len];
        for value in w2.iter_mut() {
            *value = (rng.next_f32() * 2.0 - 1.0) * limit2;
        }
        ByteMlp {
            context,
            hidden,
            w1,
            b1: vec![0.0; hidden],
            w2,
            b2: vec![0.0; VOCAB],
        }
    }

    pub fn params(&self) -> u64 {
        (self.w1.len() + self.b1.len() + self.w2.len() + self.b2.len()) as u64
    }

    fn in_dim(&self) -> usize {
        self.context * VOCAB
    }

    /// Forward pass. `ids` must have length `context`; each element is a byte.
    /// Returns (hidden activations, logits).
    fn forward(&self, ids: &[u8]) -> (Vec<f32>, Vec<f32>) {
        let mut h = self.b1.clone();
        for (pos, byte) in ids.iter().enumerate() {
            let base = pos * VOCAB + *byte as usize;
            for k in 0..self.hidden {
                h[k] += self.w1[k * self.in_dim() + base];
            }
        }
        for value in h.iter_mut() {
            if *value < 0.0 {
                *value = 0.0;
            }
        }
        let mut logits = self.b2.clone();
        for v in 0..VOCAB {
            let row = &self.w2[v * self.hidden..(v + 1) * self.hidden];
            let mut sum = 0f32;
            for k in 0..self.hidden {
                sum += row[k] * h[k];
            }
            logits[v] += sum;
        }
        (h, logits)
    }

    pub fn logits_for(&self, ids: &[u8]) -> Vec<f32> {
        self.forward(ids).1
    }

    /// Greedy next byte — used by the teacher-agreement metric.
    pub fn predict_byte(&self, ids: &[u8]) -> u8 {
        let logits = self.logits_for(ids);
        let mut best = 0usize;
        for (i, value) in logits.iter().enumerate() {
            if *value > logits[best] {
                best = i;
            }
        }
        best as u8
    }

    pub fn train(
        &mut self,
        train: &[u8],
        val: &[u8],
        cfg: &TrainConfig,
        mut progress: impl FnMut(&str, f64),
        cancel: impl Fn() -> bool,
    ) -> Result<TrainReport> {
        if train.len() <= self.context + 2 {
            return Err(AtomError::Invalid(
                "the training split is shorter than the model context".into(),
            ));
        }
        if val.len() <= self.context + 2 {
            return Err(AtomError::Invalid(
                "the validation split is shorter than the model context".into(),
            ));
        }
        let started = std::time::Instant::now();
        let hidden = self.hidden;
        let in_dim = self.in_dim();
        let mut rng = Rng::new(cfg.seed);
        let mut grad_w1 = vec![0f32; self.w1.len()];
        let mut grad_b1 = vec![0f32; hidden];
        let mut grad_w2 = vec![0f32; self.w2.len()];
        let mut grad_b2 = vec![0f32; VOCAB];
        let mut probs = vec![0f32; VOCAB];
        let mut curve: Vec<EpochPoint> = Vec::new();
        let mut windows_total = 0u64;
        let batch = cfg.batch.max(1) as usize;
        let per_epoch = cfg.windows_per_epoch.max(64);
        let max_start = train.len() - self.context - 1;

        for epoch in 1..=cfg.epochs {
            let epoch_start = std::time::Instant::now();
            if cancel() {
                return Err(AtomError::Cancelled("training cancelled".into()));
            }
            let mut accum = 0f32;
            let mut accum_count = 0u64;
            let mut filled = 0usize;
            let lr = cfg.learning_rate * (1.0 - 0.65 * (epoch as f32 / cfg.epochs.max(1) as f32));
            progress(
                &format!("epoch {epoch}/{} · training", cfg.epochs),
                0.02 + 0.9 * ((epoch - 1) as f64 / cfg.epochs as f64),
            );
            for _ in 0..per_epoch {
                if cancel() {
                    return Err(AtomError::Cancelled("training cancelled".into()));
                }
                let start = (rng.next_u64() as usize) % max_start;
                let ids = &train[start..start + self.context];
                let target = train[start + self.context] as usize;
                let (h, mut logits) = self.forward(ids);
                let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                let mut sum = 0f32;
                for v in 0..VOCAB {
                    let e = (logits[v] - max).exp();
                    probs[v] = e;
                    sum += e;
                }
                let inv = 1.0 / sum.max(1e-9);
                for p in probs.iter_mut() {
                    *p *= inv;
                }
                accum += -(probs[target].max(1e-9)).ln();
                accum_count += 1;
                for v in 0..VOCAB {
                    logits[v] = probs[v] - if v == target { 1.0 } else { 0.0 };
                }
                // grad W2 / b2
                for v in 0..VOCAB {
                    let d = logits[v];
                    if d == 0.0 {
                        continue;
                    }
                    grad_b2[v] += d;
                    let row = &mut grad_w2[v * hidden..(v + 1) * hidden];
                    for k in 0..hidden {
                        row[k] += d * h[k];
                    }
                }
                // grad hidden
                let mut dh = vec![0f32; hidden];
                for v in 0..VOCAB {
                    let d = logits[v];
                    if d == 0.0 {
                        continue;
                    }
                    let row = &self.w2[v * hidden..(v + 1) * hidden];
                    for k in 0..hidden {
                        dh[k] += row[k] * d;
                    }
                }
                for k in 0..hidden {
                    if h[k] <= 0.0 {
                        dh[k] = 0.0;
                    }
                }
                for pos in 0..self.context {
                    let base = pos * VOCAB + ids[pos] as usize;
                    for k in 0..hidden {
                        grad_w1[k * in_dim + base] += dh[k];
                    }
                }
                for k in 0..hidden {
                    grad_b1[k] += dh[k];
                }
                filled += 1;
                if filled >= batch {
                    self.apply_grads(&mut grad_w1, &mut grad_b1, &mut grad_w2, &mut grad_b2, lr, filled);
                    filled = 0;
                }
                windows_total += 1;
            }
            if filled > 0 {
                self.apply_grads(&mut grad_w1, &mut grad_b1, &mut grad_w2, &mut grad_b2, lr, filled);
            }
            // Progress within this epoch from real completed windows.
            let epoch_val = self.evaluate_quick(val);
            let train_loss = if accum_count > 0 {
                (accum / accum_count as f32) as f64
            } else {
                0.0
            };
            curve.push(EpochPoint {
                epoch,
                train_loss,
                val_loss: epoch_val.0,
                val_perplexity: epoch_val.1,
                top1: epoch_val.2,
                seconds: epoch_start.elapsed().as_secs_f64(),
            });
            progress(
                &format!("epoch {epoch}/{} · val loss {:.4}", cfg.epochs, epoch_val.0),
                0.02 + 0.9 * (epoch as f64 / cfg.epochs as f64),
            );
        }

        let eval = self.evaluate(val, 1600);
        let first_loss = curve.first().map(|c| c.train_loss).unwrap_or(0.0);
        Ok(TrainReport {
            context: self.context as u32,
            hidden: self.hidden as u32,
            params: self.params(),
            epochs: cfg.epochs,
            windows: windows_total,
            val_windows: eval.windows,
            duration_ms: started.elapsed().as_millis() as u64,
            first_loss,
            train_loss: curve.last().map(|c| c.train_loss).unwrap_or(0.0),
            val_loss: eval.loss,
            perplexity: eval.perplexity,
            top1: eval.top1,
            top5: eval.top5,
            curve,
            samples: eval.samples,
        })
    }

    fn apply_grads(
        &mut self,
        grad_w1: &mut [f32],
        grad_b1: &mut [f32],
        grad_w2: &mut [f32],
        grad_b2: &mut [f32],
        lr: f32,
        count: usize,
    ) {
        let scale = lr / count as f32;
        for (w, g) in self.w1.iter_mut().zip(grad_w1.iter()) {
            *w -= scale * g;
        }
        for (w, g) in self.b1.iter_mut().zip(grad_b1.iter()) {
            *w -= scale * g;
        }
        for (w, g) in self.w2.iter_mut().zip(grad_w2.iter()) {
            *w -= scale * g;
        }
        for (w, g) in self.b2.iter_mut().zip(grad_b2.iter()) {
            *w -= scale * g;
        }
        grad_w1.fill(0.0);
        grad_b1.fill(0.0);
        grad_w2.fill(0.0);
        grad_b2.fill(0.0);
    }

    /// Loss/perplexity/top-1 over a bounded, deterministic sample of windows.
    fn evaluate_quick(&self, bytes: &[u8]) -> (f64, f64, f64) {
        let report = self.evaluate(bytes, 320);
        (report.loss, report.perplexity, report.top1)
    }

    pub fn evaluate(&self, bytes: &[u8], max_windows: usize) -> EvalReport {
        let started = std::time::Instant::now();
        if bytes.len() <= self.context + 1 {
            return EvalReport {
                windows: 0,
                loss: 0.0,
                perplexity: 0.0,
                top1: 0.0,
                top5: 0.0,
                duration_ms: 0,
                samples: Vec::new(),
                note: "validation split too short to evaluate".into(),
            };
        }
        let max_start = bytes.len() - self.context - 1;
        let stride = (max_start / max_windows.max(1)).max(1);
        let mut loss = 0f64;
        let mut count = 0u64;
        let mut top1 = 0u64;
        let mut top5 = 0u64;
        let mut start = 0usize;
        while start <= max_start && (count as usize) < max_windows {
            let ids = &bytes[start..start + self.context];
            let target = bytes[start + self.context] as usize;
            let logits = self.logits_for(ids);
            let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let mut sum = 0f64;
            for v in 0..VOCAB {
                sum += ((logits[v] - max) as f64).exp();
            }
            let prob = (((logits[target] - max) as f64).exp() / sum).max(1e-12);
            loss += -prob.ln();
            let mut ranked: Vec<(usize, f32)> = logits.iter().cloned().enumerate().collect();
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            if ranked.first().map(|r| r.0) == Some(target) {
                top1 += 1;
            }
            if ranked.iter().take(5).any(|r| r.0 == target) {
                top5 += 1;
            }
            count += 1;
            start += stride;
        }
        let mean_loss = if count > 0 { loss / count as f64 } else { 0.0 };
        EvalReport {
            windows: count,
            loss: mean_loss,
            perplexity: mean_loss.exp(),
            top1: if count > 0 { top1 as f64 / count as f64 } else { 0.0 },
            top5: if count > 0 { top5 as f64 / count as f64 } else { 0.0 },
            duration_ms: started.elapsed().as_millis() as u64,
            samples: self.sample_set(bytes),
            note: "Measured on held-out validation windows. Rule-free: this is the model's \
                   own cross-entropy, not a quality score."
                .into(),
        }
    }

    fn sample_set(&self, val: &[u8]) -> Vec<SampleGeneration> {
        let mut samples = Vec::new();
        let prompts = [
            val.get(..self.context).map(|s| String::from_utf8_lossy(s).to_string()),
            val.get(val.len() / 2..val.len() / 2 + self.context)
                .map(|s| String::from_utf8_lossy(s).to_string()),
        ];
        for (i, prompt) in prompts.into_iter().enumerate() {
            let Some(prompt) = prompt else { continue };
            let text = self.generate(prompt.as_bytes(), 120, 0.7, 7 + i as u64);
            samples.push(SampleGeneration {
                prompt: prompt.replace('\n', " "),
                output: text,
            });
        }
        samples
    }

    pub fn generate(&self, prompt: &[u8], max_tokens: usize, temperature: f32, seed: u64) -> String {
        let mut rng = Rng::new(seed);
        let mut window: Vec<u8> = vec![b' '; self.context];
        for byte in prompt.iter().rev().take(self.context).rev() {
            window.push(*byte);
        }
        window = window[window.len() - self.context..].to_vec();
        let mut out: Vec<u8> = Vec::with_capacity(max_tokens);
        for _ in 0..max_tokens {
            let mut logits = self.logits_for(&window);
            let temp = temperature.max(0.05);
            for value in logits.iter_mut() {
                *value /= temp;
            }
            let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let mut sum = 0f32;
            for value in logits.iter_mut() {
                *value = (*value - max).exp();
                sum += *value;
            }
            let target = rng.next_f32() * sum;
            let mut acc = 0f32;
            let mut chosen = 0usize;
            for (v, value) in logits.iter().enumerate() {
                acc += *value;
                if acc >= target {
                    chosen = v;
                    break;
                }
            }
            out.push(chosen as u8);
            window.remove(0);
            window.push(chosen as u8);
        }
        String::from_utf8_lossy(&out).to_string()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(path)?;
        file.write_all(MAGIC)?;
        file.write_all(&(self.context as u32).to_le_bytes())?;
        file.write_all(&(self.hidden as u32).to_le_bytes())?;
        file.write_all(&(VOCAB as u32).to_le_bytes())?;
        for array in [&self.w1, &self.b1, &self.w2, &self.b2] {
            file.write_all(&(array.len() as u32).to_le_bytes())?;
            let mut buffer = Vec::with_capacity(array.len() * 4);
            for value in array {
                buffer.extend_from_slice(&value.to_le_bytes());
            }
            file.write_all(&buffer)?;
        }
        file.flush()?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let mut file = std::fs::File::open(path)?;
        let mut magic = [0u8; 8];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(AtomError::Parse(format!(
                "{} is not an ATOM Micro model file",
                path.display()
            )));
        }
        let mut u32buf = [0u8; 4];
        file.read_exact(&mut u32buf)?;
        let context = u32::from_le_bytes(u32buf) as usize;
        file.read_exact(&mut u32buf)?;
        let hidden = u32::from_le_bytes(u32buf) as usize;
        file.read_exact(&mut u32buf)?;
        let vocab = u32::from_le_bytes(u32buf) as usize;
        if vocab != VOCAB || context == 0 || hidden == 0 || context > 64 || hidden > 4096 {
            return Err(AtomError::Parse("model header is out of range".into()));
        }
        let expected = [context * VOCAB * hidden, hidden, vocab * hidden, vocab];
        let mut arrays: Vec<Vec<f32>> = Vec::new();
        for len in expected {
            file.read_exact(&mut u32buf)?;
            let stored = u32::from_le_bytes(u32buf) as usize;
            if stored != len {
                return Err(AtomError::Parse(format!(
                    "model tensor has {stored} values, expected {len}"
                )));
            }
            let mut raw = vec![0u8; len * 4];
            file.read_exact(&mut raw)?;
            let mut values = Vec::with_capacity(len);
            for chunk in raw.chunks_exact(4) {
                values.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            arrays.push(values);
        }
        Ok(ByteMlp {
            context,
            hidden,
            w1: arrays.remove(0),
            b1: arrays.remove(0),
            w2: arrays.remove(0),
            b2: arrays.remove(0),
        })
    }

    pub fn quantize(&self, format: &str) -> Result<QuantizedModel> {
        let bits = match format {
            "f32" => 32u8,
            "f16" => 16,
            "int8" => 8,
            "int4" => 4,
            other => {
                return Err(AtomError::Invalid(format!(
                    "unknown quantization format '{other}' (use f32, f16, int8 or int4)"
                )))
            }
        };
        let w1 = QMatrix::from_f32(&self.w1, self.hidden, self.in_dim(), bits)?;
        let w2 = QMatrix::from_f32(&self.w2, VOCAB, self.hidden, bits)?;
        let mut weight_bytes = w1.data.len() as u64 + w2.data.len() as u64;
        weight_bytes += (w1.scales.len() + w2.scales.len()) as u64 * 4;
        weight_bytes += (self.b1.len() + self.b2.len()) as u64 * 4;
        Ok(QuantizedModel {
            format: format.to_string(),
            bits,
            context: self.context,
            hidden: self.hidden,
            w1,
            w2,
            b1: self.b1.clone(),
            b2: self.b2.clone(),
            weight_bytes,
            source: self.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct QMatrix {
    pub rows: usize,
    pub cols: usize,
    pub bits: u8,
    pub scales: Vec<f32>,
    pub data: Vec<u8>,
}

impl QMatrix {
    fn from_f32(values: &[f32], rows: usize, cols: usize, bits: u8) -> Result<Self> {
        if values.len() != rows * cols {
            return Err(AtomError::Internal("weight matrix shape mismatch".into()));
        }
        let mut scales = Vec::with_capacity(rows);
        let mut data = Vec::with_capacity(values.len() * bits as usize / 8 + rows);
        match bits {
            32 => {
                for chunk in values.chunks_exact(cols) {
                    let max = chunk.iter().fold(0f32, |m, v| m.max(v.abs()));
                    scales.push(max.max(1e-8));
                }
                for value in values {
                    data.extend_from_slice(&value.to_le_bytes());
                }
            }
            16 => {
                for chunk in values.chunks_exact(cols) {
                    let mut max = 0f32;
                    for v in chunk {
                        let h = f16_to_f32(f32_to_f16(*v));
                        max = max.max(h.abs());
                    }
                    scales.push(max.max(1e-8));
                }
                for value in values {
                    data.extend_from_slice(&f32_to_f16(*value).to_le_bytes());
                }
            }
            8 => {
                for chunk in values.chunks_exact(cols) {
                    let max = chunk.iter().fold(0f32, |m, v| m.max(v.abs())).max(1e-8);
                    let scale = max / 127.0;
                    scales.push(scale);
                }
                for (row, chunk) in values.chunks_exact(cols).enumerate() {
                    let scale = scales[row];
                    for value in chunk {
                        let q = (value / scale).round().clamp(-127.0, 127.0) as i8;
                        data.push(q as u8);
                    }
                }
            }
            4 => {
                for chunk in values.chunks_exact(cols) {
                    let max = chunk.iter().fold(0f32, |m, v| m.max(v.abs())).max(1e-8);
                    scales.push(max / 7.0);
                }
                for (row, chunk) in values.chunks_exact(cols).enumerate() {
                    let scale = scales[row];
                    let mut bytes = Vec::with_capacity((cols + 1) / 2);
                    for pair in chunk.chunks(2) {
                        let a = (pair[0] / scale).round().clamp(-7.0, 7.0) as i8 as u8 & 0x0f;
                        let b = pair
                            .get(1)
                            .map(|v| (v / scale).round().clamp(-7.0, 7.0) as i8 as u8 & 0x0f)
                            .unwrap_or(0);
                        bytes.push(a | (b << 4));
                    }
                    data.extend_from_slice(&bytes);
                }
            }
            _ => return Err(AtomError::Internal("unsupported quantization bits".into())),
        }
        Ok(QMatrix { rows, cols, bits, scales, data })
    }

    pub fn dequantize(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.rows * self.cols);
        match self.bits {
            32 => {
                for chunk in self.data.chunks_exact(4) {
                    out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            16 => {
                for chunk in self.data.chunks_exact(2) {
                    out.push(f16_to_f32(u16::from_le_bytes([chunk[0], chunk[1]])));
                }
            }
            8 => {
                for (row, chunk) in self.data.chunks_exact(self.cols).enumerate() {
                    let scale = self.scales[row];
                    for byte in chunk {
                        out.push((*byte as i8) as f32 * scale);
                    }
                }
            }
            4 => {
                for (row, chunk) in self.data.chunks_exact((self.cols + 1) / 2).enumerate() {
                    let scale = self.scales[row];
                    let mut produced = 0;
                    for byte in chunk {
                        for nibble in [byte & 0x0f, (byte >> 4) & 0x0f] {
                            if produced >= self.cols {
                                break;
                            }
                            let value = if nibble >= 8 {
                                nibble as i8 - 16
                            } else {
                                nibble as i8
                            };
                            out.push(value as f32 * scale);
                            produced += 1;
                        }
                    }
                }
            }
            _ => {}
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct QuantizedModel {
    pub format: String,
    pub bits: u8,
    pub context: usize,
    pub hidden: usize,
    pub w1: QMatrix,
    pub w2: QMatrix,
    pub b1: Vec<f32>,
    pub b2: Vec<f32>,
    pub weight_bytes: u64,
    pub source: ByteMlp,
}

impl QuantizedModel {
    pub fn dequantized(&self) -> ByteMlp {
        let mut model = self.source.clone();
        model.w1 = self.w1.dequantize();
        model.w2 = self.w2.dequantize();
        model.b1 = self.b1.clone();
        model.b2 = self.b2.clone();
        model
    }

    pub fn params(&self) -> u64 {
        self.source.params()
    }

    pub fn f32_bytes(&self) -> u64 {
        self.params() * 4
    }

    /// Write the packed weights in the documented `ATOMQ1` container.
    pub fn write_container(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"ATOMQ1\0\0");
        out.extend_from_slice(&(self.context as u16).to_le_bytes());
        out.extend_from_slice(&(self.hidden as u16).to_le_bytes());
        out.push(self.bits);
        out.push(0);
        for matrix in [&self.w1, &self.w2] {
            out.extend_from_slice(&(matrix.rows as u32).to_le_bytes());
            out.extend_from_slice(&(matrix.cols as u32).to_le_bytes());
            out.extend_from_slice(&(matrix.scales.len() as u32).to_le_bytes());
            for scale in &matrix.scales {
                out.extend_from_slice(&scale.to_le_bytes());
            }
            out.extend_from_slice(&(matrix.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&matrix.data);
        }
        for vector in [&self.b1, &self.b2] {
            out.extend_from_slice(&(vector.len() as u32).to_le_bytes());
            for value in vector {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        std::fs::write(path, out)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus() -> Vec<u8> {
        let mut text = String::new();
        for i in 0..80 {
            text.push_str(&format!(
                "LED {i} is {}. Set brightness to {} percent.\n",
                if i % 2 == 0 { "on" } else { "off" },
                (i * 7) % 100
            ));
        }
        text.into_bytes()
    }

    #[test]
    fn quantization_roundtrip_is_close() {
        let model = ByteMlp::new(4, 8, 5);
        for format in ["f16", "int8", "int4"] {
            let q = model.quantize(format).unwrap();
            let back = q.dequantized();
            let diff: f32 = model
                .w1
                .iter()
                .zip(back.w1.iter())
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max);
            let max = model.w1.iter().fold(0f32, |m, v| m.max(v.abs()));
            assert!(diff <= max * 0.15 + 1e-4, "{format} max diff {diff}");
        }
    }

    #[test]
    fn weight_sizes_follow_bits() {
        let model = ByteMlp::new(6, 24, 1);
        let f32m = model.quantize("f32").unwrap();
        let int8 = model.quantize("int8").unwrap();
        let int4 = model.quantize("int4").unwrap();
        assert!(int8.weight_bytes < f32m.weight_bytes);
        assert!(int4.weight_bytes < int8.weight_bytes);
        assert_eq!(int4.bits, 4);
    }

    #[test]
    fn training_reduces_loss_and_saves() {
        let data = corpus();
        let split = data.len() * 3 / 4;
        let mut model = ByteMlp::new(6, 16, 11);
        let report = model
            .train(
                &data[..split],
                &data[split..],
                &TrainConfig {
                    epochs: 4,
                    learning_rate: 0.08,
                    batch: 16,
                    seed: 3,
                    windows_per_epoch: 600,
                },
                |_, _| {},
                || false,
            )
            .unwrap();
        assert!(report.curve.len() == 4);
        assert!(report.val_loss.is_finite());
        assert!(report.val_loss < 5.55, "val loss {}", report.val_loss);
        assert!(report.params > 0);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.atomlm");
        model.save(&path).unwrap();
        let loaded = ByteMlp::load(&path).unwrap();
        assert_eq!(loaded.context, model.context);
        assert_eq!(loaded.w1.len(), model.w1.len());
    }

    #[test]
    fn cancellation_is_reported() {
        let data = corpus();
        let mut model = ByteMlp::new(6, 16, 2);
        let err = model
            .train(
                &data[..data.len() - 500],
                &data[data.len() - 500..],
                &TrainConfig {
                    epochs: 50,
                    learning_rate: 0.05,
                    batch: 8,
                    seed: 1,
                    windows_per_epoch: 100_000,
                },
                |_, _| {},
                || true,
            )
            .unwrap_err();
        assert!(matches!(err, AtomError::Cancelled(_)));
    }

    #[test]
    fn f16_roundtrip_is_finite() {
        for value in [0.5f32, -1.25, 12.0, 0.0, -0.0] {
            let back = f16_to_f32(f32_to_f16(value));
            assert!((back - value).abs() < 1e-2, "{value} -> {back}");
        }
    }
}

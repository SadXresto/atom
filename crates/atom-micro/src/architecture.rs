//! Model architecture catalog.
//!
//! Two families are offered, and the difference is stated plainly in the UI:
//!
//! * **Built-in byte MLP** — a real one-hidden-layer neural language model over
//!   a fixed 256-byte vocabulary. It is trained in this process by
//!   `crate::train`, so it works without Python or PyTorch.
//! * **Transformer adapter** — requires an external PyTorch toolchain. The
//!   workspace probes for it and reports precisely what is missing instead of
//!   pretending a transformer was trained.

use serde::Serialize;

const VOCAB: u64 = 256;

#[derive(Clone, Debug, Serialize)]
pub struct Architecture {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    pub summary: &'static str,
    pub context: u32,
    pub hidden: u32,
    pub layers: u32,
    pub trainable_here: bool,
    pub requires: Option<&'static str>,
    pub default_epochs: u32,
    pub default_lr: f32,
    pub default_batch: u32,
    /// Windows of the corpus consumed per epoch (a training budget, not a score).
    pub windows_per_epoch: u32,
    pub parameters: u64,
    pub description: &'static str,
    pub best_for: &'static str,
}

fn mlp_params(ctx: u32, hidden: u32) -> u64 {
    // W1: hidden × (ctx · 256), b1: hidden, W2: 256 × hidden, b2: 256
    let w1 = hidden as u64 * (ctx as u64 * VOCAB);
    let w2 = VOCAB * hidden as u64;
    w1 + w2 + hidden as u64 + VOCAB
}

fn transformer_params(d_model: u64, layers: u64, ffn: u64, vocab: u64) -> u64 {
    let per_layer = 4 * d_model * d_model + 3 * d_model * ffn + 4 * d_model;
    layers * per_layer + vocab * d_model
}

static ARCHITECTURES: &[Architecture] = &[
    Architecture {
        id: "atom-mlp-tiny",
        name: "Atom MLP · Tiny",
        family: "Byte MLP",
        summary: "Byte-level one-hidden-layer language model, 6-byte context.",
        context: 6,
        hidden: 24,
        layers: 1,
        trainable_here: true,
        requires: None,
        default_epochs: 12,
        default_lr: 0.05,
        default_batch: 16,
        windows_per_epoch: 8000,
        parameters: 0,
        description: "The smallest built-in model. Fast to train, small enough for an \
            8 MB ESP32-S3 with plenty of headroom, and expressive enough to learn the \
            local character statistics of a specialist dataset.",
        best_for: "Tight flash budgets, quick iteration, command-style specialists",
    },
    Architecture {
        id: "atom-mlp-small",
        name: "Atom MLP · Small",
        family: "Byte MLP",
        summary: "Wider hidden layer and 8-byte context.",
        context: 8,
        hidden: 48,
        layers: 1,
        trainable_here: true,
        requires: None,
        default_epochs: 12,
        default_lr: 0.04,
        default_batch: 16,
        windows_per_epoch: 10000,
        parameters: 0,
        description: "More capacity and a longer context than Tiny. Still trains in \
            this process and remains comfortable on ESP32-S3 with int8 weights.",
        best_for: "Structured command dialects, small domain corpora",
    },
    Architecture {
        id: "atom-mlp-wide",
        name: "Atom MLP · Wide",
        family: "Byte MLP",
        summary: "Widest built-in model for host and ESP32-S3 with PSRAM.",
        context: 12,
        hidden: 96,
        layers: 1,
        trainable_here: true,
        requires: None,
        default_epochs: 14,
        default_lr: 0.03,
        default_batch: 16,
        windows_per_epoch: 12000,
        parameters: 0,
        description: "The most capacity the built-in trainer offers. Needs PSRAM on \
            microcontrollers; int4 keeps it inside a 4 MB flash part.",
        best_for: "Richer text style on larger targets",
    },
    Architecture {
        id: "tiny-transformer-1m",
        name: "Tiny Transformer · 1M",
        family: "Transformer",
        summary: "4 layers, 128 dim, 512 context. External PyTorch trainer.",
        context: 512,
        hidden: 128,
        layers: 4,
        trainable_here: false,
        requires: Some("PyTorch (python3 -m torch)"),
        default_epochs: 3,
        default_lr: 0.0006,
        default_batch: 32,
        windows_per_epoch: 20000,
        parameters: 0,
        description: "A small decoder-only transformer trained by the external \
            adapter (tools/micro_train_transformer.py). ATOM launches it as a child \
            process and streams its logs; it is unavailable when PyTorch is absent.",
        best_for: "Higher quality than the built-in MLP, larger hosts/ESP32-S3 PSRAM",
    },
    Architecture {
        id: "tiny-transformer-4m",
        name: "Tiny Transformer · 4M",
        family: "Transformer",
        summary: "6 layers, 192 dim, 512 context. External PyTorch trainer.",
        context: 512,
        hidden: 192,
        layers: 6,
        trainable_here: false,
        requires: Some("PyTorch (python3 -m torch)"),
        default_epochs: 3,
        default_lr: 0.0004,
        default_batch: 24,
        windows_per_epoch: 24000,
        parameters: 0,
        description: "The larger transformer option. Practical on a host or an \
            ESP32-S3 with PSRAM after int8 quantization; exact fit is computed by the \
            Hardware Lab from the exported artifact.",
        best_for: "Maximum quality within the Micro catalog",
    },
];

pub fn architectures() -> Vec<Architecture> {
    ARCHITECTURES
        .iter()
        .map(|a| {
            let mut copy = a.clone();
            copy.parameters = match a.family {
                "Byte MLP" => mlp_params(a.context, a.hidden),
                _ => transformer_params(
                    a.hidden as u64,
                    a.layers as u64,
                    a.hidden as u64 * 4,
                    512,
                ),
            };
            copy
        })
        .collect()
}

pub fn architecture(id: &str) -> Option<Architecture> {
    architectures().into_iter().find(|a| a.id == id)
}

pub fn mlp_parameters(context: u32, hidden: u32) -> u64 {
    mlp_params(context, hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_params_positive() {
        let list = architectures();
        let mut ids: Vec<&str> = list.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), list.len());
        assert!(list.iter().all(|a| a.parameters > 0));
    }

    #[test]
    fn mlp_param_math_matches_layout() {
        assert_eq!(mlp_params(6, 24), 24 * 6 * 256 + 256 * 24 + 24 + 256);
    }

    #[test]
    fn transformer_needs_external_toolchain() {
        let t = architecture("tiny-transformer-1m").unwrap();
        assert!(!t.trainable_here);
        assert!(t.requires.is_some());
    }
}

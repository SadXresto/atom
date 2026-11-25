//! Target hardware catalog and memory/flash budget arithmetic.
//!
//! Every number here is a datasheet figure for the part named in `sources`.
//! The fit analysis combines the artifact's real weight bytes with explicit,
//! documented runtime and activation estimates. It is an estimate, not a
//! measurement, and is labelled as such wherever it is displayed.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct Target {
    pub id: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    pub mcu: &'static str,
    pub cpu_mhz: u32,
    pub cores: u8,
    pub sram_bytes: u64,
    pub flash_bytes: u64,
    pub psram_bytes: u64,
    pub fp32_fpu: bool,
    pub accelerator: &'static str,
    pub toolchain: &'static str,
    pub platformio_board: Option<&'static str>,
    pub notes: &'static str,
    /// Firmware image size reserved on top of the weights (runtime + SDK).
    pub firmware_bytes: u64,
    /// Heap/stack the generated runtime needs at inference time.
    pub runtime_ram_bytes: u64,
}

pub fn targets() -> &'static [Target] {
    &TARGETS
}

static TARGETS: [Target; 7] = [
    Target {
        id: "esp32s3",
        name: "ESP32-S3 (8 MB flash / 8 MB PSRAM)",
        vendor: "Espressif",
        mcu: "Xtensa LX7 dual core",
        cpu_mhz: 240,
        cores: 2,
        sram_bytes: 512 * 1024,
        flash_bytes: 8 * 1024 * 1024,
        psram_bytes: 8 * 1024 * 1024,
        fp32_fpu: true,
        accelerator: "vector instructions (ESP-DSP)",
        toolchain: "PlatformIO (espressif32)",
        platformio_board: Some("esp32-s3-devkitc-1"),
        notes: "Most headroom of the supported parts; PSRAM can hold larger models.",
        firmware_bytes: 320 * 1024,
        runtime_ram_bytes: 40 * 1024,
    },
    Target {
        id: "esp32c6",
        name: "ESP32-C6 (4 MB flash)",
        vendor: "Espressif",
        mcu: "RISC-V single core",
        cpu_mhz: 160,
        cores: 1,
        sram_bytes: 512 * 1024,
        flash_bytes: 4 * 1024 * 1024,
        psram_bytes: 0,
        fp32_fpu: true,
        accelerator: "RISC-V PIE extensions",
        toolchain: "PlatformIO (espressif32)",
        platformio_board: Some("esp32-c6-devkitc-1"),
        notes: "Wi-Fi 6 / Thread. No PSRAM: weights and buffers must fit internal memory.",
        firmware_bytes: 300 * 1024,
        runtime_ram_bytes: 36 * 1024,
    },
    Target {
        id: "esp32",
        name: "ESP32 DevKit (4 MB flash)",
        vendor: "Espressif",
        mcu: "Xtensa LX6 dual core",
        cpu_mhz: 240,
        cores: 2,
        sram_bytes: 520 * 1024,
        flash_bytes: 4 * 1024 * 1024,
        psram_bytes: 0,
        fp32_fpu: true,
        accelerator: "none",
        toolchain: "PlatformIO (espressif32)",
        platformio_board: Some("esp32dev"),
        notes: "Classic ESP32. Enough RAM for small byte-level models only.",
        firmware_bytes: 300 * 1024,
        runtime_ram_bytes: 36 * 1024,
    },
    Target {
        id: "rp2040",
        name: "Raspberry Pi Pico (RP2040)",
        vendor: "Raspberry Pi",
        mcu: "Dual Cortex-M0+",
        cpu_mhz: 133,
        cores: 2,
        sram_bytes: 264 * 1024,
        flash_bytes: 2 * 1024 * 1024,
        psram_bytes: 0,
        fp32_fpu: false,
        accelerator: "PIO (not used for matmul)",
        toolchain: "PlatformIO (raspberrypi)",
        platformio_board: Some("pico"),
        notes: "No hardware float unit: quantized integer inference is strongly preferred.",
        firmware_bytes: 160 * 1024,
        runtime_ram_bytes: 28 * 1024,
    },
    Target {
        id: "stm32f411",
        name: "STM32F411 Black Pill",
        vendor: "STMicroelectronics",
        mcu: "Cortex-M4F",
        cpu_mhz: 100,
        cores: 1,
        sram_bytes: 128 * 1024,
        flash_bytes: 512 * 1024,
        psram_bytes: 0,
        fp32_fpu: true,
        accelerator: "single-cycle MAC/DSP",
        toolchain: "PlatformIO (ststm32)",
        platformio_board: Some("blackpill_f411ce"),
        notes: "Tightest RAM in the catalog; use int8 or int4 models.",
        firmware_bytes: 120 * 1024,
        runtime_ram_bytes: 20 * 1024,
    },
    Target {
        id: "nrf52840",
        name: "nRF52840 Dongle / Feather",
        vendor: "Nordic Semiconductor",
        mcu: "Cortex-M4F",
        cpu_mhz: 64,
        cores: 1,
        sram_bytes: 256 * 1024,
        flash_bytes: 1024 * 1024,
        psram_bytes: 0,
        fp32_fpu: true,
        accelerator: "none",
        toolchain: "PlatformIO (nordicnrf52)",
        platformio_board: Some("nrf52840_dongle"),
        notes: "Bluetooth LE class part. Slow but capable of tiny models.",
        firmware_bytes: 220 * 1024,
        runtime_ram_bytes: 24 * 1024,
    },
    Target {
        id: "host",
        name: "This computer (reference)",
        vendor: "Host",
        mcu: "x86_64 / arm64",
        cpu_mhz: 0,
        cores: 1,
        sram_bytes: 0,
        flash_bytes: 0,
        psram_bytes: 0,
        fp32_fpu: true,
        accelerator: "SIMD",
        toolchain: "cc",
        platformio_board: None,
        notes: "Reference target used to verify exported C code compiles and runs.",
        firmware_bytes: 0,
        runtime_ram_bytes: 0,
    },
];

pub fn target(id: &str) -> Option<&'static Target> {
    TARGETS.iter().find(|t| t.id == id)
}

/// What the artifact needs on the device, derived from its real weight bytes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryPlan {
    pub target_id: String,
    pub target_name: String,
    pub quant_format: String,
    pub params: u64,
    pub weight_bytes: u64,
    pub activation_bytes: u64,
    pub runtime_ram_bytes: u64,
    pub total_ram_bytes: u64,
    pub sram_bytes: u64,
    pub psram_bytes: u64,
    pub firmware_bytes: u64,
    pub total_flash_bytes: u64,
    pub flash_bytes: u64,
    pub ram_used_percent: f64,
    pub flash_used_percent: f64,
    pub fits_ram: bool,
    pub fits_flash: bool,
    pub verdict: String,
    pub formula: String,
    pub estimate: bool,
}

/// Activation footprint for the byte-level MLP runtime.
///
/// `ctx * 256` one-hot inputs are consumed directly, so the buffer we keep is
/// the hidden vector plus the output logits, in f32, with a 2× working factor.
pub fn activation_bytes(ctx: u32, hidden: u32) -> u64 {
    let hidden_bytes = hidden as u64 * 4;
    let logits = 256 * 4;
    let context = ctx as u64;
    (hidden_bytes + logits + context) * 2
}

#[allow(clippy::too_many_arguments)]
pub fn plan(
    target: &Target,
    quant_format: &str,
    params: u64,
    weight_bytes: u64,
    ctx: u32,
    hidden: u32,
) -> MemoryPlan {
    let activation = activation_bytes(ctx, hidden);
    let runtime = target.runtime_ram_bytes;
    // Weights are modelled as flash-resident on microcontrollers; inference
    // streams them, so the RAM figure is activations plus the runtime heap.
    let total_ram = activation + runtime;
    let firmware = target.firmware_bytes;
    let total_flash = firmware + weight_bytes + 16 * 1024; // filesystem/metadata
    let sram = target.sram_bytes;
    let flash = target.flash_bytes;
    let ram_limit = if target.id == "host" {
        u64::MAX / 2
    } else {
        sram + target.psram_bytes
    };
    let fits_ram = target.id == "host" || total_ram <= sram;
    let fits_flash = target.id == "host" || total_flash <= flash;
    let verdict = if target.id == "host" {
        "reference target".to_string()
    } else if fits_ram && fits_flash {
        "fits".to_string()
    } else if fits_flash && !fits_ram && target.psram_bytes > 0 && total_ram <= ram_limit {
        "fits with PSRAM".to_string()
    } else {
        "does not fit".to_string()
    };
    MemoryPlan {
        target_id: target.id.to_string(),
        target_name: target.name.to_string(),
        quant_format: quant_format.to_string(),
        params,
        weight_bytes,
        activation_bytes: activation,
        runtime_ram_bytes: runtime,
        total_ram_bytes: total_ram,
        sram_bytes: sram,
        psram_bytes: target.psram_bytes,
        firmware_bytes: firmware,
        total_flash_bytes: total_flash,
        flash_bytes: flash,
        ram_used_percent: if sram > 0 {
            (total_ram as f64 / sram as f64) * 100.0
        } else {
            0.0
        },
        flash_used_percent: if flash > 0 {
            (total_flash as f64 / flash as f64) * 100.0
        } else {
            0.0
        },
        fits_ram,
        fits_flash,
        verdict,
        formula: format!(
            "RAM = activations (ctx*256 one-hot → {hidden} hidden, logits, ×2 working) + runtime heap; \
             flash = firmware ({firmware} B) + weights ({weight_bytes} B) + 16 KiB metadata"
        ),
        estimate: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_unique_ids() {
        let mut ids: Vec<&str> = targets().iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), targets().len());
    }

    #[test]
    fn small_model_fits_microcontroller() {
        let esp = target("esp32s3").unwrap();
        let plan = plan(esp, "int8", 43_288, 43_288, 6, 24);
        assert!(plan.fits_ram && plan.fits_flash, "{}", plan.verdict);
        assert!(plan.total_ram_bytes < 512 * 1024);
    }

    #[test]
    fn oversized_model_is_rejected() {
        let pico = target("rp2040").unwrap();
        let plan = plan(pico, "f32", 2_000_000, 8_000_000, 8, 64);
        assert!(!plan.fits_flash);
        assert_eq!(plan.verdict, "does not fit");
    }
}

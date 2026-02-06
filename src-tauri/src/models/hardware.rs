#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub chip: ChipType,
    pub chip_name: String,
    pub ram_gb: u64,
    pub cpu_cores: usize,
    pub has_neural_engine: bool,
    pub has_metal: bool,
    pub recommended_tier: Tier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChipType {
    AppleSilicon,
    Intel,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Lite,     // Intel or 8GB Apple Silicon
    Standard, // 16GB Apple Silicon
    Power,    // 32GB+ Apple Silicon
}

pub struct HardwareDetector;

impl HardwareDetector {
    pub fn detect() -> HardwareProfile {
        let mut sys = System::new_all();
        sys.refresh_all();

        let ram_bytes = sys.total_memory();
        let ram_gb = ram_bytes / (1024 * 1024 * 1024);
        let cpu_cores = sys.cpus().len();

        // Detect chip type
        let (chip, chip_name, has_neural_engine, has_metal) = Self::detect_chip();

        // Determine recommended tier
        let recommended_tier = Self::determine_tier(&chip, ram_gb);

        HardwareProfile {
            chip,
            chip_name,
            ram_gb,
            cpu_cores,
            has_neural_engine,
            has_metal,
            recommended_tier,
        }
    }

    fn detect_chip() -> (ChipType, String, bool, bool) {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            let output = Command::new("sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output();

            if let Ok(output) = output {
                let brand = String::from_utf8_lossy(&output.stdout).trim().to_string();

                if brand.contains("Apple") {
                    let chip_name = if brand.contains("M1") {
                        if brand.contains("Max") {
                            "Apple M1 Max".to_string()
                        } else if brand.contains("Pro") {
                            "Apple M1 Pro".to_string()
                        } else if brand.contains("Ultra") {
                            "Apple M1 Ultra".to_string()
                        } else {
                            "Apple M1".to_string()
                        }
                    } else if brand.contains("M2") {
                        if brand.contains("Max") {
                            "Apple M2 Max".to_string()
                        } else if brand.contains("Pro") {
                            "Apple M2 Pro".to_string()
                        } else if brand.contains("Ultra") {
                            "Apple M2 Ultra".to_string()
                        } else {
                            "Apple M2".to_string()
                        }
                    } else if brand.contains("M3") {
                        if brand.contains("Max") {
                            "Apple M3 Max".to_string()
                        } else if brand.contains("Pro") {
                            "Apple M3 Pro".to_string()
                        } else if brand.contains("Ultra") {
                            "Apple M3 Ultra".to_string()
                        } else {
                            "Apple M3".to_string()
                        }
                    } else if brand.contains("M4") {
                        if brand.contains("Max") {
                            "Apple M4 Max".to_string()
                        } else if brand.contains("Pro") {
                            "Apple M4 Pro".to_string()
                        } else {
                            "Apple M4".to_string()
                        }
                    } else {
                        brand.clone()
                    };

                    return (ChipType::AppleSilicon, chip_name, true, true);
                } else if brand.contains("Intel") {
                    return (ChipType::Intel, brand, false, true);
                }
            }
        }

        (ChipType::Unknown, "Unknown".to_string(), false, false)
    }

    fn determine_tier(chip: &ChipType, ram_gb: u64) -> Tier {
        match chip {
            ChipType::AppleSilicon => {
                if ram_gb >= 32 {
                    Tier::Power
                } else if ram_gb >= 16 {
                    Tier::Standard
                } else {
                    Tier::Lite
                }
            }
            ChipType::Intel | ChipType::Unknown => Tier::Lite,
        }
    }
}

impl HardwareProfile {
    pub fn recommended_stt_model(&self) -> &str {
        match self.recommended_tier {
            Tier::Power => "ggml-base.en.bin",
            Tier::Standard => "ggml-base.en.bin",
            Tier::Lite => "ggml-tiny.en.bin",
        }
    }

    pub fn recommended_tts_model(&self) -> &str {
        match self.recommended_tier {
            Tier::Power | Tier::Standard => "kokoro-v1.0.onnx",
            Tier::Lite => "kokoro-v1.0.onnx",
        }
    }
}

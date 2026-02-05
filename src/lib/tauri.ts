import { invoke } from "@tauri-apps/api/core";

// Type definitions for Tauri commands

export interface TranscriptionResult {
  text: string;
  duration_ms: number;
}

export interface VoiceInfo {
  id: string;
  name: string;
  language: string;
  gender: string;
}

export interface ModelInfo {
  id: string;
  name: string;
  model_type: "stt" | "tts";
  size_bytes: number;
  size_display: string;
  download_url: string;
  status: "available" | "downloaded" | "downloading";
  description: string;
}

export interface AppSettings {
  stt_hotkey: string;
  tts_hotkey: string;
  stt_model: string;
  tts_voice: string;
  tts_speed: number;
  auto_paste: boolean;
  launch_at_login: boolean;
  menu_bar_mode: boolean;
}

export interface HardwareProfile {
  chip: "applesilicon" | "intel" | "unknown";
  chip_name: string;
  ram_gb: number;
  cpu_cores: number;
  has_neural_engine: boolean;
  has_metal: boolean;
  recommended_tier: "lite" | "standard" | "power";
}

// STT Commands
export const stt = {
  startRecording: () => invoke("start_recording"),
  stopRecording: () => invoke<number[]>("stop_recording"),
  transcribe: (audioData: number[], modelPath: string) =>
    invoke<TranscriptionResult>("transcribe_audio", { audioData, modelPath }),
};

// TTS Commands
export const tts = {
  speak: (text: string, voiceId: string, speed: number, modelPath: string) =>
    invoke("speak_text", { text, voiceId, speed, modelPath }),
  stop: () => invoke("stop_speaking"),
  getVoices: () => invoke<VoiceInfo[]>("get_voices"),
};

// Model Commands
export const models = {
  list: () => invoke<ModelInfo[]>("list_models"),
  download: (modelId: string) => invoke<string>("download_model", { modelId }),
  delete: (modelId: string) => invoke("delete_model", { modelId }),
  getStatus: (modelId: string) => invoke<string>("get_model_status", { modelId }),
};

// Settings Commands
export const settings = {
  get: () => invoke<AppSettings>("get_settings"),
  update: (settings: AppSettings) => invoke("update_settings", { settings }),
  getHardwareInfo: () => invoke<HardwareProfile>("get_hardware_info"),
};

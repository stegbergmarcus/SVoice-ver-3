import { invoke } from "@tauri-apps/api/core";

export type ComputeMode = "auto" | "cpu" | "gpu";
export type LlmProviderChoice = "auto" | "claude" | "ollama";

export interface Settings {
  mic_device: string | null;
  stt_model: string;
  stt_compute_mode: ComputeMode;
  vad_threshold: number;
  llm_provider: LlmProviderChoice;
  anthropic_api_key: string | null;
  anthropic_model: string;
  ollama_model: string;
  ollama_url: string;
}

export async function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

export async function setSettings(settings: Settings): Promise<void> {
  await invoke<void>("set_settings", { settings });
}

export async function listMicDevices(): Promise<string[]> {
  return invoke<string[]>("list_mic_devices");
}

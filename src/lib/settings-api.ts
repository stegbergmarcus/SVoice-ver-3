import { invoke } from "@tauri-apps/api/core";

export type ComputeMode = "auto" | "cpu" | "gpu";

export interface Settings {
  mic_device: string | null;
  stt_model: string;
  stt_compute_mode: ComputeMode;
  vad_threshold: number;
  anthropic_api_key: string | null;
  anthropic_model: string;
}

export async function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

export async function setSettings(settings: Settings): Promise<void> {
  await invoke<void>("set_settings", { settings });
}

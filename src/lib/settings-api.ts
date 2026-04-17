import { invoke } from "@tauri-apps/api/core";

export type ComputeMode = "auto" | "cpu" | "gpu";
export type LlmProviderChoice = "auto" | "claude" | "ollama";

export interface Settings {
  mic_device: string | null;
  stt_enabled: boolean;
  stt_model: string;
  stt_compute_mode: ComputeMode;
  vad_threshold: number;
  action_llm_enabled: boolean;
  llm_polish_dictation: boolean;
  llm_provider: LlmProviderChoice;
  anthropic_api_key: string | null;
  anthropic_model: string;
  ollama_model: string;
  ollama_url: string;
}

export interface OllamaModelInfo {
  name: string;
  size: number;
}

export interface PullProgress {
  model: string;
  status: string;
  total: number | null;
  completed: number | null;
  done: boolean;
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

export async function listOllamaModels(): Promise<OllamaModelInfo[]> {
  return invoke<OllamaModelInfo[]>("list_ollama_models");
}

export async function pullOllamaModel(model: string): Promise<void> {
  await invoke<void>("pull_ollama_model", { model });
}

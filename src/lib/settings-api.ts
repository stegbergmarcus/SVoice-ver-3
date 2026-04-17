import { invoke } from "@tauri-apps/api/core";

export type ComputeMode = "auto" | "cpu" | "gpu";
export type LlmProviderChoice = "auto" | "claude" | "ollama";
export type HotKeyChoice =
  | "right_ctrl"
  | "insert"
  | "right_alt"
  | "f12"
  | "pause"
  | "scroll_lock"
  | "caps_lock"
  | "home"
  | "end";

export interface Settings {
  mic_device: string | null;
  stt_enabled: boolean;
  stt_model: string;
  stt_compute_mode: ComputeMode;
  vad_threshold: number;
  action_llm_enabled: boolean;
  llm_polish_dictation: boolean;
  llm_provider: LlmProviderChoice;
  anthropic_model: string;
  ollama_model: string;
  ollama_url: string;
  dictation_hotkey: HotKeyChoice;
  action_hotkey: HotKeyChoice;
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

export async function checkHfCached(model: string): Promise<boolean> {
  return invoke<boolean>("check_hf_cached", { model });
}

export async function hasAnthropicKey(): Promise<boolean> {
  return invoke<boolean>("has_anthropic_key");
}

export async function setAnthropicKey(key: string): Promise<void> {
  await invoke<void>("set_anthropic_key", { key });
}

export async function clearAnthropicKey(): Promise<void> {
  await invoke<void>("clear_anthropic_key");
}

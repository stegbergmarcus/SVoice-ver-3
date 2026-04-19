import { invoke } from "@tauri-apps/api/core";

export type ComputeMode = "auto" | "cpu" | "gpu";
export type LlmProviderChoice = "auto" | "claude" | "ollama" | "groq" | "gemini";
export type SttProviderChoice = "local" | "groq";
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
  /** LLM-provider för action-popup (Insert-PTT → svar) */
  action_llm_provider: LlmProviderChoice;
  /** LLM-provider för dikterings-polering (efter RightCtrl-STT) */
  dictation_llm_provider: LlmProviderChoice;
  anthropic_model: string;
  ollama_model: string;
  ollama_url: string;
  groq_llm_model: string;
  groq_stt_model: string;
  gemini_model: string;
  stt_provider: SttProviderChoice;
  stt_language: string;
  /** Avancerat: beam search-storlek (1 = greedy, 5 = balans, 10 = diminishing returns) */
  stt_beam_size: number;
  /** Avancerat: aktivera faster-whispers inbyggda Silero-VAD */
  stt_vad_filter: boolean;
  /** Avancerat: "priming"-text som biased:ar stil och fackord */
  stt_initial_prompt: string;
  /** Avancerat: tröskel 0-1 för no_speech-filtret */
  stt_no_speech_threshold: number;
  /** Avancerat: feeda tidigare transkript tillbaka som kontext */
  stt_condition_on_previous_text: boolean;
  dictation_hotkey: HotKeyChoice;
  action_hotkey: HotKeyChoice;
  google_oauth_client_id: string | null;
  google_oauth_client_secret: string | null;
  autostart: boolean;
}

export interface GoogleStatus {
  connected: boolean;
  client_id_configured: boolean;
}

export interface UpdateStatus {
  current_version: string;
  latest_version: string | null;
  available: boolean;
  download_url: string | null;
  release_notes: string | null;
  checked_at: number;
}

export interface SttModelDownloadProgress {
  model: string;
  status: string;
}

export async function downloadSttModel(model: string): Promise<void> {
  await invoke<void>("download_stt_model", { model });
}

export async function checkForUpdates(): Promise<UpdateStatus> {
  return invoke<UpdateStatus>("check_for_updates");
}

export async function checkForUpdatesCached(): Promise<UpdateStatus> {
  return invoke<UpdateStatus>("check_for_updates_cached");
}

export type SmartMode = "transform" | "query";

export interface SmartFunction {
  id: string;
  name: string;
  description: string;
  mode: SmartMode;
  system: string;
  user_template: string;
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

export async function hasGroqKey(): Promise<boolean> {
  return invoke<boolean>("has_groq_key");
}

export async function setGroqKey(key: string): Promise<void> {
  await invoke<void>("set_groq_key", { key });
}

export async function clearGroqKey(): Promise<void> {
  await invoke<void>("clear_groq_key");
}

export async function hasGeminiKey(): Promise<boolean> {
  return invoke<boolean>("has_gemini_key");
}

export async function setGeminiKey(key: string): Promise<void> {
  await invoke<void>("set_gemini_key", { key });
}

export async function clearGeminiKey(): Promise<void> {
  await invoke<void>("clear_gemini_key");
}

export async function googleConnectionStatus(): Promise<GoogleStatus> {
  return invoke<GoogleStatus>("google_connection_status");
}

export async function googleConnect(): Promise<void> {
  await invoke<void>("google_connect");
}

export async function googleDisconnect(): Promise<void> {
  await invoke<void>("google_disconnect");
}

export async function listSmartFunctions(): Promise<SmartFunction[]> {
  return invoke<SmartFunction[]>("list_smart_functions");
}

export async function openSmartFunctionsDir(): Promise<void> {
  await invoke<void>("open_smart_functions_dir");
}

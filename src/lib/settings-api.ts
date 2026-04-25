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
  /** Padding (ms) på båda sidor av RMS-trimmen så ordstart/slut inte kapas */
  vad_trim_padding_ms: number;
  /** Auto-prepend mellanslag om ny diktering sker inom X sekunder efter förra (0 = av) */
  dictation_auto_space_seconds: number;
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

export type GoogleVerifyState =
  | "ok"
  | "no_token"
  | "revoked"
  | "no_client_id"
  | "transient"
  | "unknown";

export interface GoogleStatus {
  connected: boolean;
  client_id_configured: boolean;
  /**
   * Senaste verifierings-resultat. `connected=true` ↔ `verify_state="ok"`.
   * `revoked` betyder att Google avvisat tokenen (typiskt: user revokat
   * appen via myaccount.google.com). `transient` = nätverksfel, status okänd.
   */
  verify_state: GoogleVerifyState;
}

export interface OllamaStatus {
  online: boolean;
  installed: boolean;
  install_path: string | null;
  platform: string;
  install_supported: boolean;
  url: string;
}

export type OllamaInstallStatus =
  | { kind: "installed"; path: string }
  | { kind: "not_installed" }
  | { kind: "unsupported"; platform: string };

export type OllamaInstallProgress =
  | { phase: "download_started"; url: string }
  | { phase: "download_progress"; downloaded: number; total: number | null }
  | { phase: "installing" }
  | { phase: "waiting_for_service" }
  | { phase: "done"; path: string | null };

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

/**
 * Validera anslutning genom att faktiskt minta en access-token mot Google.
 * Långsammare än `googleConnectionStatus` (kräver nätverk), men säger
 * sanningen — om refresh-token revokats raderas den lokalt så UI:t direkt
 * visar "ej ansluten".
 */
export async function googleVerifyConnection(): Promise<GoogleStatus> {
  return invoke<GoogleStatus>("google_verify_connection");
}

export async function googleConnect(): Promise<void> {
  await invoke<void>("google_connect");
}

export async function googleDisconnect(): Promise<void> {
  await invoke<void>("google_disconnect");
}

export async function ollamaStatus(): Promise<OllamaStatus> {
  return invoke<OllamaStatus>("ollama_status");
}

export async function ollamaInstallDetect(): Promise<OllamaInstallStatus> {
  return invoke<OllamaInstallStatus>("ollama_install_detect");
}

export async function ollamaInstall(): Promise<void> {
  await invoke<void>("ollama_install");
}

export async function listSmartFunctions(): Promise<SmartFunction[]> {
  return invoke<SmartFunction[]>("list_smart_functions");
}

/**
 * Live-resolved provider för action-popup eller dictation-polish.
 * Spegelvänd `select_llm_provider` (src-tauri/src/lib.rs) — visar vad
 * som *faktiskt* skulle plockas just nu, inkl. fallback-kedjan i Auto.
 */
export type ActiveLlm =
  | { kind: "disabled" }
  | { kind: "ollama"; model: string; base_url: string }
  | { kind: "claude"; model: string }
  | { kind: "groq"; model: string }
  | { kind: "gemini"; model: string }
  | { kind: "unavailable"; configured: string; reason: string };

export type ActiveStt =
  | { kind: "disabled" }
  | { kind: "local"; model: string; compute: string }
  | { kind: "groq"; model: string };

export interface ActiveStack {
  stt: ActiveStt;
  action_llm: ActiveLlm;
  dictation_llm: ActiveLlm;
}

export async function activeStack(): Promise<ActiveStack> {
  return invoke<ActiveStack>("active_stack");
}

export async function openSmartFunctionsDir(): Promise<void> {
  await invoke<void>("open_smart_functions_dir");
}

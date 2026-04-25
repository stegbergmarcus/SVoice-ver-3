import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import SVoiceLogo from "../components/SVoiceLogo";
import {
  activeStack,
  checkForUpdates,
  checkForUpdatesCached,
  checkHfCached,
  clearAnthropicKey,
  clearGeminiKey,
  clearGroqKey,
  downloadSttModel,
  getSettings,
  googleConnect,
  googleConnectionStatus,
  googleDisconnect,
  googleVerifyConnection,
  hasAnthropicKey,
  hasGeminiKey,
  hasGroqKey,
  listMicDevices,
  listOllamaModels,
  listSmartFunctions,
  ollamaInstall,
  ollamaStart,
  ollamaStatus,
  ollamaStop,
  openSmartFunctionsDir,
  pullOllamaModel,
  setAnthropicKey,
  setGeminiKey,
  setGroqKey,
  setSettings,
  type ActiveLlm,
  type ActiveStack,
  type ActiveStt,
  type ComputeMode,
  type GoogleStatus,
  type HotKeyChoice,
  type LlmProviderChoice,
  type OllamaInstallProgress,
  type OllamaModelInfo,
  type PullProgress,
  type Settings,
  type SmartFunction,
  type SttModelDownloadProgress,
  type SttProviderChoice,
  type UpdateStatus,
} from "../lib/settings-api";
import "./Settings.css";

const MODELS: Array<{ id: string; label: string; note: string }> = [
  { id: "KBLab/kb-whisper-base", label: "KB-Whisper Base", note: "snabbast · ~1 GB VRAM" },
  { id: "KBLab/kb-whisper-medium", label: "KB-Whisper Medium", note: "balans · ~4 GB VRAM" },
  { id: "KBLab/kb-whisper-large", label: "KB-Whisper Large", note: "högst kvalitet · ~6 GB VRAM" },
];

const COMPUTE_LABELS: Record<ComputeMode, string> = {
  auto: "Auto",
  cpu: "CPU",
  gpu: "GPU",
};

const PROVIDER_LABELS: Record<LlmProviderChoice, string> = {
  auto: "Auto (lokal → Groq → Gemini → Claude)",
  ollama: "Lokal (Ollama)",
  claude: "Claude API",
  groq: "Groq API (gratis-tier)",
  gemini: "Gemini (Google AI)",
};

const GEMINI_MODELS: Array<{ id: string; label: string; note: string }> = [
  {
    id: "gemini-3-pro-preview",
    label: "Gemini 3 Pro Preview",
    note: "senaste flaggskepp · högsta kvalitet (preview)",
  },
  {
    id: "gemini-3-flash-preview",
    label: "Gemini 3 Flash Preview",
    note: "Gemini 3 · snabb + smart (preview)",
  },
  {
    id: "gemini-3.1-pro-preview",
    label: "Gemini 3.1 Pro Preview",
    note: "nyaste Pro — mest kapabel (preview)",
  },
  {
    id: "gemini-2.5-pro",
    label: "Gemini 2.5 Pro",
    note: "stabil · hög kvalitet",
  },
  {
    id: "gemini-2.5-flash",
    label: "Gemini 2.5 Flash",
    note: "stabil · snabb · billig · Google Search-grounding",
  },
];

const STT_PROVIDER_LABELS: Record<SttProviderChoice, string> = {
  local: "Lokal (KB-Whisper via Python-sidecar)",
  groq: "Groq Whisper API (gratis, ~100× snabbare)",
};

/** Visningsnamn för en aktiv LLM-rad i "Aktiv stack"-kortet. */
function describeActiveLlm(a: ActiveLlm): {
  badge: string;
  badgeTone: "local" | "cloud" | "off" | "warn";
  primary: string;
  secondary: string;
} {
  switch (a.kind) {
    case "ollama":
      return {
        badge: "LOKAL",
        badgeTone: "local",
        primary: "Ollama",
        secondary: a.model,
      };
    case "claude":
      return {
        badge: "MOLN",
        badgeTone: "cloud",
        primary: "Claude",
        secondary: a.model,
      };
    case "groq":
      return {
        badge: "MOLN",
        badgeTone: "cloud",
        primary: "Groq",
        secondary: a.model,
      };
    case "gemini":
      return {
        badge: "MOLN",
        badgeTone: "cloud",
        primary: "Gemini",
        secondary: a.model,
      };
    case "disabled":
      return {
        badge: "AV",
        badgeTone: "off",
        primary: "Avstängd",
        secondary: "—",
      };
    case "unavailable":
      return {
        badge: "FEL",
        badgeTone: "warn",
        primary: a.configured.charAt(0).toUpperCase() + a.configured.slice(1),
        secondary: a.reason,
      };
  }
}

function describeActiveStt(a: ActiveStt): {
  badge: string;
  badgeTone: "local" | "cloud" | "off";
  primary: string;
  secondary: string;
} {
  switch (a.kind) {
    case "local":
      return {
        badge: "LOKAL",
        badgeTone: "local",
        primary: a.model.split("/").pop() || a.model,
        secondary: `KB-Whisper · ${a.compute.toUpperCase()}`,
      };
    case "groq":
      return {
        badge: "MOLN",
        badgeTone: "cloud",
        primary: "Groq Whisper",
        secondary: a.model,
      };
    case "disabled":
      return {
        badge: "AV",
        badgeTone: "off",
        primary: "STT avstängd",
        secondary: "—",
      };
  }
}

// Vanliga språkkoder för STT. "auto" låter Whisper detektera.
const STT_LANGUAGES: Array<{ code: string; label: string }> = [
  { code: "auto", label: "Auto-detektera" },
  { code: "sv", label: "Svenska" },
  { code: "en", label: "Engelska" },
  { code: "no", label: "Norska" },
  { code: "da", label: "Danska" },
  { code: "de", label: "Tyska" },
  { code: "fr", label: "Franska" },
  { code: "es", label: "Spanska" },
  { code: "fi", label: "Finska" },
  { code: "it", label: "Italienska" },
  { code: "nl", label: "Nederländska" },
  { code: "pl", label: "Polska" },
];

const GROQ_STT_MODELS: Array<{ id: string; label: string; note: string }> = [
  { id: "whisper-large-v3-turbo", label: "Whisper Large v3 Turbo", note: "snabbast · gratis-tier" },
  { id: "whisper-large-v3", label: "Whisper Large v3", note: "högst kvalitet" },
];

type TabId =
  | "overview"
  | "audio"
  | "llm"
  | "integrations"
  | "hotkeys"
  | "advanced"
  | "help";

const TABS: Array<{ id: TabId; label: string; icon: string }> = [
  { id: "overview", label: "Översikt", icon: "◆" },
  { id: "audio", label: "Diktering", icon: "◉" },
  { id: "llm", label: "Action-popup", icon: "❋" },
  { id: "integrations", label: "Integrationer", icon: "⊕" },
  { id: "hotkeys", label: "Snabbkommandon", icon: "⌘" },
  { id: "advanced", label: "Avancerat", icon: "⚙" },
  { id: "help", label: "Hjälp", icon: "?" },
];

/** Rekommenderade STT-parametrar — matchar Settings::default() i Rust. */
const STT_DEFAULTS = {
  stt_beam_size: 5,
  stt_vad_filter: true,
  stt_initial_prompt: "Svensk diktering. Korrekt interpunktion och stor bokstav.",
  stt_no_speech_threshold: 0.5,
  stt_condition_on_previous_text: false,
  vad_trim_padding_ms: 250,
  dictation_auto_space_seconds: 30,
} as const;

const GROQ_LLM_MODELS: Array<{ id: string; label: string; note: string }> = [
  { id: "llama-3.3-70b-versatile", label: "Llama 3.3 70B", note: "balans · stark på svenska" },
  { id: "openai/gpt-oss-120b", label: "GPT-OSS 120B", note: "toppresonemang · OpenAI-öppen" },
  { id: "moonshotai/kimi-k2-instruct", label: "Kimi K2", note: "stark på EU-språk" },
  { id: "llama-3.1-8b-instant", label: "Llama 3.1 8B", note: "snabbast · enkel" },
];

const HOTKEY_LABELS: Record<HotKeyChoice, string> = {
  right_ctrl: "Höger Ctrl",
  insert: "Insert",
  right_alt: "Höger Alt",
  f12: "F12",
  pause: "Pause / Break",
  scroll_lock: "Scroll Lock",
  caps_lock: "Caps Lock",
  home: "Home",
  end: "End",
};
const HOTKEY_ORDER: HotKeyChoice[] = [
  "right_ctrl",
  "insert",
  "right_alt",
  "f12",
  "pause",
  "scroll_lock",
  "caps_lock",
  "home",
  "end",
];

// Rekommenderade Ollama-modeller för RTX 5080 (16 GB VRAM).
// Användaren måste själv köra `ollama pull <modell>` innan första användning.
const OLLAMA_MODELS: Array<{ id: string; label: string; note: string }> = [
  { id: "qwen2.5:14b", label: "Qwen 2.5 14B", note: "balans · ~9 GB VRAM · stark svenska" },
  { id: "gpt-oss:20b", label: "GPT-OSS 20B", note: "OpenAI öppen · ~13 GB · toppresonemang" },
  { id: "qwen2.5:32b", label: "Qwen 2.5 32B", note: "högsta lokal kvalitet · ~20 GB (tight)" },
  { id: "llama3.1:8b", label: "Llama 3.1 8B", note: "snabbast · ~5 GB · allround" },
  { id: "mistral-small:24b", label: "Mistral Small 24B", note: "~14 GB · stark på EU-språk" },
  { id: "gemma2:27b", label: "Gemma 2 27B", note: "~16 GB · Googles öppna flaggskepp" },
];

function ToggleRow({
  label,
  help,
  value,
  onChange,
  details,
}: {
  label: string;
  help: string;
  value: boolean;
  onChange: (v: boolean) => void;
  details?: React.ReactNode;
}) {
  return (
    <div className="toggle-row">
      <div className="toggle-row-text">
        <div className="toggle-row-label">{label}</div>
        <div className="toggle-row-help">{help}</div>
        {details}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={value}
        aria-label={label}
        className={`toggle-switch${value ? " on" : ""}`}
        onClick={() => onChange(!value)}
      >
        <span className="toggle-thumb" />
      </button>
    </div>
  );
}

/** Expanderbar "Läs mer"-sektion som ligger under en field-help. Håller
 * Advanced-fliken ren som default men låter user fälla ut fördjupning när
 * det behövs. Använder native <details> för a11y + zero state. */
function FieldDetails({ children }: { children: React.ReactNode }) {
  return (
    <details className="field-details">
      <summary className="field-details-summary">
        <span className="field-details-icon" aria-hidden>
          ⓘ
        </span>
        <span className="field-details-label-text">Läs mer</span>
        <span className="field-details-chevron" aria-hidden>
          ›
        </span>
      </summary>
      <div className="field-details-body">{children}</div>
    </details>
  );
}

/** Rad i en FieldDetails-utläggning. `label` är t.ex. "Om på" / "Lägre".
 * `children` är värdet/förklaringen. Två-kolumns grid gör det scannbart. */
function DetailsRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="field-details-row">
      <div className="field-details-row-label">{label}</div>
      <div className="field-details-row-value">{children}</div>
    </div>
  );
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function installPhaseLabel(p: OllamaInstallProgress): string {
  switch (p.phase) {
    case "download_started":
      return "Startar nedladdning…";
    case "download_progress":
      return "Laddar ner Ollama…";
    case "installing":
      return "Installerar (godkänn UAC-prompten)…";
    case "waiting_for_service":
      return "Väntar på att tjänsten startar…";
    case "done":
      return "Klart";
  }
}

export default function SettingsView() {
  const [draft, setDraft] = useState<Settings | null>(null);
  const [loaded, setLoaded] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedTick, setSavedTick] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [micLevel, setMicLevel] = useState(0);
  const [promptModalOpen, setPromptModalOpen] = useState(false);
  const [promptModalDraft, setPromptModalDraft] = useState("");
  const [micDevices, setMicDevices] = useState<string[]>([]);
  const [ollamaModels, setOllamaModels] = useState<OllamaModelInfo[]>([]);
  const [ollamaOnline, setOllamaOnline] = useState(false);
  const [pullState, setPullState] = useState<PullProgress | null>(null);
  const [sttCached, setSttCached] = useState<Record<string, boolean>>({});
  const [sttDownload, setSttDownload] = useState<{
    model: string;
    status: string;
    done: boolean;
  } | null>(null);
  const [keyStored, setKeyStored] = useState(false);
  const [keyDraft, setKeyDraft] = useState<string | null>(null); // null=orört, ""=rensa, annars=ny nyckel
  const [groqKeyStored, setGroqKeyStored] = useState(false);
  const [groqKeyDraft, setGroqKeyDraft] = useState<string | null>(null);
  const [geminiKeyStored, setGeminiKeyStored] = useState(false);
  const [geminiKeyDraft, setGeminiKeyDraft] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<TabId>("overview");
  const [googleStatus, setGoogleStatus] = useState<GoogleStatus>({
    connected: false,
    client_id_configured: false,
    verify_state: "unknown",
  });
  const [googleBusy, setGoogleBusy] = useState(false);
  const [ollamaInstalled, setOllamaInstalled] = useState<boolean | null>(null);
  const [ollamaInstallSupported, setOllamaInstallSupported] = useState(true);
  const [ollamaInstallBusy, setOllamaInstallBusy] = useState(false);
  const [ollamaInstallProgress, setOllamaInstallProgress] =
    useState<OllamaInstallProgress | null>(null);
  const [ollamaInstallError, setOllamaInstallError] = useState<string | null>(null);
  const [smartFns, setSmartFns] = useState<SmartFunction[]>([]);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [stack, setStack] = useState<ActiveStack | null>(null);
  const [ollamaStartBusy, setOllamaStartBusy] = useState(false);
  const [ollamaStartError, setOllamaStartError] = useState<string | null>(null);
  const [ollamaStopBusy, setOllamaStopBusy] = useState(false);

  // Refresh Ollama-modell-listan (t.ex. efter lyckad pull) + binary-detect.
  async function refreshOllama() {
    try {
      const status = await ollamaStatus();
      setOllamaOnline(status.online);
      setOllamaInstalled(status.installed);
      setOllamaInstallSupported(status.install_supported);
      if (status.online) {
        const models = await listOllamaModels();
        setOllamaModels(models);
      } else {
        setOllamaModels([]);
      }
    } catch {
      setOllamaModels([]);
      setOllamaOnline(false);
    }
  }

  useEffect(() => {
    getSettings()
      .then((s) => {
        setDraft(s);
        setLoaded(s);
      })
      .catch((e) => setError(String(e)));
    listMicDevices()
      .then(setMicDevices)
      .catch((e) => console.error("[settings] list_mic_devices failed:", e));
    refreshOllama();
    hasAnthropicKey()
      .then(setKeyStored)
      .catch(() => setKeyStored(false));
    hasGroqKey()
      .then(setGroqKeyStored)
      .catch(() => setGroqKeyStored(false));
    hasGeminiKey()
      .then(setGeminiKeyStored)
      .catch(() => setGeminiKeyStored(false));
    // Snabb keyring-koll först så UI:t inte flimrar; sedan riktig Google-
    // verifiering i bakgrunden (gör ett HTTP-anrop, ~200–800 ms). Vid
    // revokat token raderar backend lokal kopia och returnerar
    // connected=false så UI:t direkt visar "ej ansluten".
    googleConnectionStatus()
      .then(setGoogleStatus)
      .catch(() =>
        setGoogleStatus({
          connected: false,
          client_id_configured: false,
          verify_state: "unknown",
        }),
      );
    googleVerifyConnection()
      .then(setGoogleStatus)
      .catch((e) => console.debug("[settings] google_verify_connection failed:", e));
    listSmartFunctions().then(setSmartFns).catch(() => setSmartFns([]));
    checkForUpdatesCached()
      .then(setUpdateStatus)
      .catch((e) => console.debug("[settings] update-check (cached) failed:", e));
    // Kolla HF-cache-status för alla listade STT-modeller i bakgrunden.
    Promise.all(
      MODELS.map(async (m) => ({
        id: m.id,
        cached: await checkHfCached(m.id).catch(() => false),
      })),
    ).then((results) => {
      const out: Record<string, boolean> = {};
      for (const r of results) out[r.id] = r.cached;
      setSttCached(out);
    });
  }, []);

  // Live-probe av aktiv stack (vad som *faktiskt* körs just nu, inkl.
  // Auto-fallback). Initial fetch + poll var 8:e sek så Ollama-online/
  // offline-skiften reflekteras utan att user behöver göra något.
  useEffect(() => {
    let cancelled = false;
    const fetchStack = () => {
      activeStack()
        .then((s) => {
          if (!cancelled) setStack(s);
        })
        .catch(() => {});
    };
    fetchStack();
    const id = setInterval(fetchStack, 8000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  // Bakgrunds-events från backend: Google-verifiering var 5:e min och
  // Ollama-status var 30:e sek. Vi listar både Pull-progress, install-
  // progress och status-pings i samma effect så all teardown sker tillsammans.
  useEffect(() => {
    const unGoogleStatus = listen<GoogleStatus>(
      "google_connection_status",
      (ev) => {
        setGoogleStatus(ev.payload);
      },
    );
    const unOllamaStatus = listen<{ online: boolean; url: string }>(
      "ollama_status",
      (ev) => {
        setOllamaOnline(ev.payload.online);
        if (ev.payload.online) {
          // Service kom upp — refresha modell-listan så dropdown-status
          // (✓/↓) blir korrekt utan att user behöver klicka något.
          listOllamaModels().then(setOllamaModels).catch(() => {});
        }
        // Auto-resolved provider kan ha ändrats (Ollama up/down) — re-
        // fetcha aktiv stack så vänsterspaltens kort speglar verkligheten.
        activeStack().then(setStack).catch(() => {});
      },
    );
    const unInstallProgress = listen<OllamaInstallProgress>(
      "ollama_install_progress",
      (ev) => {
        setOllamaInstallProgress(ev.payload);
      },
    );
    const unInstallDone = listen<{ ok: boolean; error?: string }>(
      "ollama_install_done",
      (ev) => {
        setOllamaInstallBusy(false);
        if (!ev.payload.ok) {
          setOllamaInstallError(ev.payload.error ?? "okänt fel");
        } else {
          setOllamaInstallError(null);
          setOllamaInstallProgress(null);
          // Re-detect så badgen byter från "Installera" till "Installerad".
          refreshOllama();
        }
      },
    );
    return () => {
      unGoogleStatus.then((fn) => fn());
      unOllamaStatus.then((fn) => fn());
      unInstallProgress.then((fn) => fn());
      unInstallDone.then((fn) => fn());
    };
  }, []);

  // Lyssna på Ollama pull-progress events.
  useEffect(() => {
    const unProgress = listen<PullProgress>("ollama_pull_progress", (ev) => {
      setPullState(ev.payload);
    });
    const unDone = listen<{ model: string }>("ollama_pull_done", (ev) => {
      setPullState({
        model: ev.payload.model,
        status: "klar",
        total: null,
        completed: null,
        done: true,
      });
      setTimeout(() => setPullState(null), 2500);
      refreshOllama();
    });
    const unSttProgress = listen<SttModelDownloadProgress>(
      "stt_model_download_progress",
      (ev) => {
        setSttDownload({
          model: ev.payload.model,
          status: ev.payload.status,
          done: false,
        });
      },
    );
    const unSttDone = listen<{ model: string }>(
      "stt_model_download_done",
      (ev) => {
        setSttDownload({ model: ev.payload.model, status: "klar", done: true });
        setTimeout(() => setSttDownload(null), 2500);
        // Re-check HF-cache så dropdown-prefix uppdateras.
        MODELS.forEach(async (m) => {
          const cached = await checkHfCached(m.id).catch(() => false);
          setSttCached((prev) => ({ ...prev, [m.id]: cached }));
        });
      },
    );
    return () => {
      unProgress.then((fn) => fn());
      unDone.then((fn) => fn());
      unSttProgress.then((fn) => fn());
      unSttDone.then((fn) => fn());
    };
  }, []);

  async function handlePullOllama() {
    if (!draft) return;
    setPullState({
      model: draft.ollama_model,
      status: "startar…",
      total: null,
      completed: null,
      done: false,
    });
    try {
      await pullOllamaModel(draft.ollama_model);
    } catch (e) {
      setPullState(null);
      setError(`pull misslyckades: ${e}`);
    }
  }

  async function handleStartOllama() {
    setOllamaStartBusy(true);
    setOllamaStartError(null);
    try {
      const spawned = await ollamaStart();
      if (!spawned) {
        setOllamaStartError(
          "Ollama-binären hittades inte — installera först.",
        );
        setOllamaStartBusy(false);
        return;
      }
      // Polla `/api/tags` tills tjänsten svarar (max ~15 sek). Tray-
      // appen tar typiskt 2-4 sek att lyfta upp HTTP-servern.
      const deadline = Date.now() + 15_000;
      while (Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 800));
        try {
          const status = await ollamaStatus();
          if (status.online) {
            setOllamaOnline(true);
            setOllamaInstalled(status.installed);
            await refreshOllama();
            activeStack().then(setStack).catch(() => {});
            setOllamaStartBusy(false);
            return;
          }
        } catch {
          /* ignorera tillfälliga fel under uppstart */
        }
      }
      setOllamaStartError(
        "Tjänsten svarade inte inom 15 sek — tray-appen kanske inte hann starta.",
      );
    } catch (e) {
      setOllamaStartError(String(e));
    } finally {
      setOllamaStartBusy(false);
    }
  }

  async function handleStopOllama() {
    setOllamaStopBusy(true);
    setOllamaStartError(null);
    try {
      await ollamaStop();
      // Polla tills tjänsten verkligen är nere så badge byter direkt.
      const deadline = Date.now() + 5_000;
      while (Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 400));
        try {
          const status = await ollamaStatus();
          if (!status.online) {
            setOllamaOnline(false);
            setOllamaModels([]);
            activeStack().then(setStack).catch(() => {});
            break;
          }
        } catch {
          break;
        }
      }
    } catch (e) {
      setOllamaStartError(`Kunde inte stoppa Ollama: ${e}`);
    } finally {
      setOllamaStopBusy(false);
    }
  }

  async function handleInstallOllama() {
    setOllamaInstallBusy(true);
    setOllamaInstallError(null);
    setOllamaInstallProgress({ phase: "download_started", url: "" });
    try {
      await ollamaInstall();
    } catch (e) {
      // Fel-event triggas också via ollama_install_done — men om hela IPC
      // misslyckas (t.ex. permission denied) hamnar vi här i stället.
      setOllamaInstallBusy(false);
      setOllamaInstallError(String(e));
    }
  }

  async function handleDownloadStt(model: string) {
    setSttDownload({ model, status: "startar…", done: false });
    try {
      await downloadSttModel(model);
    } catch (e) {
      setError(`STT-download misslyckades: ${e}`);
      setSttDownload(null);
    }
  }

  useEffect(() => {
    const unlisten = listen<{ rms: number }>("mic_level", (ev) => {
      setMicLevel(ev.payload.rms);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function handleSave() {
    if (!draft) return;
    setSaving(true);
    setError(null);
    try {
      if (keyDraft !== null) {
        if (keyDraft.trim() === "") {
          await clearAnthropicKey();
          setKeyStored(false);
        } else {
          await setAnthropicKey(keyDraft.trim());
          setKeyStored(true);
        }
        setKeyDraft(null);
      }
      if (groqKeyDraft !== null) {
        if (groqKeyDraft.trim() === "") {
          await clearGroqKey();
          setGroqKeyStored(false);
        } else {
          await setGroqKey(groqKeyDraft.trim());
          setGroqKeyStored(true);
        }
        setGroqKeyDraft(null);
      }
      if (geminiKeyDraft !== null) {
        if (geminiKeyDraft.trim() === "") {
          await clearGeminiKey();
          setGeminiKeyStored(false);
        } else {
          await setGeminiKey(geminiKeyDraft.trim());
          setGeminiKeyStored(true);
        }
        setGeminiKeyDraft(null);
      }
      await setSettings(draft);
      setLoaded(draft);
      setSavedTick((t) => t + 1);
      // Re-fetcha Google-status efter save så "Anslut"-knappen enable:as
      // direkt om user precis fyllde i client-ID.
      googleConnectionStatus()
        .then(setGoogleStatus)
        .catch(() => {});
      activeStack()
        .then(setStack)
        .catch(() => {});
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  function handleReset() {
    if (loaded) setDraft(loaded);
    setKeyDraft(null);
    setGroqKeyDraft(null);
    setGeminiKeyDraft(null);
  }

  async function handleGoogleConnect() {
    setGoogleBusy(true);
    setError(null);
    try {
      await googleConnect();
      // Använd verify så vi vet att refresh-tokenen vi precis sparade
      // faktiskt ger en access-token — i stället för att bara konstatera
      // att den finns på disk.
      const status = await googleVerifyConnection();
      setGoogleStatus(status);
    } catch (e) {
      setError(String(e));
    } finally {
      setGoogleBusy(false);
    }
  }

  async function handleGoogleDisconnect() {
    setGoogleBusy(true);
    setError(null);
    try {
      await googleDisconnect();
      const status = await googleConnectionStatus();
      setGoogleStatus(status);
    } catch (e) {
      setError(String(e));
    } finally {
      setGoogleBusy(false);
    }
  }

  async function handleCheckUpdates() {
    setUpdateChecking(true);
    setUpdateError(null);
    try {
      const status = await checkForUpdates();
      setUpdateStatus(status);
    } catch (e) {
      setUpdateError(String(e));
    } finally {
      setUpdateChecking(false);
    }
  }

  if (!draft) {
    return (
      <div className="settings-root">
        <div className="loading-shell">laddar konfiguration…</div>
      </div>
    );
  }

  const dirty =
    JSON.stringify(draft) !== JSON.stringify(loaded) ||
    keyDraft !== null ||
    groqKeyDraft !== null ||
    geminiKeyDraft !== null;

  return (
    <div className="settings-root">
      {/* LEFT — wordmark + identity */}
      <aside className="settings-wordmark">
        <div>
          <div className="settings-monogram" aria-hidden>
            <SVoiceLogo size={56} />
          </div>
          <h1 className="settings-wordmark-title">
            SVoice
            <sub>by Stegberg · v3</sub>
          </h1>
          <p className="settings-wordmark-lede">
            Röststyrd produktivitetssvit för Windows. Diktera var som helst, omformulera
            markerad text, ställ frågor och hantera kalender + mail — allt via två tangenter.
          </p>
        </div>

        <div className="settings-wordmark-footer">
          <div className="active-stack" aria-label="Aktiv stack just nu">
            <div className="active-stack-header">
              <span className="active-stack-title">Aktiv stack</span>
              <span
                className={`active-stack-pulse${stack ? " live" : ""}`}
                aria-hidden
              />
            </div>
            {stack ? (
              <ul className="active-stack-rows">
                {(() => {
                  const sttD = describeActiveStt(stack.stt);
                  return (
                    <li className="active-stack-row">
                      <span className="active-stack-role">STT</span>
                      <span
                        className={`active-stack-badge tone-${sttD.badgeTone}`}
                      >
                        {sttD.badge}
                      </span>
                      <span className="active-stack-primary">
                        {sttD.primary}
                      </span>
                      <span className="active-stack-secondary">
                        {sttD.secondary}
                      </span>
                    </li>
                  );
                })()}
                {(() => {
                  const a = describeActiveLlm(stack.action_llm);
                  return (
                    <li className="active-stack-row">
                      <span className="active-stack-role">Action</span>
                      <span
                        className={`active-stack-badge tone-${a.badgeTone}`}
                      >
                        {a.badge}
                      </span>
                      <span className="active-stack-primary">{a.primary}</span>
                      <span className="active-stack-secondary">
                        {a.secondary}
                      </span>
                    </li>
                  );
                })()}
                {(() => {
                  const d = describeActiveLlm(stack.dictation_llm);
                  return (
                    <li className="active-stack-row">
                      <span className="active-stack-role">Polish</span>
                      <span
                        className={`active-stack-badge tone-${d.badgeTone}`}
                      >
                        {d.badge}
                      </span>
                      <span className="active-stack-primary">{d.primary}</span>
                      <span className="active-stack-secondary">
                        {d.secondary}
                      </span>
                    </li>
                  );
                })()}
              </ul>
            ) : (
              <div className="active-stack-loading">probar tjänster…</div>
            )}
          </div>
          <div className="active-stack-foot">
            Ingen telemetri · ljud endast i RAM
          </div>
        </div>
      </aside>

      {/* RIGHT — panel */}
      <section className="settings-panel">
        <header className="settings-panel-header">
          <h1 className="settings-panel-title">
            Inställningar<em>.</em>
          </h1>
          <div className="settings-panel-meta">
            <span
              key={savedTick}
              className={`save-status${savedTick > 0 && !dirty ? " visible" : ""}`}
              style={{ marginRight: 12 }}
            >
              <span className="tick">✓</span> sparat
            </span>
            %APPDATA%/svoice-v3/settings.json
          </div>
        </header>

        <nav className="settings-tabs" role="tablist">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={activeTab === t.id}
              className={`settings-tab${activeTab === t.id ? " active" : ""}`}
              onClick={() => setActiveTab(t.id)}
            >
              <span className="settings-tab-icon" aria-hidden>
                {t.icon}
              </span>
              <span className="settings-tab-label">{t.label}</span>
            </button>
          ))}
        </nav>

        {activeTab === "overview" && (<>
        {/* Kom igång — onboarding-checklist */}
        <article
          className="settings-section"
          style={{ gridTemplateColumns: "180px minmax(0, 1fr)" }}
        >
          <div className="settings-section-label">
            <h2>Kom igång</h2>
            <p>
              Minsta uppsättning för att börja använda SVoice. Alla steg är
              oberoende — välj det du vill köra.
            </p>
          </div>
          <div className="settings-section-body">
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 10,
                padding: "16px 18px",
                background: "rgba(243, 237, 227, 0.02)",
                border: "1px solid rgba(243, 237, 227, 0.06)",
                borderRadius: 12,
              }}
            >
              {(() => {
                const hasAnthropic = keyStored;
                const hasGroq = groqKeyStored;
                const hasGoogle = googleStatus.connected;
                const items: Array<{
                  ok: boolean;
                  title: string;
                  hint: string;
                }> = [
                  {
                    ok: true,
                    title: "1. Diktering",
                    hint: `Håll ${HOTKEY_LABELS[draft.dictation_hotkey]} och prata. Texten injiceras där markören står. Välj lokal eller Groq Whisper under "Transkribering".`,
                  },
                  {
                    ok: hasAnthropic || hasGroq,
                    title: "2. Action-LLM (Claude eller Groq)",
                    hint: hasAnthropic || hasGroq
                      ? `Håll ${HOTKEY_LABELS[draft.action_hotkey]} + säg kommando → AI-popup med svar. Markera text innan för transformering.`
                      : "Lägg till Anthropic- eller Groq-nyckel under 'Action-LLM' för att aktivera AI-popup.",
                  },
                  {
                    ok: hasGoogle,
                    title: "3. Google (valfritt)",
                    hint: hasGoogle
                      ? "Säg 'vad har jag i kalendern idag' eller 'sök mail från X'."
                      : "Koppla Google-konto under 'Integrationer' för kalender + mail via röst.",
                  },
                ];
                return items.map((it, i) => (
                  <div
                    key={i}
                    style={{
                      display: "flex",
                      alignItems: "flex-start",
                      gap: 12,
                    }}
                  >
                    <span
                      style={{
                        marginTop: 2,
                        fontSize: 14,
                        color: it.ok ? "#7bd37e" : "var(--ink-tertiary)",
                      }}
                    >
                      {it.ok ? "✓" : "○"}
                    </span>
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontWeight: 500, marginBottom: 2 }}>
                        {it.title}
                      </div>
                      <div
                        style={{
                          fontSize: 12,
                          color: "var(--ink-tertiary)",
                          lineHeight: 1.5,
                        }}
                      >
                        {it.hint}
                      </div>
                    </div>
                  </div>
                ));
              })()}
            </div>
          </div>
        </article>

        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Version</h2>
            <p>
              SVoice 3 uppdateras via nya MSI-installer från GitHub Releases.
              Auto-check körs en gång per dygn.
            </p>
          </div>
          <div className="settings-section-body">
            <div
              style={{
                padding: "14px 16px",
                background: updateStatus?.available
                  ? "rgba(212, 169, 85, 0.08)"
                  : "rgba(243, 237, 227, 0.02)",
                border: updateStatus?.available
                  ? "1px solid rgba(212, 169, 85, 0.28)"
                  : "1px solid rgba(243, 237, 227, 0.06)",
                borderRadius: 12,
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 12,
                  marginBottom: 6,
                }}
              >
                <span
                  style={{
                    width: 10,
                    height: 10,
                    borderRadius: 5,
                    background: updateStatus?.available
                      ? "#d4a955"
                      : updateStatus
                        ? "#7bd37e"
                        : "rgba(243, 237, 227, 0.3)",
                  }}
                />
                <div style={{ fontWeight: 500 }}>
                  {updateStatus?.available
                    ? `Ny version ${updateStatus.latest_version} tillgänglig`
                    : updateStatus
                      ? `Version ${updateStatus.current_version} (senaste)`
                      : "Kontrollerar version…"}
                </div>
                <div style={{ flex: 1 }} />
                {updateStatus?.available && updateStatus.download_url && (
                  <a
                    href={updateStatus.download_url}
                    target="_blank"
                    rel="noreferrer"
                    className="btn btn-primary btn-compact"
                    style={{ textDecoration: "none" }}
                  >
                    Ladda ner
                  </a>
                )}
                <button
                  type="button"
                  className="btn btn-ghost btn-compact"
                  onClick={handleCheckUpdates}
                  disabled={updateChecking}
                >
                  {updateChecking ? "Söker…" : "Sök uppdateringar"}
                </button>
              </div>
              {updateError && (
                <div
                  style={{
                    fontSize: 12,
                    color: "var(--danger)",
                    marginTop: 6,
                  }}
                >
                  {updateError}
                </div>
              )}
              {updateStatus?.release_notes && updateStatus.available && (
                <details style={{ marginTop: 10 }}>
                  <summary
                    style={{
                      fontSize: 12,
                      color: "var(--ink-tertiary)",
                      cursor: "pointer",
                    }}
                  >
                    Visa release-notes
                  </summary>
                  <pre
                    style={{
                      marginTop: 6,
                      fontSize: 12,
                      lineHeight: 1.5,
                      whiteSpace: "pre-wrap",
                      color: "var(--ink-secondary)",
                      fontFamily: "var(--font-sans)",
                    }}
                  >
                    {updateStatus.release_notes}
                  </pre>
                </details>
              )}
            </div>
          </div>
        </article>

        {/* Moduler — av/på-togglar */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Moduler</h2>
            <p>
              Slå av delar du inte använder för att spara VRAM och CPU. STT
              kräver modell-sidecar; Action-LLM kräver Ollama eller API-nyckel.
            </p>
          </div>
          <div className="settings-section-body">
            <ToggleRow
              label="Diktering (STT)"
              help="Höger Ctrl → tal-till-text → injection. Sidecar spawnar bara när aktiverat."
              value={draft.stt_enabled}
              onChange={(v) => setDraft({ ...draft, stt_enabled: v })}
            />
            <ToggleRow
              label="Action-LLM popup"
              help="Insert → kontextmedveten LLM-popup med selection-transform eller Q&A."
              value={draft.action_llm_enabled}
              onChange={(v) => setDraft({ ...draft, action_llm_enabled: v })}
            />
            <ToggleRow
              label="Starta automatiskt med Windows"
              help="Lägger SVoice i Windows startup-registret så den startar tyst i tray vid inloggning. Inget fönster öppnas — tray-ikonen är ingången."
              value={draft.autostart}
              onChange={(v) => setDraft({ ...draft, autostart: v })}
            />
          </div>
        </article>

        </>)}

        {activeTab === "audio" && (<>
        {/* Audio */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Audio</h2>
            <p>Mikrofon och ingångsnivå. Lämna tomt för systemets standard.</p>
          </div>
          <div className="settings-section-body">
            <div className="field">
              <label className="field-label" htmlFor="mic">
                Mikrofon
              </label>
              <select
                id="mic"
                className="select"
                value={draft.mic_device ?? ""}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    mic_device: e.target.value === "" ? null : e.target.value,
                  })
                }
              >
                <option value="">Systemets standard-mic</option>
                {micDevices.map((d) => (
                  <option key={d} value={d}>
                    {d}
                  </option>
                ))}
              </select>
              <div className="field-help">
                {micDevices.length} enheter upptäckta. Default-mic används om inget
                explicit val görs. Val av specifik enhet aktiveras i iter 4.
              </div>
            </div>
          </div>
        </article>

        {/* Transkription */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Transkribering</h2>
            <p>
              Välj mellan lokal KB-Whisper (privat, fungerar offline) eller
              Groq Whisper (snabb, kräver internet + gratis API-nyckel).
            </p>
          </div>
          <div className="settings-section-body">
            <div className="field">
              <label className="field-label" htmlFor="stt-provider">
                Provider
              </label>
              <select
                id="stt-provider"
                className="select"
                value={draft.stt_provider}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    stt_provider: e.target.value as SttProviderChoice,
                  })
                }
              >
                {(Object.keys(STT_PROVIDER_LABELS) as SttProviderChoice[]).map((p) => (
                  <option key={p} value={p}>
                    {STT_PROVIDER_LABELS[p]}
                  </option>
                ))}
              </select>
              <div className="field-help">
                {draft.stt_provider === "groq"
                  ? "Kräver Groq-nyckel nedan. Faller tillbaka till lokal STT vid nätfel."
                  : "Allt lokalt — ljud lämnar aldrig datorn."}
              </div>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="stt-language">
                Språk
              </label>
              <select
                id="stt-language"
                className="select"
                value={draft.stt_language}
                onChange={(e) =>
                  setDraft({ ...draft, stt_language: e.target.value })
                }
              >
                {STT_LANGUAGES.map((l) => (
                  <option key={l.code} value={l.code}>
                    {l.label}
                  </option>
                ))}
              </select>
              <div className="field-help">
                Whisper-modellerna stödjer ~100 språk. Fast val är snabbare än auto.
              </div>
            </div>

            {draft.stt_provider === "local" && (
              <>
                <div className="field">
                  <label className="field-label" htmlFor="model">
                    Lokal modell
                  </label>
                  <div className="field-with-action">
                    <select
                      id="model"
                      className="select"
                      value={draft.stt_model}
                      onChange={(e) => setDraft({ ...draft, stt_model: e.target.value })}
                    >
                      {MODELS.map((m) => {
                        const cached = sttCached[m.id];
                        const prefix = cached === undefined ? "…" : cached ? "✓" : "↓";
                        return (
                          <option key={m.id} value={m.id}>
                            {prefix} {m.label} — {m.note}
                          </option>
                        );
                      })}
                    </select>
                    {(() => {
                      const cached = sttCached[draft.stt_model];
                      const downloading =
                        sttDownload &&
                        sttDownload.model === draft.stt_model &&
                        !sttDownload.done;
                      if (cached === undefined) return null;
                      if (downloading) return null;
                      if (cached) return <span className="field-badge ok">✓ nedladdad</span>;
                      return (
                        <button
                          type="button"
                          className="btn btn-primary btn-compact"
                          onClick={() => handleDownloadStt(draft.stt_model)}
                        >
                          Ladda ner
                        </button>
                      );
                    })()}
                  </div>

                  {sttDownload && sttDownload.model === draft.stt_model && (
                    <div className="download-progress">
                      <div className="download-progress-label">
                        <span>{sttDownload.status}</span>
                      </div>
                      <div className="download-progress-bar">
                        <div
                          className="download-progress-fill"
                          style={{ width: sttDownload.done ? "100%" : "45%" }}
                        />
                      </div>
                    </div>
                  )}

                  <div className="field-help">
                    Rekommenderat minimum-VRAM: Base 1 GB · Medium 4 GB · Large 6 GB.
                    CPU-fallback funkar men är 5-10× långsammare.
                    {sttCached[draft.stt_model] === false && !sttDownload && (
                      <>
                        {" "}
                        Modellen är inte nedladdad — klicka "Ladda ner" innan första
                        användning (Base ~150 MB · Medium ~1,5 GB · Large ~3 GB).
                      </>
                    )}
                  </div>
                </div>

                <div className="field">
                  <label className="field-label">Beräkningsläge</label>
                  <div className="segmented" role="tablist">
                    {(Object.keys(COMPUTE_LABELS) as ComputeMode[]).map((mode) => (
                      <button
                        key={mode}
                        type="button"
                        role="tab"
                        aria-selected={draft.stt_compute_mode === mode}
                        className={draft.stt_compute_mode === mode ? "active" : ""}
                        onClick={() => setDraft({ ...draft, stt_compute_mode: mode })}
                      >
                        {COMPUTE_LABELS[mode]}
                      </button>
                    ))}
                  </div>
                  <div className="field-help">
                    Auto väljer GPU om CUDA-körningstid finns, annars CPU-fallback.
                  </div>
                </div>
              </>
            )}

            {draft.stt_provider === "groq" && (
              <div className="field">
                <label className="field-label" htmlFor="groq-stt-model">
                  Groq-modell
                </label>
                <select
                  id="groq-stt-model"
                  className="select"
                  value={draft.groq_stt_model}
                  onChange={(e) =>
                    setDraft({ ...draft, groq_stt_model: e.target.value })
                  }
                >
                  {GROQ_STT_MODELS.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.label} — {m.note}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </div>
        </article>

        </>)}

        {activeTab === "llm" && (<>
        {/* Action-popup (iter 3) */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Action-popup</h2>
            <p>
              Håll Insert och ge ett röstkommando för ett AI-svar i popup-fönstret.
              Markerad text tolkas som en transformations-uppgift, tomt fält som
              en Q&amp;A. Alla fyra providers kan användas — API-nycklarna nedan
              används även av dikterings-polering (Diktering-fliken) om den är på.
            </p>
          </div>
          <div className="settings-section-body">
            <div className="field">
              <label className="field-label">Provider</label>
              <div className="segmented" role="tablist">
                {(Object.keys(PROVIDER_LABELS) as LlmProviderChoice[]).map((p) => (
                  <button
                    key={`action-${p}`}
                    type="button"
                    role="tab"
                    aria-selected={draft.action_llm_provider === p}
                    className={draft.action_llm_provider === p ? "active" : ""}
                    onClick={() => setDraft({ ...draft, action_llm_provider: p })}
                  >
                    {PROVIDER_LABELS[p]}
                  </button>
                ))}
              </div>
              <div className="field-help">
                Svarar på röstkommandon i popup-fönstret. Claude krävs för
                web_search/Google-verktyg (agentic-flow). Auto: Ollama först,
                Claude som fallback.
              </div>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="ollama-model">
                Ollama-modell
              </label>
              <div className="field-with-action">
                <select
                  id="ollama-model"
                  className="select"
                  value={draft.ollama_model}
                  onChange={(e) =>
                    setDraft({ ...draft, ollama_model: e.target.value })
                  }
                >
                  {OLLAMA_MODELS.map((m) => {
                    const installed = ollamaModels.some((o) =>
                      o.name.startsWith(m.id.split(":")[0]) && o.name === m.id
                    );
                    return (
                      <option key={m.id} value={m.id}>
                        {installed ? "✓" : "↓"} {m.label} — {m.note}
                      </option>
                    );
                  })}
                </select>
                {(() => {
                  const installed = ollamaModels.some((o) => o.name === draft.ollama_model);
                  const pulling = pullState && pullState.model === draft.ollama_model && !pullState.done;
                  if (!ollamaOnline) {
                    // Binären finns men tjänsten är nere → erbjud klick-
                    // start. Saknas binären faller vi tillbaka till en
                    // muted "offline"-badge (Installera-knappen visas
                    // ändå längre ner i field-help-sektionen).
                    if (ollamaInstalled) {
                      return (
                        <button
                          type="button"
                          className="btn btn-primary btn-compact"
                          onClick={handleStartOllama}
                          disabled={ollamaStartBusy}
                        >
                          {ollamaStartBusy ? "Startar…" : "Starta Ollama"}
                        </button>
                      );
                    }
                    return <span className="field-badge muted">Ollama offline</span>;
                  }
                  if (pulling) return null;
                  // Tjänsten kör — erbjud Stoppa, plus visa modell-status.
                  return (
                    <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                      {installed ? (
                        <span className="field-badge ok">✓ installerad</span>
                      ) : (
                        <button
                          type="button"
                          className="btn btn-primary btn-compact"
                          onClick={handlePullOllama}
                        >
                          Ladda ner
                        </button>
                      )}
                      <button
                        type="button"
                        className="btn btn-ghost btn-compact"
                        onClick={handleStopOllama}
                        disabled={ollamaStopBusy}
                        title="Stoppa Ollama-tjänsten för att frigöra RAM"
                      >
                        {ollamaStopBusy ? "Stoppar…" : "Stoppa Ollama"}
                      </button>
                    </div>
                  );
                })()}
              </div>

              {pullState && pullState.model === draft.ollama_model && (
                <div className="download-progress">
                  <div className="download-progress-label">
                    <span>{pullState.status}</span>
                    {pullState.total && pullState.completed ? (
                      <span className="mono">
                        {formatBytes(pullState.completed)} / {formatBytes(pullState.total)}
                      </span>
                    ) : null}
                  </div>
                  <div className="download-progress-bar">
                    <div
                      className="download-progress-fill"
                      style={{
                        width:
                          pullState.total && pullState.completed
                            ? `${Math.min(100, (pullState.completed / pullState.total) * 100)}%`
                            : "5%",
                      }}
                    />
                  </div>
                </div>
              )}

              <div className="field-help">
                {ollamaOnline ? (
                  <>
                    Tjänsten kör. {ollamaModels.length} modeller installerade.{" "}
                    <strong>Qwen 2.5 14B</strong> ger bra balans för RTX 5080.
                    <br />
                    <span style={{ opacity: 0.75 }}>
                      Drar ~0,5-2 GB RAM medan den kör (mer när modellen är
                      laddad i VRAM). Klicka "Stoppa Ollama" när du är klar
                      för att frigöra minnet — du kan starta om när som helst.
                    </span>
                  </>
                ) : ollamaInstalled ? (
                  <>
                    Ollama är installerat men <strong>inte igång</strong>. SVoice
                    startar inte tjänsten automatiskt eftersom den drar
                    0,5-2 GB RAM bara av att stå i bakgrunden — klicka
                    "Starta Ollama" ovan när du vill använda lokal LLM.
                    {ollamaStartError && (
                      <div className="field-error" style={{ marginTop: 6 }}>
                        {ollamaStartError}
                      </div>
                    )}
                  </>
                ) : ollamaInstallSupported ? (
                  <>
                    Ollama är inte installerat. Klicka nedan för att ladda
                    ned och installera direkt — UAC-prompten visas av
                    Windows när installern startar.
                  </>
                ) : (
                  <>
                    Ollama-service inte detekterad på{" "}
                    <code>{draft.ollama_url}</code>. Installera från{" "}
                    <code>ollama.com</code> och starta tjänsten manuellt
                    (auto-install stöds bara på Windows just nu).
                  </>
                )}
              </div>

              {!ollamaOnline && ollamaInstalled === false && ollamaInstallSupported && (
                <div className="field" style={{ marginTop: 8 }}>
                  {!ollamaInstallBusy && !ollamaInstallProgress && (
                    <button
                      type="button"
                      className="btn btn-primary btn-compact"
                      onClick={handleInstallOllama}
                    >
                      Installera Ollama (~700 MB)
                    </button>
                  )}
                  {ollamaInstallProgress && (
                    <div className="download-progress">
                      <div className="download-progress-label">
                        <span>{installPhaseLabel(ollamaInstallProgress)}</span>
                        {ollamaInstallProgress.phase === "download_progress" &&
                        ollamaInstallProgress.total ? (
                          <span className="mono">
                            {formatBytes(ollamaInstallProgress.downloaded)} /{" "}
                            {formatBytes(ollamaInstallProgress.total)}
                          </span>
                        ) : null}
                      </div>
                      <div className="download-progress-bar">
                        <div
                          className="download-progress-fill"
                          style={{
                            width:
                              ollamaInstallProgress.phase === "download_progress" &&
                              ollamaInstallProgress.total
                                ? `${Math.min(100, (ollamaInstallProgress.downloaded / ollamaInstallProgress.total) * 100)}%`
                                : ollamaInstallProgress.phase === "installing"
                                  ? "70%"
                                  : ollamaInstallProgress.phase === "waiting_for_service"
                                    ? "92%"
                                    : ollamaInstallProgress.phase === "done"
                                      ? "100%"
                                      : "5%",
                          }}
                        />
                      </div>
                    </div>
                  )}
                  {ollamaInstallError && (
                    <div className="field-help" style={{ color: "#d97070", marginTop: 6 }}>
                      Installation misslyckades: {ollamaInstallError}
                    </div>
                  )}
                </div>
              )}
            </div>

            <div className="field">
              <label className="field-label" htmlFor="api-key">
                Anthropic API-nyckel
              </label>
              <input
                id="api-key"
                className="input"
                type="password"
                placeholder={keyStored && keyDraft === null ? "••••••••" : "sk-ant-…"}
                value={keyDraft ?? ""}
                onChange={(e) => setKeyDraft(e.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
              <div className="field-help">
                {keyDraft === ""
                  ? "Nyckeln raderas när du sparar."
                  : "Sparas säkert i Windows Credential Manager."}
              </div>
              {keyStored && keyDraft === null && (
                <button
                  type="button"
                  className="link-button"
                  onClick={() => setKeyDraft("")}
                >
                  Rensa nyckel
                </button>
              )}
            </div>

            <div className="field">
              <label className="field-label" htmlFor="anthropic-model">
                Anthropic-modell
              </label>
              <select
                id="anthropic-model"
                className="select"
                value={draft.anthropic_model}
                onChange={(e) =>
                  setDraft({ ...draft, anthropic_model: e.target.value })
                }
              >
                <option value="claude-haiku-4-5-20251001">
                  Claude Haiku 4.5 — snabbast, billigast
                </option>
                <option value="claude-sonnet-4-5">
                  Claude Sonnet 4.5 — balans
                </option>
                <option value="claude-sonnet-4-6">
                  Claude Sonnet 4.6 — senaste Sonnet
                </option>
                <option value="claude-opus-4-7">
                  Claude Opus 4.7 — högsta kvalitet
                </option>
              </select>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="groq-key">
                Groq API-nyckel
              </label>
              <input
                id="groq-key"
                className="input"
                type="password"
                placeholder={
                  groqKeyStored && groqKeyDraft === null ? "••••••••" : "gsk_…"
                }
                value={groqKeyDraft ?? ""}
                onChange={(e) => setGroqKeyDraft(e.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
              <div className="field-help">
                {groqKeyDraft === ""
                  ? "Nyckeln raderas när du sparar."
                  : "Skapa gratis-nyckel på console.groq.com/keys. Samma nyckel används för både STT och LLM."}
              </div>
              {groqKeyStored && groqKeyDraft === null && (
                <button
                  type="button"
                  className="link-button"
                  onClick={() => setGroqKeyDraft("")}
                >
                  Rensa nyckel
                </button>
              )}
            </div>

            <div className="field">
              <label className="field-label" htmlFor="groq-llm-model">
                Groq-modell
              </label>
              <select
                id="groq-llm-model"
                className="select"
                value={draft.groq_llm_model}
                onChange={(e) =>
                  setDraft({ ...draft, groq_llm_model: e.target.value })
                }
              >
                {GROQ_LLM_MODELS.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label} — {m.note}
                  </option>
                ))}
              </select>
              <div className="field-help">
                Används när provider är Groq eller Auto (fallback efter Ollama).
              </div>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="gemini-key">
                Gemini API-nyckel
              </label>
              <input
                id="gemini-key"
                className="input"
                type="password"
                placeholder={
                  geminiKeyStored && geminiKeyDraft === null ? "••••••••" : "AI…"
                }
                value={geminiKeyDraft ?? ""}
                onChange={(e) => setGeminiKeyDraft(e.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
              <div className="field-help">
                {geminiKeyDraft === ""
                  ? "Nyckeln raderas när du sparar."
                  : "Skapa gratis-nyckel på aistudio.google.com/apikey. Gemini 2.5 Flash har inbyggd Google Search-grounding — skarpare på realtidsdata än Claude web_search."}
              </div>
              {geminiKeyStored && geminiKeyDraft === null && (
                <button
                  type="button"
                  className="link-button"
                  onClick={() => setGeminiKeyDraft("")}
                >
                  Rensa nyckel
                </button>
              )}
            </div>

            <div className="field">
              <label className="field-label" htmlFor="gemini-model">
                Gemini-modell
              </label>
              <select
                id="gemini-model"
                className="select"
                value={draft.gemini_model}
                onChange={(e) =>
                  setDraft({ ...draft, gemini_model: e.target.value })
                }
              >
                {GEMINI_MODELS.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label} — {m.note}
                  </option>
                ))}
              </select>
              <div className="field-help">
                Flash är standard (snabb, billig, räcker för de flesta frågor).
                Pro är smartare men långsammare + dyrare.
              </div>
            </div>
          </div>
        </article>

        </>)}

        {activeTab === "hotkeys" && (<>
        {/* Snabbkommandon */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Snabbkommandon</h2>
            <p>
              Vilken tangent som ska hållas för diktering respektive action-popup.
              Samma tangent kan inte användas för båda.
            </p>
          </div>
          <div className="settings-section-body">
            <div className="field">
              <label className="field-label" htmlFor="dict-hotkey">
                Dikterings-tangent
              </label>
              <select
                id="dict-hotkey"
                className="select"
                value={draft.dictation_hotkey}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    dictation_hotkey: e.target.value as HotKeyChoice,
                  })
                }
              >
                {HOTKEY_ORDER.map((k) => (
                  <option key={k} value={k} disabled={k === draft.action_hotkey}>
                    {HOTKEY_LABELS[k]}
                  </option>
                ))}
              </select>
              <div className="field-help">
                Håll nedtryckt för att spela in. Default: Höger Ctrl.
              </div>
            </div>
            <div className="field">
              <label className="field-label" htmlFor="action-hotkey">
                Action-popup-tangent
              </label>
              <select
                id="action-hotkey"
                className="select"
                value={draft.action_hotkey}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    action_hotkey: e.target.value as HotKeyChoice,
                  })
                }
              >
                {HOTKEY_ORDER.map((k) => (
                  <option key={k} value={k} disabled={k === draft.dictation_hotkey}>
                    {HOTKEY_LABELS[k]}
                  </option>
                ))}
              </select>
              <div className="field-help">
                Öppnar LLM-popupen. Default: Insert.
              </div>
            </div>
            <div
              className="field-help"
              style={{ marginTop: 8, fontStyle: "italic", opacity: 0.7 }}
            >
              Hot-reload aktivt — ändringen träder i kraft direkt när du sparar.
            </div>
          </div>
        </article>

        </>)}

        {activeTab === "integrations" && (<>
        {/* Integrationer */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Integrationer</h2>
            <p>
              Koppla externa tjänster för agentic action-LLM (lägg till
              möten, sök mail via språk-kommandon).
            </p>
          </div>
          <div className="settings-section-body">
            <div className="field">
              <label className="field-label" htmlFor="google-client-id">
                Google OAuth client-ID
              </label>
              <input
                id="google-client-id"
                className="input"
                type="text"
                placeholder="1234-xxx.apps.googleusercontent.com"
                value={draft.google_oauth_client_id ?? ""}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    google_oauth_client_id:
                      e.target.value.trim() === "" ? null : e.target.value.trim(),
                  })
                }
                autoComplete="off"
                spellCheck={false}
              />
              <div className="field-help">
                Skapa en OAuth-client i{" "}
                <a
                  href="https://console.cloud.google.com/apis/credentials"
                  target="_blank"
                  rel="noreferrer"
                  style={{ color: "var(--color-amber, #d4a955)" }}
                >
                  Google Cloud Console
                </a>{" "}
                som typ <em>Desktop app</em>. Google ger både ID och
                secret — fyll i båda nedan.
              </div>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="google-client-secret">
                Google OAuth client-secret
              </label>
              <input
                id="google-client-secret"
                className="input"
                type="password"
                placeholder="GOCSPX-…"
                value={draft.google_oauth_client_secret ?? ""}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    google_oauth_client_secret:
                      e.target.value.trim() === "" ? null : e.target.value.trim(),
                  })
                }
                autoComplete="off"
                spellCheck={false}
              />
              <div className="field-help">
                Från samma OAuth-client i Google Cloud. Secret är inte
                hemligt i native apps (kan extraheras från binären), men
                Google kräver att det skickas i token-exchange.
              </div>
            </div>

            <div
              className="field"
              style={{
                marginTop: 4,
                padding: "14px 16px",
                background: "rgba(212, 169, 85, 0.06)",
                border: "1px solid rgba(212, 169, 85, 0.18)",
                borderRadius: 10,
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <span
                  aria-hidden
                  style={{
                    width: 10,
                    height: 10,
                    borderRadius: 5,
                    background: googleStatus.connected
                      ? "#7bd37e"
                      : googleStatus.client_id_configured
                        ? "#d4a955"
                        : "rgba(243, 237, 227, 0.3)",
                  }}
                />
                <div style={{ fontWeight: 500 }}>
                  Google ·{" "}
                  {googleStatus.connected
                    ? "ansluten"
                    : googleStatus.client_id_configured
                      ? "redo att anslutas"
                      : "client-ID saknas"}
                </div>
                <div style={{ flex: 1 }} />
                {googleStatus.connected ? (
                  <button
                    type="button"
                    className="button button-ghost"
                    onClick={handleGoogleDisconnect}
                    disabled={googleBusy}
                  >
                    {googleBusy ? "Kopplar från…" : "Koppla från"}
                  </button>
                ) : (
                  <button
                    type="button"
                    className="button"
                    onClick={handleGoogleConnect}
                    disabled={googleBusy || !googleStatus.client_id_configured}
                  >
                    {googleBusy ? "Ansluter…" : "Anslut Google-konto"}
                  </button>
                )}
              </div>
              <div
                className="field-help"
                style={{ marginTop: 8, marginBottom: 0 }}
              >
                {googleStatus.verify_state === "revoked"
                  ? "Token avvisades av Google (revokat eller inaktivt > 6 mån). Klicka 'Anslut' för att godkänna på nytt."
                  : googleStatus.verify_state === "transient"
                    ? "Kunde inte verifiera mot Google just nu (nätverksfel). Statusen kan vara inaktuell — vi pingar igen om några minuter."
                    : googleStatus.connected
                      ? "Refresh-token sparad i Windows Credential Manager. Frånkoppling raderar den lokalt."
                      : googleStatus.client_id_configured
                        ? "Klick öppnar browser för godkännande. Scopes: Calendar (läs/skriv) + Gmail (läs)."
                        : "Fyll i client-ID ovan och spara inställningarna, sedan kan du ansluta."}
              </div>
            </div>
          </div>
        </article>

        {/* Smart-functions */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Smart-functions</h2>
            <p>
              Återanvändbara prompts för vanliga redigeringsuppgifter. Appen
              seedar 5 svenska defaults första gången. Du kan redigera eller
              lägga till egna som JSON-filer.
            </p>
          </div>
          <div className="settings-section-body">
            {smartFns.length === 0 ? (
              <div className="field-help" style={{ fontStyle: "italic" }}>
                Inga smart-functions hittades. Starta om appen så seedas
                defaults.
              </div>
            ) : (
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 10,
                  marginBottom: 16,
                }}
              >
                {smartFns.map((sf) => (
                  <div
                    key={sf.id}
                    style={{
                      padding: "12px 14px",
                      background: "rgba(243, 237, 227, 0.03)",
                      border: "1px solid rgba(243, 237, 227, 0.06)",
                      borderRadius: 10,
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 10,
                        marginBottom: 4,
                      }}
                    >
                      <span style={{ fontWeight: 500 }}>{sf.name}</span>
                      <span
                        style={{
                          fontSize: 10,
                          padding: "2px 7px",
                          borderRadius: 4,
                          background:
                            sf.mode === "transform"
                              ? "rgba(212, 169, 85, 0.16)"
                              : "rgba(123, 211, 126, 0.14)",
                          color:
                            sf.mode === "transform" ? "#d4a955" : "#7bd37e",
                          textTransform: "uppercase",
                          letterSpacing: "0.06em",
                          fontFamily: "var(--font-mono)",
                        }}
                      >
                        {sf.mode}
                      </span>
                    </div>
                    <div
                      className="field-help"
                      style={{ marginBottom: 0, marginTop: 0 }}
                    >
                      {sf.description}
                    </div>
                  </div>
                ))}
              </div>
            )}
            <button
              type="button"
              className="link-button"
              onClick={() => openSmartFunctionsDir()}
            >
              Öppna mappen i Explorer
            </button>
            <div className="field-help" style={{ marginTop: 6 }}>
              Command palette (Ctrl+Shift+Space) för att snabb-triggern smart-functions
              kommer i senare iter.
            </div>
          </div>
        </article>

        </>)}

        {activeTab === "audio" && (<>
        {/* Röstdetektion — del av Diktering-fliken */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Tystnadströskel</h2>
            <p>
              Hur känslig mic-en ska vara. Ljud under denna nivå räknas som tystnad och
              klipps bort i början och slutet av inspelningen.
            </p>
          </div>
          <div className="settings-section-body">
            <div className="field">
              <label className="field-label" htmlFor="vad">
                Känslighet
              </label>
              <div className="slider-row">
                <input
                  id="vad"
                  className="slider"
                  type="range"
                  min="0"
                  max="0.05"
                  step="0.001"
                  value={draft.vad_threshold}
                  onChange={(e) =>
                    setDraft({ ...draft, vad_threshold: Number(e.target.value) })
                  }
                />
                <div className="slider-value">{draft.vad_threshold.toFixed(3)}</div>
              </div>
              <div className="slider-scale">
                <span>↓ fångar svagt tal</span>
                <span>↑ ignorerar rum-brus</span>
              </div>

              {/* Live mic-meter — visar aktuell ingångsnivå. Bar fylls bärnsten
                  när över tröskel (tal detekterat), grå under (tystnad). */}
              <div
                className="mic-meter"
                title="Live mic-nivå"
                role="meter"
                aria-valuenow={Math.round(micLevel * 1000) / 1000}
                aria-valuemin={0}
                aria-valuemax={0.05}
              >
                <div
                  className={
                    "mic-meter-fill" +
                    (micLevel > draft.vad_threshold ? " active" : "")
                  }
                  style={{ width: `${Math.min(100, (micLevel / 0.05) * 100)}%` }}
                />
                <div
                  className="mic-meter-threshold"
                  style={{ left: `${(draft.vad_threshold / 0.05) * 100}%` }}
                  aria-hidden
                />
              </div>
              <div className="mic-meter-legend">
                <span>
                  {micLevel > draft.vad_threshold ? "🎙 tal upptäckt" : "tystnad"}
                </span>
                <span className="mono">RMS {micLevel.toFixed(3)}</span>
              </div>

              <div className="field-help">
                Standard är 0.005. Tröskeln är linjen i mätaren — tala normalt och
                justera slidern så att din röst är över linjen men bakgrundsbrus under.
              </div>
            </div>
          </div>
        </article>

        {/* Dikterings-polering — LLM-postprocessing av transkriberingen */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Dikterings-polering</h2>
            <p>
              Kör transkriberingen genom en LLM för grammatik- och stavnings-
              fixar innan den injiceras. Långsammare (~300-700 ms extra) men
              vassare resultat.
            </p>
          </div>
          <div className="settings-section-body">
            <ToggleRow
              label="Aktivera LLM-polering"
              help="När avstängd injiceras rå-transkriberingen direkt från Whisper."
              value={draft.llm_polish_dictation}
              onChange={(v) => setDraft({ ...draft, llm_polish_dictation: v })}
            />

            <div
              className="field"
              style={{
                opacity: draft.llm_polish_dictation ? 1 : 0.45,
                pointerEvents: draft.llm_polish_dictation ? "auto" : "none",
              }}
              aria-disabled={!draft.llm_polish_dictation}
            >
              <label className="field-label">Provider för polering</label>
              <div className="segmented" role="tablist">
                {(Object.keys(PROVIDER_LABELS) as LlmProviderChoice[]).map((p) => (
                  <button
                    key={`dict-${p}`}
                    type="button"
                    role="tab"
                    aria-selected={draft.dictation_llm_provider === p}
                    className={draft.dictation_llm_provider === p ? "active" : ""}
                    disabled={!draft.llm_polish_dictation}
                    onClick={() => setDraft({ ...draft, dictation_llm_provider: p })}
                  >
                    {PROVIDER_LABELS[p]}
                  </button>
                ))}
              </div>
              <div className="field-help">
                {draft.llm_polish_dictation
                  ? "API-nycklar + modeller för varje provider konfigureras i Action-popup-fliken — de delas mellan dikterings-polering och action-popup."
                  : "Aktivera polering ovan för att välja provider."}
              </div>
            </div>
          </div>
        </article>

        </>)}

        {activeTab === "advanced" && (<>

        {/* STT-parametrar */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>STT-parametrar</h2>
            <p>
              Finjustering av Whisper-inferens. Standardvärdena är valda för god
              svensk diktering — ändra bara om du vet vad du gör. Alla ändringar
              tillämpas direkt, utan omstart.
            </p>
          </div>
          <div className="settings-section-body">
            <ToggleRow
              label="VAD-filter (Silero)"
              help="Filtrerar tystnader och icke-tal inuti ljudet innan transkribering. Ger robustare STT mot bakgrundsljud och andningar. Stäng av om du upplever att slutet av meningar klipps."
              value={draft.stt_vad_filter}
              onChange={(v) => setDraft({ ...draft, stt_vad_filter: v })}
              details={
                <FieldDetails>
                  <DetailsRow label="Om på">
                    Whispers inbyggda röstdetektor klipper tystnader <em>inuti</em>{" "}
                    talsegmentet innan transkribering. Mindre risk för hallucinationer
                    i pauser (som &quot;tack för att du tittade&quot;-artefakter) och
                    robustare mot bakgrundsljud.
                  </DetailsRow>
                  <DetailsRow label="Om av">
                    Hela ljudet skickas orörd till modellen. Inget klipps bort — bra
                    om du ofta upplever att sista ordet försvinner, men öppnar för
                    fler hallucinationer i tysta passager.
                  </DetailsRow>
                </FieldDetails>
              }
            />

            <div className="field">
              <label className="field-label" htmlFor="stt-initial-prompt">
                Initial prompt
              </label>
              <button
                id="stt-initial-prompt"
                type="button"
                className="prompt-preview"
                onClick={() => {
                  setPromptModalDraft(draft.stt_initial_prompt);
                  setPromptModalOpen(true);
                }}
              >
                <span className="prompt-preview-text">
                  {draft.stt_initial_prompt.trim() || (
                    <span className="prompt-preview-placeholder">
                      Klicka för att skriva prompt…
                    </span>
                  )}
                </span>
                <span className="prompt-preview-icon" aria-hidden>
                  ✎
                </span>
              </button>
              <div className="field-help">
                Kort text som matas in som historisk kontext till Whisper. Stabiliserar
                stil och kan förbättra igenkänning av fackord (t.ex. medicinska termer
                om du skriver det i prompten). Klicka för att redigera i stor editor.
              </div>
              <FieldDetails>
                <DetailsRow label="Använd för">
                  Att biasa Whisper mot svensk stil och rätt interpunktion, eller för
                  att nämna fackord du dikterar ofta (medicinska/juridiska termer,
                  produktnamn). Modellen blir bättre på att känna igen dem.
                </DetailsRow>
                <DetailsRow label="Tips">
                  Håll prompten kort (1-2 meningar). Långa prompts styr paradoxalt
                  nog mindre — modellen &quot;späds ut&quot; och börjar försöka
                  efterlikna promptens egen stil istället för att lyssna.
                </DetailsRow>
              </FieldDetails>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="stt-beam">
                Beam size
              </label>
              <div className="slider-row">
                <input
                  id="stt-beam"
                  className="slider"
                  type="range"
                  min="1"
                  max="10"
                  step="1"
                  value={draft.stt_beam_size}
                  onChange={(e) =>
                    setDraft({ ...draft, stt_beam_size: Number(e.target.value) })
                  }
                />
                <div className="slider-value">{draft.stt_beam_size}</div>
              </div>
              <div className="slider-scale">
                <span>↓ snabbare (greedy)</span>
                <span>↑ bättre kvalitet</span>
              </div>
              <div className="field-help">
                Antal hypoteser Whisper utvärderar parallellt. 5 är en bra balans
                mellan kvalitet och hastighet på KB-Large.
              </div>
              <FieldDetails>
                <DetailsRow label="Lägre (1-3)">
                  Greedy eller nästan-greedy sökning — modellen tar första rimliga
                  ord och går vidare. Snabbt men kan missa ovanliga ord eller
                  korrekt homofon (t.ex. &quot;hel&quot; vs &quot;häl&quot;).
                </DetailsRow>
                <DetailsRow label="Högre (5-10)">
                  Fler parallella hypoteser utvärderas och den mest sannolika väljs.
                  Bättre kvalitet på fackord och svårhörda passager, men inferensen
                  tar längre tid. Över 5 ger oftast bara marginell vinst.
                </DetailsRow>
              </FieldDetails>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="stt-nospeech">
                No-speech threshold
              </label>
              <div className="slider-row">
                <input
                  id="stt-nospeech"
                  className="slider"
                  type="range"
                  min="0.1"
                  max="0.9"
                  step="0.05"
                  value={draft.stt_no_speech_threshold}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      stt_no_speech_threshold: Number(e.target.value),
                    })
                  }
                />
                <div className="slider-value">
                  {draft.stt_no_speech_threshold.toFixed(2)}
                </div>
              </div>
              <div className="slider-scale">
                <span>↓ fångar svagt tal</span>
                <span>↑ mindre hallucinering</span>
              </div>
              <div className="field-help">
                Segment vars no-speech-sannolikhet ligger över tröskeln filtreras
                bort. Höj om du ser &quot;hallucinerade&quot; meningar i tysta
                passager. Sänk om appen missar korta ord.
              </div>
              <FieldDetails>
                <DetailsRow label="Lägre (0.1-0.4)">
                  Tolerant filter: fler segment släpps igenom. Bra om du har tyst
                  eller lågmäld röst, men risk att modellen börjar skriva ut
                  hallucinationer från brus eller andhämtningar.
                </DetailsRow>
                <DetailsRow label="Högre (0.6-0.9)">
                  Strängare: segment där modellen är osäker om det är tal kastas.
                  Färre hallucinationer men risk att korta/svaga ord missas. Höj
                  om du ofta ser påhittade meningar i tysta passager.
                </DetailsRow>
              </FieldDetails>
            </div>

            <ToggleRow
              label="Condition on previous text"
              help="Feedar tidigare transkript tillbaka till modellen som kontext. Förbättrar koherens i lång flytande diktering, men kan trunkera vid naturliga pauser. Rekommenderas av för dikteringsflöden med pauser."
              value={draft.stt_condition_on_previous_text}
              onChange={(v) =>
                setDraft({ ...draft, stt_condition_on_previous_text: v })
              }
              details={
                <FieldDetails>
                  <DetailsRow label="Om på">
                    Modellen får föregående transkript som kontext inför nästa.
                    Ger bättre sammanhang och koherens vid lång, sammanhängande
                    diktering.
                  </DetailsRow>
                  <DetailsRow label="Om av">
                    Varje segment tolkas fristående. Säkrare vid pauser eller
                    ämnesbyten — eventuella fel sprider sig inte framåt.
                    Rekommenderat för PTT-diktering där varje tryck är fristående.
                  </DetailsRow>
                </FieldDetails>
              }
            />

            <div className="field">
              <label className="field-label" htmlFor="vad-pad">
                VAD-trim padding
              </label>
              <div className="slider-row">
                <input
                  id="vad-pad"
                  className="slider"
                  type="range"
                  min="0"
                  max="500"
                  step="25"
                  value={draft.vad_trim_padding_ms}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      vad_trim_padding_ms: Number(e.target.value),
                    })
                  }
                />
                <div className="slider-value">
                  {draft.vad_trim_padding_ms} ms
                </div>
              </div>
              <div className="slider-scale">
                <span>↓ snävare trim</span>
                <span>↑ mer marginal</span>
              </div>
              <div className="field-help">
                Padding före och efter det detekterade talet innan ljudet går
                till Whisper. Utan padding klipps tonlösa konsonanter (s, f, t,
                k) ibland bort. Höj om du upplever att ord kapas i början eller
                slutet.
              </div>
              <FieldDetails>
                <DetailsRow label="Lägre (0-100 ms)">
                  Snäv trim — ljudet klipps exakt där tal detekteras. Risk att
                  mjuka konsonanter (s, f, t, k) tappas eftersom deras energi
                  ligger under tröskeln. &quot;sekund&quot; kan bli &quot;ekund&quot;.
                </DetailsRow>
                <DetailsRow label="Högre (250-500 ms)">
                  Mer marginal — säkrare mot klippning. Whisper ignorerar extra
                  tystnad så det kostar inget i kvalitet. 250 ms räcker för de
                  flesta; höj till 400-500 om ord fortfarande kapas.
                </DetailsRow>
              </FieldDetails>
            </div>

            <div className="field">
              <label className="field-label" htmlFor="auto-space">
                Auto-mellanslag vid paus
              </label>
              <div className="slider-row">
                <input
                  id="auto-space"
                  className="slider"
                  type="range"
                  min="0"
                  max="120"
                  step="5"
                  value={draft.dictation_auto_space_seconds}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      dictation_auto_space_seconds: Number(e.target.value),
                    })
                  }
                />
                <div className="slider-value">
                  {draft.dictation_auto_space_seconds === 0
                    ? "av"
                    : `${draft.dictation_auto_space_seconds} s`}
                </div>
              </div>
              <div className="slider-scale">
                <span>0 = av</span>
                <span>↑ längre fönster</span>
              </div>
              <div className="field-help">
                När du dikterar, pausar och dikterar igen inom detta fönster
                lägger appen automatiskt till ett mellanslag mellan
                segmenten — så du slipper trycka space själv. Sätt till 0 för
                att stänga av.
              </div>
              <FieldDetails>
                <DetailsRow label="0 (av)">
                  Ingen automatik. Du trycker space själv mellan dikteringar.
                </DetailsRow>
                <DetailsRow label="10-30 s">
                  Rekommenderat för de flesta. Korta tänkpauser mellan meningar
                  länkas naturligt utan att störa flödet.
                </DetailsRow>
                <DetailsRow label="60-120 s">
                  Även längre pauser räknas som samma diktering. Risk att ett
                  mellanslag läggs in om du bytt app eller börjat ett nytt fält
                  inom fönstret — då får du en oönskad ledande space.
                </DetailsRow>
              </FieldDetails>
            </div>

            <div className="field" style={{ marginTop: 8 }}>
              <button
                type="button"
                className="btn-reset"
                onClick={() => setDraft({ ...draft, ...STT_DEFAULTS })}
              >
                <span className="btn-reset-icon" aria-hidden>↺</span>
                Återställ till rekommenderade
              </button>
              <div className="field-help">
                Nollställer alla fält ovan till de värden som SVoice levereras
                med. Övriga inställningar (modell, provider, språk) behålls.
              </div>
            </div>
          </div>
        </article>

        </>)}

        {activeTab === "help" && (<>

        {/* Sektion 1 — Så fungerar appen */}
        <article className="settings-section" style={{ gridTemplateColumns: "180px minmax(0, 1fr)" }}>
          <div className="settings-section-label">
            <h2>Så fungerar appen</h2>
            <p>
              SVoice kör i bakgrunden (tray-ikon) och lyssnar på två tangenter.
              Allt sker utan att byta app eller fönster.
            </p>
          </div>
          <div className="settings-section-body">
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
              <div style={{ padding: "14px 16px", background: "rgba(243,237,227,0.02)", border: "1px solid rgba(243,237,227,0.06)", borderRadius: 10 }}>
                <div style={{ fontWeight: 500, marginBottom: 6 }}>Diktering — Höger Ctrl</div>
                <div className="field-help" style={{ marginBottom: 0 }}>
                  Håll tangenten, prata, släpp. Whisper-modellen transkriberar och
                  texten injiceras där markören står — i vilket textfält som helst.
                  Kräver ingen nätuppkoppling med lokal STT.
                </div>
              </div>
              <div style={{ padding: "14px 16px", background: "rgba(243,237,227,0.02)", border: "1px solid rgba(243,237,227,0.06)", borderRadius: 10 }}>
                <div style={{ fontWeight: 500, marginBottom: 6 }}>Action-popup — Insert</div>
                <div className="field-help" style={{ marginBottom: 0 }}>
                  Håll tangenten, ge ett kommando, släpp. Har du markerad text tolkas
                  det som en transformations-uppgift ("gör mer formellt"). Utan
                  markering är det Q&amp;A eller kalender/mail-kommandon.
                </div>
              </div>
            </div>
          </div>
        </article>

        {/* Sektion 2 — Första uppsättning */}
        <article className="settings-section" style={{ gridTemplateColumns: "180px minmax(0, 1fr)" }}>
          <div className="settings-section-label">
            <h2>Första uppsättning</h2>
            <p>Följ stegen i ordning — diktering fungerar utan API-nyckel.</p>
          </div>
          <div className="settings-section-body">
            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              {[
                {
                  n: "1",
                  title: "Välj mikrofon",
                  hint: 'Diktering-fliken → Audio. Lämna tomt för Windows systemstandard.',
                },
                {
                  n: "2",
                  title: "Välj och ladda ner STT-modell",
                  hint: 'Diktering-fliken → Transkribering → välj "KB-Whisper Base" → klicka Ladda ner (~150 MB). Base räcker för de flesta.',
                },
                {
                  n: "3",
                  title: "Lägg till API-nyckel för AI-funktioner (valfritt)",
                  hint: 'Action-popup-fliken → välj provider → klistra in nyckel. Se "Skaffa API-nycklar" nedan.',
                },
                {
                  n: "4",
                  title: "Koppla Google-konto (valfritt)",
                  hint: 'Integrationer-fliken → fyll i OAuth client-ID + secret → Anslut. Ger kalender- och mail-röststyrning. Se "Google OAuth" nedan.',
                },
              ].map((it) => (
                <div key={it.n} style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
                  <span style={{
                    flexShrink: 0,
                    width: 22,
                    height: 22,
                    borderRadius: 11,
                    background: "rgba(212,169,85,0.14)",
                    border: "1px solid rgba(212,169,85,0.3)",
                    color: "#d4a955",
                    fontSize: 11,
                    fontWeight: 600,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                  }}>{it.n}</span>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontWeight: 500, marginBottom: 2 }}>{it.title}</div>
                    <div className="field-help" style={{ marginBottom: 0 }}>{it.hint}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </article>

        {/* Sektion 3 — Skaffa API-nycklar */}
        <article className="settings-section" style={{ gridTemplateColumns: "180px minmax(0, 1fr)" }}>
          <div className="settings-section-label">
            <h2>Skaffa API-nycklar</h2>
            <p>Fyra providers — Ollama kräver ingen nyckel alls.</p>
          </div>
          <div className="settings-section-body">
            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>

              {/* Anthropic */}
              <div style={{ padding: "14px 16px", background: "rgba(243,237,227,0.02)", border: "1px solid rgba(243,237,227,0.06)", borderRadius: 10 }}>
                <div style={{ fontWeight: 500, marginBottom: 8 }}>Anthropic (Claude)</div>
                <ol style={{ margin: 0, paddingLeft: 18, display: "flex", flexDirection: "column", gap: 4 }}>
                  <li className="field-help" style={{ marginBottom: 0 }}>
                    Gå till{" "}
                    <a href="https://console.anthropic.com" target="_blank" rel="noreferrer" style={{ color: "var(--color-amber, #d4a955)" }}>
                      console.anthropic.com
                    </a>{" "}
                    → Skapa konto / logga in
                  </li>
                  <li className="field-help" style={{ marginBottom: 0 }}>Settings → API Keys → "Create Key"</li>
                  <li className="field-help" style={{ marginBottom: 0 }}>
                    Kopiera nyckeln (sk-ant-…) → Settings → Action-popup → Anthropic API-nyckel
                  </li>
                </ol>
                <div className="field-help" style={{ marginTop: 8, marginBottom: 0 }}>
                  Kostnad: ~$0.003/1K input-tokens för Sonnet 4.5. Bäst för: agentic tool-use, tung resonemang.
                </div>
              </div>

              {/* Gemini */}
              <div style={{ padding: "14px 16px", background: "rgba(243,237,227,0.02)", border: "1px solid rgba(243,237,227,0.06)", borderRadius: 10 }}>
                <div style={{ fontWeight: 500, marginBottom: 8 }}>Google Gemini</div>
                <ol style={{ margin: 0, paddingLeft: 18, display: "flex", flexDirection: "column", gap: 4 }}>
                  <li className="field-help" style={{ marginBottom: 0 }}>
                    Gå till{" "}
                    <a href="https://aistudio.google.com/apikey" target="_blank" rel="noreferrer" style={{ color: "var(--color-amber, #d4a955)" }}>
                      aistudio.google.com/apikey
                    </a>{" "}
                    → logga in med Google-konto
                  </li>
                  <li className="field-help" style={{ marginBottom: 0 }}>"Create API key" → välj projekt eller skapa nytt</li>
                  <li className="field-help" style={{ marginBottom: 0 }}>
                    Kopiera nyckeln (AI…) → Settings → Action-popup → Gemini API-nyckel
                  </li>
                </ol>
                <div className="field-help" style={{ marginTop: 8, marginBottom: 0 }}>
                  Gratis-tier: 10 RPM, 250 RPD för 2.5 Flash. Bäst för: realtidsdata via Google Search-grounding, kalender/mail via function-calling.
                </div>
              </div>

              {/* Groq */}
              <div style={{ padding: "14px 16px", background: "rgba(243,237,227,0.02)", border: "1px solid rgba(243,237,227,0.06)", borderRadius: 10 }}>
                <div style={{ fontWeight: 500, marginBottom: 8 }}>Groq (gratis-tier)</div>
                <ol style={{ margin: 0, paddingLeft: 18, display: "flex", flexDirection: "column", gap: 4 }}>
                  <li className="field-help" style={{ marginBottom: 0 }}>
                    Gå till{" "}
                    <a href="https://console.groq.com/keys" target="_blank" rel="noreferrer" style={{ color: "var(--color-amber, #d4a955)" }}>
                      console.groq.com/keys
                    </a>{" "}
                    → Skapa konto (gratis)
                  </li>
                  <li className="field-help" style={{ marginBottom: 0 }}>"Create API Key" → kopiera nyckeln (gsk_…)</li>
                  <li className="field-help" style={{ marginBottom: 0 }}>
                    Klistra in → Settings → Action-popup → Groq API-nyckel
                  </li>
                </ol>
                <div className="field-help" style={{ marginTop: 8, marginBottom: 0 }}>
                  Gratis-tier: 30 RPM för de flesta modeller. Bäst för: snabb + billig grammatikpolering av diktering.
                </div>
              </div>

              {/* Ollama */}
              <div style={{ padding: "14px 16px", background: "rgba(243,237,227,0.02)", border: "1px solid rgba(243,237,227,0.06)", borderRadius: 10 }}>
                <div style={{ fontWeight: 500, marginBottom: 8 }}>Ollama (lokalt, ingen nyckel)</div>
                <ol style={{ margin: 0, paddingLeft: 18, display: "flex", flexDirection: "column", gap: 4 }}>
                  <li className="field-help" style={{ marginBottom: 0 }}>
                    Ladda ner{" "}
                    <a href="https://ollama.com" target="_blank" rel="noreferrer" style={{ color: "var(--color-amber, #d4a955)" }}>
                      ollama.com
                    </a>{" "}
                    och installera
                  </li>
                  <li className="field-help" style={{ marginBottom: 0 }}>Starta Ollama-appen (kör i bakgrunden som server på port 11434)</li>
                  <li className="field-help" style={{ marginBottom: 0 }}>
                    Settings → Action-popup → Ollama-modell → välj modell → klicka "Ladda ner"
                  </li>
                </ol>
                <div className="field-help" style={{ marginTop: 8, marginBottom: 0 }}>
                  Ingen API-nyckel. Allt lokalt. Bäst för: privat/offline, ingen molnkoppling.
                </div>
              </div>

            </div>
          </div>
        </article>

        {/* Sektion 4 — Google OAuth */}
        <article className="settings-section" style={{ gridTemplateColumns: "180px minmax(0, 1fr)" }}>
          <div className="settings-section-label">
            <h2>Google OAuth (kalender + mail)</h2>
            <p>Kräver ett eget Google Cloud-projekt. Tar ~10 minuter.</p>
          </div>
          <div className="settings-section-body">
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {[
                { n: "1", text: <>Gå till{" "}<a href="https://console.cloud.google.com" target="_blank" rel="noreferrer" style={{ color: "var(--color-amber, #d4a955)" }}>console.cloud.google.com</a></> },
                { n: "2", text: "Skapa ett nytt projekt (eller använd befintligt)" },
                { n: "3", text: 'APIs & Services → Library → aktivera "Google Calendar API" och "Gmail API"' },
                { n: "4", text: 'APIs & Services → OAuth consent screen → välj "External" (om personligt konto), fyll i obligatoriska fält. Lägg till din e-post som test-user.' },
                { n: "5", text: 'APIs & Services → Credentials → "Create Credentials" → OAuth client ID → Application type: Desktop app' },
                { n: "6", text: "Kopiera Client ID och Client secret" },
                { n: "7", text: "Settings → Integrationer → klistra in båda fälten → Spara" },
                { n: "8", text: 'Klicka "Anslut Google-konto" → godkänn i browsern' },
              ].map((it) => (
                <div key={it.n} style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
                  <span style={{
                    flexShrink: 0,
                    width: 22,
                    height: 22,
                    borderRadius: 11,
                    background: "rgba(212,169,85,0.14)",
                    border: "1px solid rgba(212,169,85,0.3)",
                    color: "#d4a955",
                    fontSize: 11,
                    fontWeight: 600,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                  }}>{it.n}</span>
                  <div className="field-help" style={{ marginBottom: 0, flex: 1, minWidth: 0, paddingTop: 3 }}>{it.text}</div>
                </div>
              ))}
            </div>
            <div style={{ marginTop: 14, padding: "12px 14px", background: "rgba(212,169,85,0.06)", border: "1px solid rgba(212,169,85,0.18)", borderRadius: 10 }}>
              <div style={{ fontWeight: 500, marginBottom: 6 }}>Efter anslutning kan du säga:</div>
              <div className="field-help" style={{ marginBottom: 0, lineHeight: 1.8 }}>
                "Vad har jag i kalendern imorgon?" &nbsp;·&nbsp; "Boka möte fredag 14" &nbsp;·&nbsp;
                "Sök mail från X" &nbsp;·&nbsp; "Skicka mail till Y om Z" (skapar utkast, skickas inte)
              </div>
            </div>
          </div>
        </article>

        {/* Sektion 5 — Röstkommandon exempel */}
        <article className="settings-section" style={{ gridTemplateColumns: "180px minmax(0, 1fr)" }}>
          <div className="settings-section-label">
            <h2>Röstkommandon — exempel</h2>
            <p>Vanliga fraser att prova direkt.</p>
          </div>
          <div className="settings-section-body">
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>

              <div style={{ padding: "14px 16px", background: "rgba(243,237,227,0.02)", border: "1px solid rgba(243,237,227,0.06)", borderRadius: 10 }}>
                <div style={{ fontWeight: 500, marginBottom: 8 }}>Diktering (Höger Ctrl)</div>
                <div className="field-help" style={{ marginBottom: 0, lineHeight: 1.8 }}>
                  Håll ner → prata → släpp → text injiceras där markören står
                </div>
              </div>

              <div style={{ padding: "14px 16px", background: "rgba(243,237,227,0.02)", border: "1px solid rgba(243,237,227,0.06)", borderRadius: 10 }}>
                <div style={{ fontWeight: 500, marginBottom: 8 }}>Action-popup (Insert) — utan markering</div>
                <div className="field-help" style={{ marginBottom: 0, lineHeight: 1.8 }}>
                  "Vad är huvudstaden i Belgien?"<br />
                  "Vad är vädret i Stockholm just nu?"<br />
                  "Boka möte imorgon 14" <span style={{ opacity: 0.6 }}>(Google ansluten)</span><br />
                  "Sök mail från Anna" <span style={{ opacity: 0.6 }}>(Google ansluten)</span>
                </div>
              </div>

              <div style={{ gridColumn: "1 / -1", padding: "14px 16px", background: "rgba(243,237,227,0.02)", border: "1px solid rgba(243,237,227,0.06)", borderRadius: 10 }}>
                <div style={{ fontWeight: 500, marginBottom: 8 }}>Action-popup (Insert) — med markerad text (transformation)</div>
                <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "4px 24px" }}>
                  {["Gör detta mer formellt", "Översätt till engelska", "Kortare", "Rätta grammatiken"].map((ex) => (
                    <div key={ex} className="field-help" style={{ marginBottom: 0 }}>{ex}</div>
                  ))}
                </div>
              </div>

            </div>
          </div>
        </article>

        {/* Sektion 6 — Vanliga frågor */}
        <article className="settings-section" style={{ gridTemplateColumns: "180px minmax(0, 1fr)" }}>
          <div className="settings-section-label">
            <h2>Vanliga frågor</h2>
          </div>
          <div className="settings-section-body">
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              {[
                {
                  q: "Min modell är inte nedladdad — vad händer?",
                  a: "Första appstart laddar ner KB-Whisper Base automatiskt (~150 MB). Annars: Diktering-fliken → Transkribering → Ladda ner.",
                },
                {
                  q: 'Gemini säger 429 "quota exceeded"',
                  a: "Byt till stabila gemini-2.5-flash eller vänta tills midnatt PT (~09:00 svensk tid). Gratis-tier för 3 Preview är snävare.",
                },
                {
                  q: "Kan jag ändra Insert-tangenten?",
                  a: "Ja, Snabbkommandon-fliken.",
                },
                {
                  q: "Klipps långa inspelningar?",
                  a: "Ringbuffer är 120 sek (2 min). Om du behöver längre, öppna issue eller ändra i src-tauri/src/lib.rs rad 322.",
                },
                {
                  q: "Skickas min data nånstans?",
                  a: "Lokal STT (KB-Whisper) sker på din dator. API-providers (Anthropic/Google/Groq) ser bara det du säger i popupen. Audio finns bara i RAM, aldrig disk.",
                },
              ].map((it) => (
                <details
                  key={it.q}
                  style={{
                    padding: "10px 14px",
                    background: "rgba(243,237,227,0.02)",
                    border: "1px solid rgba(243,237,227,0.06)",
                    borderRadius: 10,
                  }}
                >
                  <summary style={{ fontWeight: 500, cursor: "pointer", userSelect: "none", listStyle: "none" }}>
                    {it.q}
                  </summary>
                  <div className="field-help" style={{ marginTop: 8, marginBottom: 0 }}>{it.a}</div>
                </details>
              ))}
            </div>
          </div>
        </article>

        {/* Sektion 7 — Hotkey-snabböversikt */}
        <article className="settings-section" style={{ gridTemplateColumns: "180px minmax(0, 1fr)" }}>
          <div className="settings-section-label">
            <h2>Tangentöversikt</h2>
          </div>
          <div className="settings-section-body">
            <table style={{ width: "100%", borderCollapse: "collapse" }}>
              <thead>
                <tr>
                  <th style={{ textAlign: "left", paddingBottom: 8, paddingRight: 24, fontWeight: 500, fontSize: 12, color: "var(--ink-tertiary)", borderBottom: "1px solid rgba(243,237,227,0.08)" }}>Tangent</th>
                  <th style={{ textAlign: "left", paddingBottom: 8, fontWeight: 500, fontSize: 12, color: "var(--ink-tertiary)", borderBottom: "1px solid rgba(243,237,227,0.08)" }}>Funktion</th>
                </tr>
              </thead>
              <tbody>
                {[
                  { key: "Höger Ctrl (håll)", fn: "Diktering" },
                  { key: "Insert (håll)", fn: "Action-popup" },
                  { key: "Ctrl+Shift+Space", fn: "Smart-function palette" },
                  { key: "Esc (i popup)", fn: "Stäng popup" },
                  { key: "Enter (i popup)", fn: "Applicera svar (klistra in)" },
                ].map((row) => (
                  <tr key={row.key}>
                    <td style={{ padding: "8px 24px 8px 0", fontFamily: "var(--font-mono)", fontSize: 12, color: "#d4a955", verticalAlign: "top" }}>{row.key}</td>
                    <td className="field-help" style={{ padding: "8px 0", marginBottom: 0, verticalAlign: "top" }}>{row.fn}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </article>

        </>)}

        {/* Footer — fade in endast vid osparade ändringar */}
        <footer className={`settings-footer${dirty ? " visible" : ""}`}>
          {error && (
            <div
              style={{
                color: "var(--danger)",
                fontFamily: "var(--font-mono)",
                fontSize: 11,
                marginRight: "auto",
              }}
            >
              {error}
            </div>
          )}
          <button
            type="button"
            className="btn btn-ghost"
            onClick={handleReset}
            disabled={!dirty || saving}
          >
            Ångra
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={handleSave}
            disabled={!dirty || saving}
          >
            {saving ? "Sparar…" : "Spara"}
          </button>
        </footer>
      </section>

      {promptModalOpen && (
        <div
          className="modal-backdrop"
          onClick={() => setPromptModalOpen(false)}
          role="presentation"
        >
          <div
            className="modal"
            role="dialog"
            aria-labelledby="prompt-modal-title"
            aria-modal="true"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 id="prompt-modal-title" className="modal-title">
              Redigera initial prompt
            </h3>
            <p className="modal-help">
              Texten matas in som historisk kontext till Whisper innan din
              diktering. Använd den för att priming:a modellen med domänord,
              stilpreferenser eller formattering. Lämna tom för ingen priming.
            </p>
            <textarea
              className="modal-textarea"
              value={promptModalDraft}
              onChange={(e) => setPromptModalDraft(e.target.value)}
              rows={6}
              autoFocus
              placeholder="T.ex. 'Medicinsk journalanteckning. Terminologi: anamnes, status, bedömning, åtgärd.'"
            />
            <div className="modal-footer">
              <button
                type="button"
                className="btn btn-ghost"
                onClick={() => setPromptModalOpen(false)}
              >
                Avbryt
              </button>
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => {
                  setDraft({ ...draft, stt_initial_prompt: promptModalDraft });
                  setPromptModalOpen(false);
                }}
              >
                Spara
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

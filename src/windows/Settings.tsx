import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import SVoiceLogo from "../components/SVoiceLogo";
import {
  checkHfCached,
  clearAnthropicKey,
  clearGroqKey,
  getSettings,
  googleConnect,
  googleConnectionStatus,
  googleDisconnect,
  hasAnthropicKey,
  hasGroqKey,
  listMicDevices,
  listOllamaModels,
  listSmartFunctions,
  openSmartFunctionsDir,
  pullOllamaModel,
  setAnthropicKey,
  setGroqKey,
  setSettings,
  type ComputeMode,
  type GoogleStatus,
  type HotKeyChoice,
  type LlmProviderChoice,
  type SttProviderChoice,
  type OllamaModelInfo,
  type PullProgress,
  type Settings,
  type SmartFunction,
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
  auto: "Auto (lokal → Groq → Claude)",
  ollama: "Lokal (Ollama)",
  claude: "Claude API",
  groq: "Groq API (gratis-tier)",
};

const STT_PROVIDER_LABELS: Record<SttProviderChoice, string> = {
  local: "Lokal (KB-Whisper via Python-sidecar)",
  groq: "Groq Whisper API (gratis, ~100× snabbare)",
};

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

type TabId = "overview" | "audio" | "llm" | "integrations" | "hotkeys";

const TABS: Array<{ id: TabId; label: string; icon: string }> = [
  { id: "overview", label: "Översikt", icon: "◆" },
  { id: "audio", label: "Ljud & STT", icon: "◉" },
  { id: "llm", label: "Action-LLM", icon: "❋" },
  { id: "integrations", label: "Integrationer", icon: "⊕" },
  { id: "hotkeys", label: "Snabbkommandon", icon: "⌘" },
];

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
}: {
  label: string;
  help: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="toggle-row">
      <div className="toggle-row-text">
        <div className="toggle-row-label">{label}</div>
        <div className="toggle-row-help">{help}</div>
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

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export default function SettingsView() {
  const [draft, setDraft] = useState<Settings | null>(null);
  const [loaded, setLoaded] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedTick, setSavedTick] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [micLevel, setMicLevel] = useState(0);
  const [micDevices, setMicDevices] = useState<string[]>([]);
  const [ollamaModels, setOllamaModels] = useState<OllamaModelInfo[]>([]);
  const [ollamaOnline, setOllamaOnline] = useState(false);
  const [pullState, setPullState] = useState<PullProgress | null>(null);
  const [sttCached, setSttCached] = useState<Record<string, boolean>>({});
  const [keyStored, setKeyStored] = useState(false);
  const [keyDraft, setKeyDraft] = useState<string | null>(null); // null=orört, ""=rensa, annars=ny nyckel
  const [groqKeyStored, setGroqKeyStored] = useState(false);
  const [groqKeyDraft, setGroqKeyDraft] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<TabId>("overview");
  const [googleStatus, setGoogleStatus] = useState<GoogleStatus>({
    connected: false,
    client_id_configured: false,
  });
  const [googleBusy, setGoogleBusy] = useState(false);
  const [smartFns, setSmartFns] = useState<SmartFunction[]>([]);

  // Refresh Ollama-modell-listan (t.ex. efter lyckad pull).
  async function refreshOllama() {
    try {
      const models = await listOllamaModels();
      setOllamaModels(models);
      setOllamaOnline(true);
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
    googleConnectionStatus()
      .then(setGoogleStatus)
      .catch(() =>
        setGoogleStatus({ connected: false, client_id_configured: false }),
      );
    listSmartFunctions().then(setSmartFns).catch(() => setSmartFns([]));
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
    return () => {
      unProgress.then((fn) => fn());
      unDone.then((fn) => fn());
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
      await setSettings(draft);
      setLoaded(draft);
      setSavedTick((t) => t + 1);
      // Re-fetcha Google-status efter save så "Anslut"-knappen enable:as
      // direkt om user precis fyllde i client-ID.
      googleConnectionStatus()
        .then(setGoogleStatus)
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
  }

  async function handleGoogleConnect() {
    setGoogleBusy(true);
    setError(null);
    try {
      await googleConnect();
      const status = await googleConnectionStatus();
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
    groqKeyDraft !== null;

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
            Lokal tal-till-text. Privat först. Håll höger Ctrl i valfri app för att diktera —
            resten är din text.
          </p>
        </div>

        <div className="settings-wordmark-footer">
          <div>
            <span className="dot" /> STT-modell laddad · 1670 ms
          </div>
          <div>GPU · kb-whisper-medium · float16</div>
          <div style={{ opacity: 0.6 }}>Ingen telemetri · ljud endast i RAM</div>
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
              label="LLM-polering av diktering"
              help="Skicka varje transkription genom LLM för grammatik/stavning innan inject. Långsammare (~300-700 ms extra) men vassare."
              value={draft.llm_polish_dictation}
              onChange={(v) => setDraft({ ...draft, llm_polish_dictation: v })}
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
                  {sttCached[draft.stt_model] === false && (
                    <div className="field-help" style={{ color: "var(--accent)" }}>
                      Inte cachad — första PTT efter spara laddar ner modellen (~
                      {draft.stt_model.includes("large")
                        ? "3 GB"
                        : draft.stt_model.includes("medium")
                          ? "1.5 GB"
                          : "150 MB"}
                      , tar 1-3 min).
                    </div>
                  )}
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
        {/* Action-LLM (iter 3) */}
        <article className="settings-section">
          <div className="settings-section-label">
            <h2>Action-LLM</h2>
            <p>
              Håll <strong>Insert</strong> och ge ett kommando för att öppna
              LLM-popup. Markerad text transformeras, tomt fält blir Q&amp;A.
            </p>
          </div>
          <div className="settings-section-body">
            <div className="field">
              <label className="field-label">Provider</label>
              <div className="segmented" role="tablist">
                {(Object.keys(PROVIDER_LABELS) as LlmProviderChoice[]).map((p) => (
                  <button
                    key={p}
                    type="button"
                    role="tab"
                    aria-selected={draft.llm_provider === p}
                    className={draft.llm_provider === p ? "active" : ""}
                    onClick={() => setDraft({ ...draft, llm_provider: p })}
                  >
                    {PROVIDER_LABELS[p]}
                  </button>
                ))}
              </div>
              <div className="field-help">
                Auto försöker lokal Ollama först, fallback till Claude API om
                den inte svarar på localhost:11434.
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
                    return <span className="field-badge muted">Ollama offline</span>;
                  }
                  if (pulling) return null;
                  if (installed) return <span className="field-badge ok">✓ installerad</span>;
                  return (
                    <button
                      type="button"
                      className="btn btn-primary btn-compact"
                      onClick={handlePullOllama}
                    >
                      Ladda ner
                    </button>
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
                    {ollamaModels.length} modeller installerade.{" "}
                    <strong>Qwen 2.5 14B</strong> ger bra balans för RTX 5080.
                  </>
                ) : (
                  <>
                    Ollama-service inte detekterad på{" "}
                    <code>{draft.ollama_url}</code>. Installera från{" "}
                    <code>ollama.com</code> och starta tjänsten.
                  </>
                )}
              </div>
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
                {googleStatus.connected
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
        {/* Röstdetektion — del av Ljud & STT-fliken */}
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
    </div>
  );
}

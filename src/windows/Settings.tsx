import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import SVoiceLogo from "../components/SVoiceLogo";
import {
  checkHfCached,
  getSettings,
  listMicDevices,
  listOllamaModels,
  pullOllamaModel,
  setSettings,
  type ComputeMode,
  type LlmProviderChoice,
  type OllamaModelInfo,
  type PullProgress,
  type Settings,
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
  auto: "Auto (lokal först)",
  ollama: "Lokal (Ollama)",
  claude: "Claude API",
};

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
      await setSettings(draft);
      setLoaded(draft);
      setSavedTick((t) => t + 1);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  function handleReset() {
    if (loaded) setDraft(loaded);
  }

  if (!draft) {
    return (
      <div className="settings-root">
        <div className="loading-shell">laddar konfiguration…</div>
      </div>
    );
  }

  const dirty = JSON.stringify(draft) !== JSON.stringify(loaded);

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
          </div>
        </article>

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
              KB-Whisper tränad på svensk tal. Större modell = bättre kvalitet men längre
              laddning och mer VRAM.
            </p>
          </div>
          <div className="settings-section-body">
            <div className="field">
              <label className="field-label" htmlFor="model">
                Modell
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
          </div>
        </article>

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
                placeholder="sk-ant-…"
                value={draft.anthropic_api_key ?? ""}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    anthropic_api_key:
                      e.target.value.trim() === "" ? null : e.target.value,
                  })
                }
                autoComplete="off"
                spellCheck={false}
              />
              <div className="field-help">
                Används när provider är Claude eller Auto (fallback).
                Sparas i klartext — keyring kommer i nästa iter.
              </div>
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
          </div>
        </article>

        {/* Röstdetektion */}
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

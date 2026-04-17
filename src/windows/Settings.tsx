import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import SVoiceLogo from "../components/SVoiceLogo";
import {
  getSettings,
  listMicDevices,
  setSettings,
  type ComputeMode,
  type LlmProviderChoice,
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

export default function SettingsView() {
  const [draft, setDraft] = useState<Settings | null>(null);
  const [loaded, setLoaded] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedTick, setSavedTick] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [micLevel, setMicLevel] = useState(0);
  const [micDevices, setMicDevices] = useState<string[]>([]);

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
  }, []);

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
                {MODELS.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label} — {m.note}
                  </option>
                ))}
              </select>
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
              <select
                id="ollama-model"
                className="select"
                value={draft.ollama_model}
                onChange={(e) =>
                  setDraft({ ...draft, ollama_model: e.target.value })
                }
              >
                {OLLAMA_MODELS.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label} — {m.note}
                  </option>
                ))}
              </select>
              <div className="field-help">
                Kräver <code>ollama pull {draft.ollama_model}</code> i terminal
                innan första användning. Default <strong>Qwen 2.5 14B</strong>{" "}
                ger bra balans mellan kvalitet och snabbhet på RTX 5080.
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

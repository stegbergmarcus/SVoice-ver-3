import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import SVoiceLogo from "../components/SVoiceLogo";
import "./ActionPopup.css";

type PopupOpenPayload = {
  selection: string | null;
  command: string;
  mode: "transform" | "query" | "follow_up";
};

type PopupMode = "transform" | "query" | "follow_up";

type ToolCallPayload = {
  name: string;
  status: "running" | "done" | "error";
  summary: string | null;
};

// Keys som triggar follow-up PTT när popup är fokuserad. Mellanslag är
// primär (har ingen default-handler i de flesta text-widgets). Insert
// mirrorar main-hotkey så user slipper lära sig ny genväg för follow-up.
function isFollowupKey(key: string): boolean {
  return key === " " || key === "Spacebar" || key === "Insert";
}

const TOOL_LABELS: Record<string, string> = {
  list_calendar_events: "Listar kalender",
  create_calendar_event: "Skapar möte",
  search_emails: "Söker mail",
  read_email: "Läser mail",
  draft_email: "Skriver mail-utkast",
  draft_reply: "Skriver svar-utkast",
  web_search: "Söker på nätet",
};

export default function ActionPopup() {
  const [visible, setVisible] = useState(false);
  const [selection, setSelection] = useState<string | null>(null);
  const [command, setCommand] = useState("");
  const [mode, setMode] = useState<PopupMode>("query");
  // Räknar antal follow-up-turns för att visa "uppföljning 2", "uppföljning 3" etc.
  const [turnCount, setTurnCount] = useState(1);
  const [response, setResponse] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
  const [toolCalls, setToolCalls] = useState<ToolCallPayload[]>([]);
  const rootRef = useRef<HTMLDivElement>(null);

  // Close popup-window and reset state via backend (pålitligare än frontend).
  async function closeWindow() {
    setVisible(false);
    try {
      await invoke("action_cancel");
    } catch (e) {
      console.error("[action-popup] cancel failed", e);
    }
  }

  // Skicka LLM-resultat till backend. Backend orkestrerar hela flödet:
  // hide popup → focus-restore till target → paste. Frontend sätter bara
  // state och invokar — alla Win32-calls görs backend-sidan.
  async function applyResult() {
    if (applying || !response.trim()) return;
    setApplying(true);
    const resultToApply = response;
    setVisible(false);
    try {
      await invoke("action_apply", { result: resultToApply });
    } catch (e) {
      console.error("[action-popup] apply failed", e);
    } finally {
      setApplying(false);
    }
  }

  // Lyssna på open/token/done/error-events från backend.
  useEffect(() => {
    const unOpen = listen<PopupOpenPayload>("action_popup_open", async (ev) => {
      const isFollowUp = ev.payload.mode === "follow_up";
      // Follow-up: bevara selection från original-konversationen (backend
      // skickar null). Öka turn-count. Rensa bara response.
      if (!isFollowUp) {
        setSelection(ev.payload.selection);
        setTurnCount(1);
      } else {
        setTurnCount((t) => t + 1);
      }
      setCommand(ev.payload.command);
      setMode(ev.payload.mode);
      setResponse("");
      setError(null);
      setStreaming(true);
      setApplying(false);
      setToolCalls([]);
      setVisible(true);
      // Visa fönstret OCH ta fokus så Enter/Escape fungerar.
      // Focus stjäls från target-appen, men target-HWND är redan sparat i
      // backend (remember_foreground_target vid keydown) så SetForegroundWindow
      // kan restore:a fokus vid paste.
      try {
        const win = getCurrentWebviewWindow();
        await win.show();
        await win.setFocus();
      } catch (e) {
        console.error("[action-popup] show/focus failed", e);
      }
    });

    const unToken = listen<{ text: string }>("action_llm_token", (ev) => {
      setResponse((prev) => prev + ev.payload.text);
    });

    const unDone = listen<void>("action_llm_done", () => {
      setStreaming(false);
    });

    const unError = listen<{ message: string }>("action_llm_error", (ev) => {
      setError(ev.payload.message);
      setStreaming(false);
    });

    const unTool = listen<ToolCallPayload>("action_tool_call", (ev) => {
      setToolCalls((prev) => {
        // Om status === 'running' för en ny tool → append.
        // Om status === 'done'/'error' och finns en matchande running → ersätt.
        if (ev.payload.status === "running") {
          return [...prev, ev.payload];
        }
        const idx = prev.findIndex(
          (t) => t.name === ev.payload.name && t.status === "running",
        );
        if (idx === -1) return [...prev, ev.payload];
        const next = [...prev];
        next[idx] = ev.payload;
        return next;
      });
    });

    return () => {
      unOpen.then((fn) => fn());
      unToken.then((fn) => fn());
      unDone.then((fn) => fn());
      unError.then((fn) => fn());
      unTool.then((fn) => fn());
    };
  }, []);

  // Global keybinds när popup är synlig.
  useEffect(() => {
    if (!visible || applying) return;
    const handler = async (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        ev.preventDefault();
        try {
          await invoke("action_cancel");
        } catch {}
        await closeWindow();
      } else if (ev.key === "Enter" && !ev.shiftKey && !streaming && !applying) {
        ev.preventDefault();
        await applyResult();
      } else if (isFollowupKey(ev.key) && !streaming && !ev.repeat) {
        // Mellanslag ELLER Insert = starta follow-up PTT. LL-hook fångar inte
        // Insert när popup-webviewen har fokus (WebView2/systemhookar filter:ar
        // den bort från systemhook-kedjan), så vi använder popup-keydown
        // istället. Backend action_followup_start → action_followup_stop (keyup)
        // översätter till samma LlKeyEvent som LL-hook hade skickat.
        ev.preventDefault();
        try {
          await invoke("action_followup_start");
        } catch (e) {
          console.error("[action-popup] followup_start failed", e);
        }
      }
    };
    const keyupHandler = async (ev: KeyboardEvent) => {
      if (isFollowupKey(ev.key) && !streaming) {
        ev.preventDefault();
        try {
          await invoke("action_followup_stop");
        } catch (e) {
          console.error("[action-popup] followup_stop failed", e);
        }
      }
    };
    window.addEventListener("keyup", keyupHandler);
    window.addEventListener("keydown", handler);
    return () => {
      window.removeEventListener("keydown", handler);
      window.removeEventListener("keyup", keyupHandler);
    };
  }, [visible, streaming, applying, response]);

  return (
    <div ref={rootRef} className={`action-popup-root${visible ? " visible" : ""}`}>
      <header className="action-popup-header">
        <div className="action-popup-logo" aria-hidden>
          <SVoiceLogo size={40} recording={streaming} />
        </div>
        <div className="action-popup-command">
          <div className="action-popup-command-eyebrow">
            {mode === "follow_up" ? `uppföljning ${turnCount}` : "du sa"}
          </div>
          <div className="action-popup-command-text">
            {command || "(inget kommando)"}
          </div>
        </div>
        <div className="action-popup-mode">
          {mode === "transform"
            ? "Transformera"
            : mode === "follow_up"
              ? "Uppföljning"
              : "Fråga"}
        </div>
      </header>

      {selection && (
        <div className="action-popup-context">
          <span className="action-popup-context-label">markerad text</span>
          {selection}
        </div>
      )}

      {toolCalls.length > 0 && (
        <div className="action-popup-tools">
          {toolCalls.map((t, i) => (
            <div
              key={`${t.name}-${i}`}
              className={`action-popup-tool action-popup-tool-${t.status}`}
            >
              <span className="action-popup-tool-dot" aria-hidden>
                {t.status === "running" ? "⏳" : t.status === "done" ? "✓" : "✕"}
              </span>
              <span className="action-popup-tool-name">
                {TOOL_LABELS[t.name] ?? t.name}
              </span>
              {t.summary && (
                <span className="action-popup-tool-summary"> · {t.summary}</span>
              )}
            </div>
          ))}
        </div>
      )}

      {error ? (
        <div className="action-popup-error">{error}</div>
      ) : (
        <div
          className={`action-popup-response${streaming ? " streaming" : ""}`}
        >
          {response}
        </div>
      )}

      <footer className="action-popup-footer">
        <div>
          {streaming
            ? "genererar…"
            : mode === "transform" && !error && response
              ? "Enter ersätter markerad text"
              : !error && response
                ? "Enter kopierar · håll Mellanslag eller Insert för följdfråga"
                : ""}
        </div>
        <div>
          <span className="kbd">Esc</span> stäng
          <span className="kbd primary">Enter</span>
          {mode === "transform" ? "ersätt" : "kopiera"}
        </div>
      </footer>
    </div>
  );
}

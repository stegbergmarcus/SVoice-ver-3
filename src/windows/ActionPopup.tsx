import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./ActionPopup.css";

type PopupOpenPayload = {
  selection: string | null;
  command: string;
  mode: "transform" | "query";
};

export default function ActionPopup() {
  const [visible, setVisible] = useState(false);
  const [selection, setSelection] = useState<string | null>(null);
  const [command, setCommand] = useState("");
  const [mode, setMode] = useState<"transform" | "query">("query");
  const [response, setResponse] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
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
      setSelection(ev.payload.selection);
      setCommand(ev.payload.command);
      setMode(ev.payload.mode);
      setResponse("");
      setError(null);
      setStreaming(true);
      setApplying(false);
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

    return () => {
      unOpen.then((fn) => fn());
      unToken.then((fn) => fn());
      unDone.then((fn) => fn());
      unError.then((fn) => fn());
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
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [visible, streaming, applying, response]);

  return (
    <div ref={rootRef} className={`action-popup-root${visible ? " visible" : ""}`}>
      <header className="action-popup-header">
        <div className="action-popup-logo" aria-hidden>
          SV
        </div>
        <div className="action-popup-command">
          <div className="action-popup-command-eyebrow">du sa</div>
          <div className="action-popup-command-text">
            {command || "(inget kommando)"}
          </div>
        </div>
        <div className="action-popup-mode">
          {mode === "transform" ? "Transformera" : "Fråga"}
        </div>
      </header>

      {selection && (
        <div className="action-popup-context">
          <span className="action-popup-context-label">markerad text</span>
          {selection}
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
          {mode === "transform" && !streaming && !error && response
            ? "Enter ersätter markerad text"
            : mode === "query"
              ? "Enter kopierar svaret"
              : streaming
                ? "genererar…"
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

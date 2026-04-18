import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  listSmartFunctions,
  type SmartFunction,
} from "../lib/settings-api";
import "./Palette.css";

export default function Palette() {
  const [visible, setVisible] = useState(false);
  const [query, setQuery] = useState("");
  const [fns, setFns] = useState<SmartFunction[]>([]);
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Filter + lägg top-match först.
  const filtered = useMemo(() => {
    if (!query.trim()) return fns;
    const q = query.toLowerCase();
    return fns.filter(
      (f) =>
        f.name.toLowerCase().includes(q) ||
        f.description.toLowerCase().includes(q) ||
        f.id.toLowerCase().includes(q),
    );
  }, [fns, query]);

  useEffect(() => {
    setSelected(0);
  }, [query, visible]);

  // Lyssna på palette_open-event (triggas från backend via hotkey).
  useEffect(() => {
    const un = listen("palette_open", async () => {
      setQuery("");
      setSelected(0);
      try {
        const list = await listSmartFunctions();
        setFns(list);
      } catch (e) {
        console.error("[palette] list failed", e);
        setFns([]);
      }
      setVisible(true);
      try {
        const win = getCurrentWebviewWindow();
        await win.show();
        await win.setFocus();
      } catch (e) {
        console.error("[palette] show/focus failed", e);
      }
      // Fokusera search-input efter render.
      setTimeout(() => inputRef.current?.focus(), 20);
    });
    return () => {
      un.then((fn) => fn());
    };
  }, []);

  async function close() {
    setVisible(false);
    try {
      const win = getCurrentWebviewWindow();
      await win.hide();
    } catch {}
  }

  async function run(fn: SmartFunction) {
    try {
      await invoke("run_smart_function", { id: fn.id });
    } catch (e) {
      console.error("[palette] run failed", e);
    }
    await close();
  }

  useEffect(() => {
    if (!visible) return;
    const handler = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") {
        ev.preventDefault();
        void close();
      } else if (ev.key === "ArrowDown") {
        ev.preventDefault();
        setSelected((s) => Math.min(filtered.length - 1, s + 1));
      } else if (ev.key === "ArrowUp") {
        ev.preventDefault();
        setSelected((s) => Math.max(0, s - 1));
      } else if (ev.key === "Enter" && filtered[selected]) {
        ev.preventDefault();
        void run(filtered[selected]);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [visible, filtered, selected]);

  return (
    <div className={`palette-root${visible ? " visible" : ""}`}>
      <div className="palette-header">
        <span className="palette-eyebrow">smart-functions</span>
        <span className="palette-kbd">Esc stäng</span>
      </div>
      <input
        ref={inputRef}
        className="palette-input"
        placeholder="Sök funktion…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        autoComplete="off"
        spellCheck={false}
      />
      <div className="palette-list">
        {filtered.length === 0 ? (
          <div className="palette-empty">
            {fns.length === 0
              ? "Inga smart-functions — öppna Settings för att seeda defaults."
              : "Inga träffar."}
          </div>
        ) : (
          filtered.map((fn, i) => (
            <button
              key={fn.id}
              type="button"
              className={`palette-item${i === selected ? " selected" : ""}`}
              onMouseEnter={() => setSelected(i)}
              onClick={() => void run(fn)}
            >
              <div className="palette-item-main">
                <span className="palette-item-name">{fn.name}</span>
                <span
                  className={`palette-item-mode palette-item-mode-${fn.mode}`}
                >
                  {fn.mode}
                </span>
              </div>
              <div className="palette-item-desc">{fn.description}</div>
            </button>
          ))
        )}
      </div>
      <div className="palette-footer">
        <span className="palette-kbd">↑↓</span> navigera
        <span className="palette-kbd">Enter</span> kör
      </div>
    </div>
  );
}

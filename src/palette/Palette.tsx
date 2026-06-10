import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  listSmartFunctions,
  type SmartFunction,
} from "../lib/settings-api";
import "./Palette.css";

/** En rad i palettlistan: antingen den pinnade ordboksposten (visas när
 *  text var markerad vid hotkey-press) eller en vanlig smart-function. */
type PaletteEntry =
  | { kind: "ordbok"; selection: string }
  | { kind: "fn"; fn: SmartFunction };

/** Trunkera markerad text för visning i palettposten. */
function previewText(s: string, max = 32): string {
  const flat = s.replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max)}…` : flat;
}

export default function Palette() {
  const [visible, setVisible] = useState(false);
  const [query, setQuery] = useState("");
  const [fns, setFns] = useState<SmartFunction[]>([]);
  const [selection, setSelection] = useState<string | null>(null);
  const [selected, setSelected] = useState(0);
  const [runError, setRunError] = useState<string | null>(null);
  // Ordboksläge: inline-formulär "från → till" istället för listan.
  const [ordbokMode, setOrdbokMode] = useState(false);
  const [ordbokFrom, setOrdbokFrom] = useState("");
  const [ordbokTo, setOrdbokTo] = useState("");
  const [ordbokSaved, setOrdbokSaved] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const ordbokToRef = useRef<HTMLInputElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);

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

  // Ordboksposten pinnas överst när text var markerad — oavsett sökfilter.
  const entries = useMemo<PaletteEntry[]>(() => {
    const list: PaletteEntry[] = filtered.map((fn) => ({ kind: "fn", fn }));
    if (selection) {
      list.unshift({ kind: "ordbok", selection });
    }
    return list;
  }, [filtered, selection]);

  useEffect(() => {
    setSelected(0);
  }, [query, visible]);

  // Scrolla det valda item:et in i view när pilarna flyttar urvalet.
  // `block: "nearest"` gör att listan bara scrollar precis så mycket som
  // behövs, inte hoppar till mitten av viewport varje pilklick.
  useEffect(() => {
    const el = itemRefs.current[selected];
    if (el) {
      el.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
  }, [selected]);

  // Lyssna på palette_open-event (triggas från backend via hotkey).
  useEffect(() => {
    const un = listen("palette_open", async () => {
      setQuery("");
      setSelected(0);
      setRunError(null);
      setOrdbokMode(false);
      setOrdbokSaved(false);
      try {
        const list = await listSmartFunctions();
        setFns(list);
      } catch (e) {
        console.error("[palette] list failed", e);
        setFns([]);
      }
      try {
        const sel = await invoke<string | null>("palette_selection_text");
        setSelection(sel);
      } catch (e) {
        console.error("[palette] selection failed", e);
        setSelection(null);
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
      // Backend-hide är pålitligare än webview.hide() från frontend
      // (senare lämnar ibland kvar en synlig svart rektangel).
      await invoke("palette_close");
    } catch {}
  }

  function enterOrdbokMode(sel: string) {
    setOrdbokFrom(sel);
    setOrdbokTo("");
    setRunError(null);
    setOrdbokMode(true);
    setTimeout(() => ordbokToRef.current?.focus(), 20);
  }

  async function saveOrdbok() {
    if (ordbokSaved) return;
    try {
      await invoke("add_stt_replacement", {
        from: ordbokFrom,
        to: ordbokTo,
      });
    } catch (e) {
      setRunError(String(e));
      return;
    }
    setOrdbokSaved(true);
    // Visa bekräftelsen kort, stäng sedan.
    setTimeout(() => void close(), 900);
  }

  async function runEntry(entry: PaletteEntry) {
    if (entry.kind === "ordbok") {
      enterOrdbokMode(entry.selection);
      return;
    }
    try {
      await invoke("run_smart_function", { id: entry.fn.id });
    } catch (e) {
      // Visa felet i paletten istället för tyst stängning — annars ser
      // user bara att inget hände.
      console.error("[palette] run failed", e);
      setRunError(`${entry.fn.name}: ${String(e)}`);
      return;
    }
    await close();
  }

  useEffect(() => {
    if (!visible) return;
    const handler = (ev: KeyboardEvent) => {
      if (ordbokMode) {
        if (ev.key === "Escape") {
          ev.preventDefault();
          setOrdbokMode(false);
          setTimeout(() => inputRef.current?.focus(), 20);
        } else if (ev.key === "Enter") {
          ev.preventDefault();
          void saveOrdbok();
        }
        return;
      }
      if (ev.key === "Escape") {
        ev.preventDefault();
        void close();
      } else if (ev.key === "ArrowDown") {
        ev.preventDefault();
        setSelected((s) => Math.min(entries.length - 1, s + 1));
      } else if (ev.key === "ArrowUp") {
        ev.preventDefault();
        setSelected((s) => Math.max(0, s - 1));
      } else if (ev.key === "Enter" && entries[selected]) {
        ev.preventDefault();
        void runEntry(entries[selected]);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [visible, entries, selected, ordbokMode, ordbokFrom, ordbokTo, ordbokSaved]);

  return (
    <div className={`palette-root${visible ? " visible" : ""}`}>
      <div className="palette-header">
        <span className="palette-eyebrow">
          {ordbokMode ? "lägg till i ordbok" : "smart-functions"}
        </span>
        <span className="palette-kbd">Esc {ordbokMode ? "tillbaka" : "stäng"}</span>
      </div>

      {ordbokMode ? (
        <div className="palette-ordbok">
          <div className="palette-ordbok-row">
            <input
              className="palette-input palette-ordbok-input"
              placeholder="felhört ord"
              value={ordbokFrom}
              onChange={(e) => setOrdbokFrom(e.target.value)}
              disabled={ordbokSaved}
              spellCheck={false}
            />
            <span className="palette-ordbok-arrow" aria-hidden>
              →
            </span>
            <input
              ref={ordbokToRef}
              className="palette-input palette-ordbok-input"
              placeholder="skriv istället…"
              value={ordbokTo}
              onChange={(e) => setOrdbokTo(e.target.value)}
              disabled={ordbokSaved}
              spellCheck={false}
            />
          </div>
          {ordbokSaved && (
            <div className="palette-success">
              ✓ Sparat — aktivt från nästa diktering
            </div>
          )}
        </div>
      ) : (
        <>
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
            {entries.length === 0 ? (
              <div className="palette-empty">
                {fns.length === 0
                  ? "Inga smart-functions — öppna Settings för att seeda defaults."
                  : "Inga träffar."}
              </div>
            ) : (
              entries.map((entry, i) => (
                <button
                  key={entry.kind === "ordbok" ? "__ordbok" : entry.fn.id}
                  ref={(el) => {
                    itemRefs.current[i] = el;
                  }}
                  type="button"
                  className={`palette-item${i === selected ? " selected" : ""}`}
                  onMouseEnter={() => setSelected(i)}
                  onClick={() => void runEntry(entry)}
                >
                  {entry.kind === "ordbok" ? (
                    <>
                      <div className="palette-item-main">
                        <span className="palette-item-name">
                          Lägg till i ordbok: "{previewText(entry.selection)}"
                        </span>
                        <span className="palette-item-mode palette-item-mode-ordbok">
                          ordbok
                        </span>
                      </div>
                      <div className="palette-item-desc">
                        Spara en korrigering så dikteringen skriver rätt nästa gång.
                      </div>
                    </>
                  ) : (
                    <>
                      <div className="palette-item-main">
                        <span className="palette-item-name">{entry.fn.name}</span>
                        <span
                          className={`palette-item-mode palette-item-mode-${entry.fn.mode}`}
                        >
                          {entry.fn.mode}
                        </span>
                      </div>
                      <div className="palette-item-desc">{entry.fn.description}</div>
                    </>
                  )}
                </button>
              ))
            )}
          </div>
        </>
      )}

      {runError && <div className="palette-error">⚠ {runError}</div>}
      {!ordbokMode && !selection && (
        <div className="palette-hint">
          Tips: markera text innan du trycker snabbkommandot för att kunna
          lägga till den i ordboken.
        </div>
      )}
      <div className="palette-footer">
        {ordbokMode ? (
          <>
            <span className="palette-kbd">Enter</span> spara
            <span className="palette-kbd">Esc</span> tillbaka
          </>
        ) : (
          <>
            <span className="palette-kbd">↑↓</span> navigera
            <span className="palette-kbd">Enter</span> kör
          </>
        )}
      </div>
    </div>
  );
}

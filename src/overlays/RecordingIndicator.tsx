import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

type PttState = "idle" | "recording" | "processing";

export default function RecordingIndicator() {
  const [state, setState] = useState<PttState>("idle");
  const [volume, setVolume] = useState<number>(0);
  const decayRef = useRef<number | null>(null);

  useEffect(() => {
    const unlistenState = listen<PttState>("ptt://state", (ev) => {
      setState(ev.payload);
      if (ev.payload !== "recording") setVolume(0);
    });
    const unlistenVolume = listen<number>("ptt://volume", (ev) => {
      // Mjuk decay: om ny volym är lägre än senaste, faller den långsamt.
      setVolume((prev) => Math.max(ev.payload, prev * 0.85));
    });
    return () => {
      unlistenState.then((fn) => fn());
      unlistenVolume.then((fn) => fn());
      if (decayRef.current !== null) cancelAnimationFrame(decayRef.current);
    };
  }, []);

  // Kontinuerlig decay så bar:en faller tillbaka när det blir tyst.
  useEffect(() => {
    if (state !== "recording") return;
    const tick = () => {
      setVolume((prev) => prev * 0.9);
      decayRef.current = requestAnimationFrame(tick);
    };
    decayRef.current = requestAnimationFrame(tick);
    return () => {
      if (decayRef.current !== null) cancelAnimationFrame(decayRef.current);
    };
  }, [state]);

  const dotColor =
    state === "recording" ? "#dc2626" : state === "processing" ? "#f59e0b" : "#6b7280";
  const label =
    state === "recording" ? "Spelar in…" : state === "processing" ? "Transkriberar…" : "Redo";

  // Skala volym så normalt tal (RMS ~0.05-0.15) fyller bar:en visuellt.
  const volumePct = Math.min(100, Math.round(volume * 400));

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 6,
        padding: "10px 14px",
        borderRadius: 14,
        background: "rgba(17, 24, 39, 0.92)",
        color: "white",
        fontSize: 13,
        boxShadow: "0 4px 12px rgba(0,0,0,0.35)",
        userSelect: "none",
        fontFamily: "system-ui, -apple-system, sans-serif",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span
          style={{
            width: 12,
            height: 12,
            borderRadius: 999,
            background: dotColor,
            boxShadow:
              state === "recording" ? `0 0 ${4 + volumePct / 10}px rgba(220,38,38,0.7)` : "none",
            transition: "box-shadow 40ms linear",
          }}
        />
        <span>{label}</span>
      </div>
      {state === "recording" && (
        <div
          style={{
            height: 4,
            background: "rgba(255,255,255,0.12)",
            borderRadius: 2,
            overflow: "hidden",
          }}
        >
          <div
            style={{
              height: "100%",
              width: `${volumePct}%`,
              background: "linear-gradient(90deg, #10b981, #f59e0b, #dc2626)",
              transition: "width 40ms linear",
            }}
          />
        </div>
      )}
    </div>
  );
}

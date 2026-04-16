import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

type PttState = "idle" | "recording" | "processing";

export default function RecordingIndicator() {
  const [state, setState] = useState<PttState>("idle");
  const [volume, setVolume] = useState<number>(0);
  const [stateEventCount, setStateEventCount] = useState<number>(0);
  const [volumeEventCount, setVolumeEventCount] = useState<number>(0);
  const decayRef = useRef<number | null>(null);

  useEffect(() => {
    console.log("[overlay] RecordingIndicator mounted — attaching listeners");
    const unlistenState = listen<PttState>("ptt_state", (ev) => {
      console.log("[overlay] ptt_state:", ev.payload);
      setState(ev.payload);
      setStateEventCount((c) => c + 1);
      if (ev.payload !== "recording") setVolume(0);
    });
    const unlistenVolume = listen<number>("ptt_volume", (ev) => {
      setVolumeEventCount((c) => c + 1);
      setVolume((prev) => Math.max(ev.payload, prev * 0.85));
    });
    return () => {
      unlistenState.then((fn) => fn());
      unlistenVolume.then((fn) => fn());
      if (decayRef.current !== null) cancelAnimationFrame(decayRef.current);
    };
  }, []);

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
  const volumePct = Math.min(100, Math.round(volume * 400));

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 4,
        padding: "8px 12px",
        borderRadius: 12,
        background: "rgba(17, 24, 39, 0.92)",
        color: "white",
        fontSize: 12,
        boxShadow: "0 4px 12px rgba(0,0,0,0.35)",
        userSelect: "none",
        fontFamily: "system-ui, -apple-system, sans-serif",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span
          style={{
            width: 10,
            height: 10,
            borderRadius: 999,
            background: dotColor,
            boxShadow:
              state === "recording" ? `0 0 ${3 + volumePct / 12}px rgba(220,38,38,0.8)` : "none",
            transition: "box-shadow 40ms linear",
          }}
        />
        <span>{label}</span>
      </div>
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
      <div style={{ fontSize: 9, opacity: 0.55, fontFamily: "monospace" }}>
        evt s:{stateEventCount} v:{volumeEventCount} vol:{volume.toFixed(3)}
      </div>
    </div>
  );
}

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

type PttState = "idle" | "recording" | "processing";

export default function RecordingIndicator() {
  const [state, setState] = useState<PttState>("idle");

  useEffect(() => {
    const unlisten = listen<PttState>("ptt://state", (ev) => {
      setState(ev.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const color =
    state === "recording" ? "#dc2626" : state === "processing" ? "#f59e0b" : "#6b7280";
  const label =
    state === "recording" ? "Spelar in…" : state === "processing" ? "Transkriberar…" : "Redo";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "10px 14px",
        borderRadius: 999,
        background: "rgba(17, 24, 39, 0.92)",
        color: "white",
        fontSize: 13,
        boxShadow: "0 4px 12px rgba(0,0,0,0.35)",
        userSelect: "none",
        fontFamily: "system-ui, -apple-system, sans-serif",
      }}
    >
      <span
        style={{
          width: 12,
          height: 12,
          borderRadius: 999,
          background: color,
        }}
      />
      <span>{label}</span>
    </div>
  );
}

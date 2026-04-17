import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./RecordingIndicator.css";

type PttState = "idle" | "recording" | "processing";

const BAR_COUNT = 28;

/**
 * "Voice-oval" — SVoice's recording-indicator overlay.
 *
 * Vänster: SV-monogram (logotyp). Höger: live waveform som reagerar på
 * mic-volym via ptt_volume-eventet. Under STT-inferens byter waveform till
 * en indeterminate progress-bar.
 *
 * Eventflöde:
 *   ptt_state (idle|recording|processing) styr synlighet och meter-mode.
 *   ptt_volume {rms} driver bar-heights vid recording.
 */
export default function RecordingIndicator() {
  const [state, setState] = useState<PttState>("idle");
  const [bars, setBars] = useState<number[]>(() => Array(BAR_COUNT).fill(0));
  const barsRef = useRef<number[]>(Array(BAR_COUNT).fill(0));
  const rafRef = useRef<number | null>(null);

  // Lyssna på state + volume-events.
  useEffect(() => {
    const unlistenState = listen<PttState>("ptt_state", (ev) => {
      setState(ev.payload);
      if (ev.payload !== "recording") {
        barsRef.current = Array(BAR_COUNT).fill(0);
        setBars(barsRef.current);
      }
    });

    const unlistenVolume = listen<{ rms: number }>("ptt_volume", (ev) => {
      // Shifta alla bars en plats vänsterut, injicera nyaste volym längst till höger.
      // Ger klassisk "wandering waveform"-effekt där nya samples flödar in och gamla
      // fade:ar ut till vänster.
      const rms = ev.payload.rms;
      // Kompander RMS till [0, 1] — mic-inputs är typiskt 0-0.3, logaritmisk skala
      // ger snyggare respons än linjär.
      const amplitude = Math.min(1, Math.pow(rms * 3.2, 0.7));
      const shifted = [...barsRef.current.slice(1), amplitude];
      barsRef.current = shifted;
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenVolume.then((fn) => fn());
    };
  }, []);

  // Animation-loop: applicera friktion och skriv state. Separerar event-rate
  // från render-rate så vi får smooth 60 FPS även om volume-events är 30 Hz.
  useEffect(() => {
    if (state !== "recording") return;
    const tick = () => {
      // Subtle decay så bars inte stannar vid senaste värdet när mic blir tyst.
      barsRef.current = barsRef.current.map((v, i) => {
        // De äldre (vänster) bars decay:ar snabbare än de nya (höger).
        const ageFactor = 1 - i / BAR_COUNT;
        return v * (0.92 + ageFactor * 0.06);
      });
      setBars([...barsRef.current]);
      rafRef.current = requestAnimationFrame(tick);
    };
    rafRef.current = requestAnimationFrame(tick);
    return () => {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    };
  }, [state]);

  const containerClass = `voice-oval ${state !== "idle" ? "visible" : ""} ${state}`;

  return (
    <div className={containerClass}>
      <div className="voice-oval-logo" aria-hidden>
        SV
      </div>

      {state === "processing" ? (
        <div className="voice-oval-meter voice-oval-meter--progress">
          <div className="progress-label">transkriberar…</div>
          <div className="progress-bar" role="progressbar" aria-busy="true" />
        </div>
      ) : (
        <div className="voice-oval-meter">
          <div className="waveform" role="meter" aria-label="Mikrofonnivå">
            {bars.map((h, i) => (
              <div
                key={i}
                className="waveform-bar"
                style={{ height: `${Math.max(3, h * 36)}px` }}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

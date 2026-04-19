import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import SVoiceLogo from "../components/SVoiceLogo";
import "./RecordingIndicator.css";

type PttState = "idle" | "recording" | "processing";

const HISTORY_LEN = 14; // unika historik-värden per sida (speglas runt centrum)

/**
 * "Voice-oval" — SVoice's recording-indicator overlay.
 *
 * Vänster: SV-monogram (logotyp). Höger: live waveform som reagerar på
 * mic-volym via mic_level-eventet. Under STT-inferens byter waveform till
 * en indeterminate progress-bar.
 *
 * Eventflöde:
 *   ptt_state (idle|recording|processing) styr synlighet och meter-mode.
 *   mic_level {rms} driver bar-heights vid recording (~30 Hz från audio-callback).
 *
 * Rendering-strategi: all animation drivs av mic_level-eventen — decay och
 * shift sker per event i handlern. Tidigare iterationer använde
 * requestAnimationFrame för decay-ticket men det throttlas aggressivt av
 * Chromium på unfocused webviews (overlay har `focus: false`), vilket gjorde
 * att animationen kunde frysa trots att events kom fram. Drivs nu helt av
 * backend-event-raten så det är robust oavsett fokus-läge.
 */
export default function RecordingIndicator() {
  const [state, setState] = useState<PttState>("idle");
  const [bars, setBars] = useState<number[]>(() => Array(HISTORY_LEN).fill(0));
  const barsRef = useRef<number[]>(Array(HISTORY_LEN).fill(0));

  useEffect(() => {
    const unlistenState = listen<PttState>("ptt_state", (ev) => {
      setState(ev.payload);
      if (ev.payload !== "recording") {
        const zeros = Array(HISTORY_LEN).fill(0);
        barsRef.current = zeros;
        setBars(zeros);
      }
    });

    const unlistenVolume = listen<{ rms: number }>("mic_level", (ev) => {
      const rms = ev.payload.rms;
      const amplitude = Math.min(1, Math.pow(rms * 3.2, 0.7));
      // Applicera per-bar decay innan shift. Äldre positioner (högre index)
      // decayar snabbare så gamla toppar fade:ar ut mot kanterna av
      // spegelmönstret. Combined med shift → waveform "andas ut" från centrum.
      const decayed = barsRef.current.map((v, i) => {
        const ageFactor = 1 - i / HISTORY_LEN;
        return v * (0.92 + ageFactor * 0.06);
      });
      const shifted = [amplitude, ...decayed.slice(0, HISTORY_LEN - 1)];
      barsRef.current = shifted;
      setBars(shifted);
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenVolume.then((fn) => fn());
    };
  }, []);

  const containerClass = `voice-oval ${state !== "idle" ? "visible" : ""} ${state}`;

  return (
    <div className={containerClass}>
      <div className="voice-oval-logo" aria-hidden>
        <SVoiceLogo size={34} recording={state === "recording"} />
      </div>

      {state === "processing" ? (
        <div className="voice-oval-meter voice-oval-meter--progress">
          <div className="progress-label">transkriberar…</div>
          <div className="progress-bar" role="progressbar" aria-busy="true" />
        </div>
      ) : (
        <div className="voice-oval-meter">
          <div className="waveform" role="meter" aria-label="Mikrofonnivå">
            {/* Symmetrisk rendering: vänster = historik i reverse, höger = historik.
                Nyaste värdet (index 0) hamnar på båda sidor om mittlinjen så
                waveform "andas ut" från centrum. */}
            {[...bars].reverse().map((h, i) => (
              <div
                key={`l-${i}`}
                className="waveform-bar"
                style={{ height: `${Math.max(3, h * 36)}px` }}
              />
            ))}
            {bars.map((h, i) => (
              <div
                key={`r-${i}`}
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

import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./RecordingIndicator.css";

type PttState = "idle" | "recording" | "processing";

const HISTORY_LEN = 14; // unika historik-värden per sida (speglas runt centrum)
const BAR_COUNT = HISTORY_LEN * 2; // 28 renderade bars totalt

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
  // barsRef: 14 unika historik-värden, index 0 = senaste. Renderas symmetriskt
  // runt centrum så waveform "andas ut" från mitten snarare än att flöda åt ett håll.
  const [bars, setBars] = useState<number[]>(() => Array(HISTORY_LEN).fill(0));
  const barsRef = useRef<number[]>(Array(HISTORY_LEN).fill(0));
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

    // Lyssnar på mic_level (alltid-på RMS från audio-capture) i stället för
    // ptt_volume (bara under dictation-VolumeMeter). Detta ger waveform-data
    // även för action-PTT (Insert). Overlay:en syns fortfarande bara när
    // state !== idle, så vi visar inte bars kontinuerligt.
    const unlistenVolume = listen<{ rms: number }>("mic_level", (ev) => {
      const rms = ev.payload.rms;
      const amplitude = Math.min(1, Math.pow(rms * 3.2, 0.7));
      // Nyaste värdet in vid index 0, äldre skiftas utåt (högre index).
      // Rendering speglar detta på båda sidor av mittlinjen.
      const shifted = [amplitude, ...barsRef.current.slice(0, HISTORY_LEN - 1)];
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
        // Äldre historik-positioner (högre index) decay:ar snabbare så
        // gamla toppar fade:ar ut mot kanterna av spegelmönstret.
        const ageFactor = 1 - i / HISTORY_LEN;
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

interface Props {
  size?: number;
  recording?: boolean;
  className?: string;
}

/**
 * SVoice-logotypen — geometriskt S byggt av tre horisontella bars.
 *
 * Komposition:
 *   - Cirkulär amber-gradient-base (editorial × studio-tema)
 *   - Topp-vänster gloss för 3D-känsla
 *   - Tre parallella bars: kort-höger, lång-mitten, kort-vänster — bildar
 *     ett abstrakt S-mönster samtidigt som det antyder audio-meter
 *   - I recording-läge pulserar den mellersta bar:en i amber
 */
export default function SVoiceLogo({
  size = 48,
  recording = false,
  className,
}: Props) {
  const uid = `svlogo-${useIdRef()}`;
  const mark = `url(#${uid}-mark)`;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 48 48"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      aria-hidden
    >
      <defs>
        <radialGradient id={`${uid}-bg`} cx="30%" cy="30%" r="80%">
          <stop offset="0%" stopColor="#ffb96a" />
          <stop offset="70%" stopColor="#e6a85c" />
          <stop offset="100%" stopColor="#c88a3f" />
        </radialGradient>
        <linearGradient id={`${uid}-mark`} x1="0%" y1="0%" x2="0%" y2="100%">
          <stop offset="0%" stopColor="#2d1608" stopOpacity="0.92" />
          <stop offset="100%" stopColor="#0b0b0d" />
        </linearGradient>
        <linearGradient id={`${uid}-gloss`} x1="0%" y1="0%" x2="60%" y2="100%">
          <stop offset="0%" stopColor="#ffffff" stopOpacity="0.4" />
          <stop offset="100%" stopColor="#ffffff" stopOpacity="0" />
        </linearGradient>
      </defs>

      {/* Cirkulär base */}
      <circle cx="24" cy="24" r="23.5" fill={`url(#${uid}-bg)`} />

      {/* Top-left gloss */}
      <path
        d="M 24 0 A 24 24 0 0 0 0 24 L 0 0 Z"
        fill={`url(#${uid}-gloss)`}
      />

      {/* Tre horisontella bars — bildar ett geometriskt S-mönster.
          Kort+förskjuten höger, lång mitten, kort+förskjuten vänster.
          Rundade ändar för mjukhet, men absolut raka linjer. */}
      <rect x="17" y="13" width="18" height="4" rx="2" fill={mark} />
      <rect
        x="10"
        y="22"
        width="28"
        height="4"
        rx="2"
        fill={mark}
        className={recording ? "svlogo-pulse-bar" : ""}
        style={{ transformOrigin: "24px 24px" }}
      />
      <rect x="13" y="31" width="18" height="4" rx="2" fill={mark} />

      {/* Subtle inner rand för djup */}
      <circle
        cx="24"
        cy="24"
        r="23"
        fill="none"
        stroke="rgba(0,0,0,0.25)"
        strokeWidth="0.5"
      />
    </svg>
  );
}

let counter = 0;
function useIdRef() {
  counter = (counter + 1) % 10000;
  return counter.toString(36);
}

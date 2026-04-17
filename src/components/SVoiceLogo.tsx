interface Props {
  size?: number;
  recording?: boolean;
  className?: string;
}

/**
 * SVoice-logotypen — stiliserat "S" som flödar som en ljudvåg.
 *
 * Komposition:
 *   - Cirkulär amber-gradient-base (matchar editorial × studio-tema)
 *   - Subtil glas-highlight topp-vänster för 3D-känsla
 *   - Flödande S-form i mörk gradient (Bezier-kurva, inte literal bokstav)
 *   - Två "voice-pulser" vid kurvans ändpunkter
 *   - I recording-läge pulserar dots med amber-glow
 *
 * Skalar rent i alla storlekar via viewBox. Aria-hidden för dekorativ
 * användning; wrappas i beskrivande element för screen-readers.
 */
export default function SVoiceLogo({
  size = 48,
  recording = false,
  className,
}: Props) {
  const uid = `svlogo-${useIdRef()}`;
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

      {/* Circular base */}
      <circle cx="24" cy="24" r="23.5" fill={`url(#${uid}-bg)`} />

      {/* Top-left gloss for 3D depth */}
      <path
        d="M 24 0 A 24 24 0 0 0 0 24 L 0 0 Z"
        fill={`url(#${uid}-gloss)`}
      />

      {/* Flowing S-wave — inte bokstav utan abstraktion av ljudrörelse.
          Bezier med två tighta svängar, rundade ändar för warmth. */}
      <path
        d="M 32 13
           C 26 10, 18 12, 16 18
           C 14 24, 24 24, 30 28
           C 34 30, 32 36, 24 36
           C 18 36, 15 33, 15 33"
        stroke={`url(#${uid}-mark)`}
        strokeWidth="3.6"
        strokeLinecap="round"
        fill="none"
      />

      {/* Voice-pulser vid ändpunkter */}
      <circle
        cx="32"
        cy="13"
        r="2.6"
        fill={`url(#${uid}-mark)`}
        className={recording ? "svlogo-pulse" : ""}
        style={{ transformOrigin: "32px 13px" }}
      />
      <circle
        cx="15"
        cy="33"
        r="2.6"
        fill={`url(#${uid}-mark)`}
        className={recording ? "svlogo-pulse" : ""}
        style={{ transformOrigin: "15px 33px", animationDelay: "0.4s" }}
      />

      {/* Subtle inner shadow för att ge djup */}
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

// Enkel uid-generator för att undvika gradient-id-kollision när
// samma logo renderas flera gånger på sidan.
let counter = 0;
function useIdRef() {
  // Unik per anrop, stable nog för SVG-defs scope
  counter = (counter + 1) % 10000;
  return counter.toString(36);
}

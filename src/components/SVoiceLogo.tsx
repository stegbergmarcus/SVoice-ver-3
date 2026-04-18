interface Props {
  size?: number;
  recording?: boolean;
  className?: string;
}

/**
 * SVoice-logotypen — "Echo".
 *
 * Komposition:
 *   - Rounded square amber-gradient-base (editorial × studio-tema) — rx 10/48 ≈ 21%
 *   - Topp-vänster gloss för 3D-känsla
 *   - Central amber-prick + två koncentriska mörka ringar = ekot av rösten
 *   - I recording-läge pulserar yttersta ringen
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
        <linearGradient id={`${uid}-bg`} x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="#ffb96a" />
          <stop offset="70%" stopColor="#e6a85c" />
          <stop offset="100%" stopColor="#c88a3f" />
        </linearGradient>
        <linearGradient id={`${uid}-gloss`} x1="0%" y1="0%" x2="60%" y2="100%">
          <stop offset="0%" stopColor="#ffffff" stopOpacity="0.4" />
          <stop offset="100%" stopColor="#ffffff" stopOpacity="0" />
        </linearGradient>
        <clipPath id={`${uid}-clip`}>
          <rect x="0" y="0" width="48" height="48" rx="10" ry="10" />
        </clipPath>
      </defs>

      {/* Rounded-square amber-base */}
      <rect
        x="0"
        y="0"
        width="48"
        height="48"
        rx="10"
        ry="10"
        fill={`url(#${uid}-bg)`}
      />

      {/* Top-left gloss, clipped till samma rounded square */}
      <rect
        x="0"
        y="0"
        width="48"
        height="48"
        rx="10"
        ry="10"
        fill={`url(#${uid}-gloss)`}
      />

      {/* Echo — center dot + två koncentriska ringar */}
      <g clipPath={`url(#${uid}-clip)`}>
        <circle cx="24" cy="24" r="3" fill="#0b0b0d" />
        <circle
          cx="24"
          cy="24"
          r="9"
          stroke="#0b0b0d"
          strokeWidth="2"
          fill="none"
          opacity="0.85"
        />
        <circle
          cx="24"
          cy="24"
          r="15"
          stroke="#0b0b0d"
          strokeWidth="1.6"
          fill="none"
          opacity="0.55"
          className={recording ? "svlogo-pulse-bar" : ""}
          style={{ transformOrigin: "24px 24px" }}
        />
      </g>

      {/* Subtle inner-stroke för djup */}
      <rect
        x="0.5"
        y="0.5"
        width="47"
        height="47"
        rx="9.5"
        ry="9.5"
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

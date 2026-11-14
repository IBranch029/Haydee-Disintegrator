/**
 * Vector icons of the console.
 *
 * The glyphs are drawn as smooth curves with round caps so that they blend
 * into the soft, rounded surface of the interface.
 */

interface IconProps {
  size?: number;
  className?: string;
}

/** Soft magnifier used by the target filter. */
export function SearchIcon({ size = 16, className }: IconProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
      focusable="false"
    >
      <circle
        cx="10.6"
        cy="10.6"
        r="6.4"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
      />
      <path
        d="M15.4 15.4 L20 20"
        stroke="currentColor"
        strokeWidth="1.9"
        strokeLinecap="round"
      />
    </svg>
  );
}

/** Small diamond used for the discovered easter egg marker. */
export function SparkIcon({ size = 12, className }: IconProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M12 2 L14.2 9.8 L22 12 L14.2 14.2 L12 22 L9.8 14.2 L2 12 L9.8 9.8 Z" />
    </svg>
  );
}

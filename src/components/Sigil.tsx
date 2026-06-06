export function Sigil({ className = "h-7 w-7" }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" fill="none" aria-hidden className={className}>
      <path d="M16 1.5 30.5 16 16 30.5 1.5 16 16 1.5Z" stroke="url(#g)" strokeWidth="1.25" opacity="0.9" />
      <path d="M16 6 26 16 16 26 6 16 16 6Z" stroke="url(#g)" strokeWidth="0.75" opacity="0.45" />
      <path d="M16 8.5v11" stroke="url(#g)" strokeWidth="1.4" strokeLinecap="round" />
      <path
        d="M12.5 12.5h7M16 19.5l-1.6 2.2h3.2L16 19.5Z"
        stroke="url(#g)"
        strokeWidth="1.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <defs>
        <linearGradient id="g" x1="6" y1="4" x2="26" y2="28">
          <stop stopColor="#f3e2b6" />
          <stop offset="0.55" stopColor="#c9a45c" />
          <stop offset="1" stopColor="#8a7440" />
        </linearGradient>
      </defs>
    </svg>
  );
}

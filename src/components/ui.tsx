import type { ReactNode, SVGProps } from "react";
import { hotkeyKeys } from "../lib/hotkey";

/* ---------- Icons (24px grid, 1.6 stroke, inherit currentColor) ---------- */

type IconProps = SVGProps<SVGSVGElement>;

function Svg({ children, ...props }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {children}
    </svg>
  );
}

/* Custom set: 24px grid, 1.7 round stroke, plus one translucent "body" fill per icon
   (currentColor at 22%) so every glyph shares the same two-tone construction. */
const T = { fill: "currentColor", fillOpacity: 0.22, stroke: "none" } as const;

export const MicIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="8.5" y="2.8" width="7" height="12" rx="3.5" {...T} />
    <rect x="8.5" y="2.8" width="7" height="12" rx="3.5" />
    <path d="M4.8 11.2c0 3.8 3 6.8 7.2 6.8s7.2-3 7.2-6.8M12 18v3M8.8 21h6.4" />
    <path d="M12 6.4v2.4" strokeWidth={1.4} />
  </Svg>
);
/** Words: a speech bubble holding two lines of text. */
export const BookIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M5 4h14a2 2 0 012 2v9a2 2 0 01-2 2h-6.5L8 20.8V17H5a2 2 0 01-2-2V6a2 2 0 012-2z" {...T} />
    <path d="M5 4h14a2 2 0 012 2v9a2 2 0 01-2 2h-6.5L8 20.8V17H5a2 2 0 01-2-2V6a2 2 0 012-2z" />
    <path d="M7.5 9h9M7.5 12.5h5" />
  </Svg>
);
/** Reader: a cone with sound bars instead of the stock "waves". */
export const SpeakerIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3.5 9.5h3L11 5.5v13l-4.5-4h-3a1 1 0 01-1-1v-3a1 1 0 011-1z" {...T} />
    <path d="M3.5 9.5h3L11 5.5v13l-4.5-4h-3a1 1 0 01-1-1v-3a1 1 0 011-1z" />
    <path d="M14.6 9.6v4.8M17.6 7.2v9.6M20.6 10v4" />
  </Svg>
);
/** Models: three stacked planes. */
export const ChipIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3.2l8.6 4.3L12 11.8 3.4 7.5 12 3.2z" {...T} />
    <path d="M12 3.2l8.6 4.3L12 11.8 3.4 7.5 12 3.2z" />
    <path d="M3.4 12l8.6 4.3 8.6-4.3M3.4 16.5L12 20.8l8.6-4.3" />
  </Svg>
);
/** Settings: two sliders. */
export const GearIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 8h16M4 16h16" />
    <circle cx="9" cy="8" r="2.7" {...T} />
    <circle cx="15.5" cy="16" r="2.7" {...T} />
    <circle cx="9" cy="8" r="2.7" />
    <circle cx="15.5" cy="16" r="2.7" />
  </Svg>
);
/** History: a clock with a back-arrow. */
export const HistoryIcon = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="13" cy="12" r="7.5" {...T} />
    <circle cx="13" cy="12" r="7.5" />
    <path d="M13 8v4.2l2.8 1.8" />
    <path d="M3.5 12H6M4.6 9.2L3.5 12l1.6 2.6" strokeWidth={1.4} />
  </Svg>
);
export const KeyboardIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="2.8" y="6" width="18.4" height="12" rx="3" {...T} />
    <rect x="2.8" y="6" width="18.4" height="12" rx="3" />
    <path d="M6.8 10h.01M10 10h.01M14 10h.01M17.2 10h.01M8 14.2h8" />
  </Svg>
);
export const StopIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...p}>
    <rect x="6.5" y="6.5" width="11" height="11" rx="2.5" />
  </svg>
);
export const PlayIcon = (p: IconProps) => (
  <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...p}>
    <path d="M8 5.5v13a1 1 0 001.5.86l10.5-6.5a1 1 0 000-1.72L9.5 4.64A1 1 0 008 5.5z" />
  </svg>
);
export const CheckIcon = (p: IconProps) => (
  <Svg strokeWidth={2.2} {...p}>
    <path d="M5 12.5l4.5 4.5L19 7.5" />
  </Svg>
);
export const AlertIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 4l9 16H3L12 4zM12 10v4.5M12 17.5v.01" />
  </Svg>
);
export const LockIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="5" y="10.5" width="14" height="10" rx="2.5" />
    <path d="M8 10.5V8a4 4 0 018 0v2.5" />
  </Svg>
);
export const AccessibilityIcon = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="5" r="1.7" />
    <path d="M5 8.5l7 1.5 7-1.5M12 10v4.5M9 20l3-5.5 3 5.5" />
  </Svg>
);
export const SpinnerIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M12 3a9 9 0 109 9" />
  </Svg>
);
export const CopyIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="8.5" y="8.5" width="11" height="11" rx="2.5" />
    <path d="M15.5 8.5V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7.5a2 2 0 002 2h2.5" />
  </Svg>
);
export const SearchIcon = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="11" cy="11" r="6.5" />
    <path d="M16 16l4 4" />
  </Svg>
);
export const CloseIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />
  </Svg>
);

/* ---------- Building blocks ---------- */

export function PageHeader({ title, lede, action }: { title: string; lede?: string; action?: ReactNode }) {
  return (
    <header className="mb-6 flex items-start justify-between gap-4">
      <div>
        <h1 className="page-title">{title}</h1>
        {lede && <p className="page-lede">{lede}</p>}
      </div>
      {action}
    </header>
  );
}

export function Section({
  title,
  note,
  children,
  className = "",
}: {
  title?: string;
  note?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={className}>
      {title && <h2 className="section-title">{title}</h2>}
      {note && <p className="section-note">{note}</p>}
      {children}
    </section>
  );
}

/** A label on the left, a control on the right. Use inside `.card.card-list`. */
export function SettingRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="row">
      <div className="min-w-0">
        <p className="text-[13px] text-fg">{label}</p>
        {hint && <p className="mt-0.5 text-xs text-muted">{hint}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-[22px] w-[38px] shrink-0 rounded-full transition-colors duration-150 ${
        checked ? "bg-brand" : "bg-subtle/40"
      }`}
    >
      <span
        className={`absolute left-0.5 top-0.5 h-[18px] w-[18px] rounded-full bg-white shadow transition-transform duration-150 ${
          checked ? "translate-x-4" : "translate-x-0"
        }`}
      />
    </button>
  );
}

/** Keycaps for a backend hotkey string. */
export function Keys({ hotkey, large = false }: { hotkey: string; large?: boolean }) {
  const keys = hotkeyKeys(hotkey);
  if (keys.length === 0) return <span className="text-xs text-subtle">Not set</span>;
  return (
    <span className="inline-flex items-center gap-1" aria-label={`Shortcut ${keys.join(" ")}`}>
      {keys.map((k, i) => (
        <kbd
          key={`${k}-${i}`}
          className={`kbd ${large ? "!min-w-[2.6rem] !rounded-lg !px-3 !py-1.5 !text-[20px]" : ""}`}
        >
          {k}
        </kbd>
      ))}
    </span>
  );
}

export function Spinner({ className = "h-5 w-5" }: { className?: string }) {
  return <SpinnerIcon className={`${className} animate-spin text-accent`} />;
}

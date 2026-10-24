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

export const MicIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="9" y="3" width="6" height="11" rx="3" />
    <path d="M5.5 11.5a6.5 6.5 0 0013 0M12 18v3" />
  </Svg>
);
export const BookIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M5 4.5A1.5 1.5 0 016.5 3H19v15H6.5A1.5 1.5 0 005 19.5v-15z" />
    <path d="M5 19.5A1.5 1.5 0 006.5 21H19v-3M9 7.5h6" />
  </Svg>
);
export const SpeakerIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5H4z" />
    <path d="M15.5 9a4 4 0 010 6M18 6.5a7.5 7.5 0 010 11" />
  </Svg>
);
export const ChipIcon = (p: IconProps) => (
  <Svg {...p}>
    <rect x="6" y="6" width="12" height="12" rx="2" />
    <rect x="9.5" y="9.5" width="5" height="5" rx="1" />
    <path d="M9 3v3M15 3v3M9 18v3M15 18v3M3 9h3M3 15h3M18 9h3M18 15h3" />
  </Svg>
);
export const GearIcon = (p: IconProps) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 3v2.2M12 18.8V21M4.6 7.5l1.9 1.1M17.5 15.4l1.9 1.1M4.6 16.5l1.9-1.1M17.5 8.6l1.9-1.1" />
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
        checked ? "bg-accent" : "bg-subtle/40"
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
          className={`kbd ${large ? "!min-w-[2.6rem] !rounded-lg !px-3 !py-1.5 !text-base" : ""}`}
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

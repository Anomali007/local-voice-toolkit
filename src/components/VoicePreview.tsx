interface Voice {
  id: string;
  name: string;
  language: string;
  gender: string;
}

interface VoicePreviewProps {
  voice: Voice;
  isSelected: boolean;
  onSelect: () => void;
}

export default function VoicePreview({ voice, isSelected, onSelect }: VoicePreviewProps) {
  return (
    <button onClick={onSelect} aria-pressed={isSelected} className="tile flex items-center gap-2.5 !p-2.5">
      <span
        aria-hidden="true"
        className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-accent/15 text-[13px] font-semibold text-accent"
      >
        {voice.name.slice(0, 1).toUpperCase()}
      </span>
      <span className="min-w-0">
        <span className="block truncate text-[13px] font-medium">{voice.name}</span>
        <span className="block truncate text-xs text-muted">
          {voice.language}
          {voice.gender ? ` · ${voice.gender}` : ""}
        </span>
      </span>
    </button>
  );
}

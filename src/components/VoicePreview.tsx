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
    <button
      onClick={onSelect}
      className={`p-3 rounded-lg text-left transition-all ${
        isSelected
          ? "bg-sky-500/20 border-2 border-sky-500"
          : "bg-slate-800 border-2 border-transparent hover:border-slate-600"
      }`}
    >
      <div className="flex items-center space-x-2">
        <span className="text-lg">{voice.gender === "Female" ? "👩" : "👨"}</span>
        <div>
          <p className="font-medium text-sm text-slate-100">{voice.name}</p>
          <p className="text-xs text-slate-400">{voice.language}</p>
        </div>
      </div>
    </button>
  );
}

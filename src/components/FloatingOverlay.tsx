import WaveformViz from "./WaveformViz";

interface FloatingOverlayProps {
  mode: "recording" | "speaking" | null;
  onStop: () => void;
}

export default function FloatingOverlay({ mode, onStop }: FloatingOverlayProps) {
  if (!mode) return null;

  return (
    <div className="fixed inset-0 flex items-center justify-center bg-black/50 backdrop-blur-sm z-50">
      <div className="bg-slate-800 rounded-2xl p-6 shadow-2xl min-w-[300px]">
        <div className="text-center mb-4">
          <div
            className={`w-16 h-16 mx-auto rounded-full flex items-center justify-center ${
              mode === "recording" ? "bg-red-500" : "bg-sky-500"
            }`}
          >
            {mode === "recording" ? (
              <MicIcon className="w-8 h-8 text-white" />
            ) : (
              <SpeakerIcon className="w-8 h-8 text-white" />
            )}
          </div>
          <p className="mt-3 text-slate-200 font-medium">
            {mode === "recording" ? "Recording..." : "Speaking..."}
          </p>
        </div>

        {mode === "recording" && <WaveformViz isActive={true} />}

        <button
          onClick={onStop}
          className="w-full mt-4 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded-lg transition-colors"
        >
          Stop
        </button>
      </div>
    </div>
  );
}

function MicIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
      />
    </svg>
  );
}

function SpeakerIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={2}
        d="M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z"
      />
    </svg>
  );
}

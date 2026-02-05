import { useState } from "react";
import { useSTT } from "../hooks/useSTT";
import WaveformViz from "./WaveformViz";

export default function DictationPanel() {
  const { isRecording, isTranscribing, transcript, error, startRecording, stopRecording } = useSTT();
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    if (transcript) {
      await navigator.clipboard.writeText(transcript);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  return (
    <div className="space-y-6">
      {/* Recording Button */}
      <div className="flex flex-col items-center space-y-4">
        <button
          onClick={isRecording ? stopRecording : startRecording}
          disabled={isTranscribing}
          className={`w-32 h-32 rounded-full flex items-center justify-center transition-all ${
            isRecording
              ? "bg-red-500 hover:bg-red-600 animate-pulse"
              : isTranscribing
              ? "bg-slate-600 cursor-not-allowed"
              : "bg-sky-500 hover:bg-sky-600"
          }`}
        >
          {isRecording ? (
            <StopIcon className="w-12 h-12 text-white" />
          ) : isTranscribing ? (
            <SpinnerIcon className="w-12 h-12 text-white animate-spin" />
          ) : (
            <MicIcon className="w-12 h-12 text-white" />
          )}
        </button>

        <p className="text-slate-400 text-sm">
          {isRecording
            ? "Recording... Click to stop"
            : isTranscribing
            ? "Transcribing..."
            : "Click to start recording"}
        </p>
      </div>

      {/* Waveform Visualization */}
      {isRecording && (
        <div className="bg-slate-800 rounded-lg p-4">
          <WaveformViz isActive={isRecording} />
        </div>
      )}

      {/* Transcript Output */}
      {transcript && (
        <div className="bg-slate-800 rounded-lg p-4">
          <div className="flex items-center justify-between mb-2">
            <h3 className="text-sm font-medium text-slate-300">Transcript</h3>
            <button
              onClick={handleCopy}
              className="text-xs text-sky-400 hover:text-sky-300 transition-colors"
            >
              {copied ? "Copied!" : "Copy"}
            </button>
          </div>
          <p className="text-slate-100 whitespace-pre-wrap">{transcript}</p>
        </div>
      )}

      {/* Error Display */}
      {error && (
        <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4">
          <p className="text-red-400 text-sm">{error}</p>
        </div>
      )}

      {/* Hotkey Hint */}
      <div className="text-center">
        <p className="text-xs text-slate-500">
          Tip: Press <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300">⌘</kbd> +{" "}
          <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300">⇧</kbd> +{" "}
          <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300">D</kbd> anywhere to dictate
        </p>
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

function StopIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="currentColor" viewBox="0 0 24 24">
      <rect x="6" y="6" width="12" height="12" rx="2" />
    </svg>
  );
}

function SpinnerIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24">
      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
      <path
        className="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
      />
    </svg>
  );
}

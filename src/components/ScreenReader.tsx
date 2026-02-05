import { useState } from "react";
import { useTTS } from "../hooks/useTTS";
import VoicePreview from "./VoicePreview";

export default function ScreenReader() {
  const { isSpeaking, voices, selectedVoice, speed, speak, stop, setSelectedVoice, setSpeed } = useTTS();
  const [text, setText] = useState("");

  const handleSpeak = () => {
    if (text.trim()) {
      speak(text);
    }
  };

  return (
    <div className="space-y-6">
      {/* Text Input */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-slate-300">Text to speak</label>
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Enter text to read aloud, or select text anywhere and press ⌘+⇧+S"
          className="w-full h-32 px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-sky-500 resize-none"
        />
      </div>

      {/* Controls */}
      <div className="flex items-center space-x-4">
        <button
          onClick={isSpeaking ? stop : handleSpeak}
          disabled={!text.trim() && !isSpeaking}
          className={`flex-1 px-4 py-3 rounded-lg font-medium transition-colors ${
            isSpeaking
              ? "bg-red-500 hover:bg-red-600 text-white"
              : text.trim()
              ? "bg-sky-500 hover:bg-sky-600 text-white"
              : "bg-slate-700 text-slate-400 cursor-not-allowed"
          }`}
        >
          {isSpeaking ? (
            <>
              <span className="mr-2">⏹</span> Stop
            </>
          ) : (
            <>
              <span className="mr-2">▶️</span> Speak
            </>
          )}
        </button>
      </div>

      {/* Voice Selection */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-slate-300">Voice</label>
        <div className="grid grid-cols-2 gap-2">
          {voices.map((voice) => (
            <VoicePreview
              key={voice.id}
              voice={voice}
              isSelected={selectedVoice === voice.id}
              onSelect={() => setSelectedVoice(voice.id)}
            />
          ))}
        </div>
      </div>

      {/* Speed Control */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label className="text-sm font-medium text-slate-300">Speed</label>
          <span className="text-sm text-slate-400">{speed.toFixed(1)}x</span>
        </div>
        <input
          type="range"
          min="0.5"
          max="2"
          step="0.1"
          value={speed}
          onChange={(e) => setSpeed(parseFloat(e.target.value))}
          className="w-full h-2 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-sky-500"
        />
        <div className="flex justify-between text-xs text-slate-500">
          <span>0.5x</span>
          <span>1.0x</span>
          <span>2.0x</span>
        </div>
      </div>

      {/* Hotkey Hint */}
      <div className="text-center">
        <p className="text-xs text-slate-500">
          Tip: Select text anywhere and press{" "}
          <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300">⌘</kbd> +{" "}
          <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300">⇧</kbd> +{" "}
          <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300">S</kbd> to read it aloud
        </p>
      </div>
    </div>
  );
}

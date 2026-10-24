import { useState } from "react";
import { formatHotkey, useTTS, type TtsEngine } from "../hooks/useTTS";
import VoicePreview from "./VoicePreview";

const ENGINES: { id: TtsEngine; name: string; detail: string }[] = [
  { id: "kokoro", name: "Kokoro", detail: "27 English voices" },
  { id: "phonon", name: "Phonon", detail: "1 voice · faster, clearer" },
];

export default function ScreenReader() {
  const {
    isSpeaking,
    voices,
    selectedVoice,
    engine,
    phononReady,
    hotkey,
    speed,
    error,
    speak,
    stop,
    setSelectedVoice,
    setEngine,
    setSpeed,
  } = useTTS();
  const [text, setText] = useState("");
  const keys = formatHotkey(hotkey);
  const needsDownload = engine === "phonon" && !phononReady;

  const handleSpeak = () => {
    if (text.trim()) {
      speak(text);
    }
  };

  return (
    <div className="space-y-6">
      {/* Text Input */}
      <div className="space-y-2">
        <label htmlFor="tts-text" className="block text-sm font-medium text-slate-300">
          Text to speak
        </label>
        <textarea
          id="tts-text"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) handleSpeak();
          }}
          placeholder={`Type or paste text here, or select text in any app and press ${keys}`}
          className="w-full h-32 px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-sky-500 resize-none"
        />
      </div>

      {/* Controls */}
      <div className="flex items-center space-x-4">
        <button
          onClick={isSpeaking ? stop : handleSpeak}
          disabled={(!text.trim() || needsDownload) && !isSpeaking}
          className={`flex-1 px-4 py-3 rounded-lg font-medium transition-colors ${
            isSpeaking
              ? "bg-red-500 hover:bg-red-600 text-white"
              : text.trim() && !needsDownload
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

      {error && (
        <p role="alert" className="px-3 py-2 text-sm text-red-300 bg-red-500/10 border border-red-500/30 rounded-lg">
          {error}
        </p>
      )}

      {/* Engine Selection */}
      <div className="space-y-2">
        <label className="block text-sm font-medium text-slate-300">Engine</label>
        <div role="radiogroup" aria-label="Speech engine" className="grid grid-cols-2 gap-2">
          {ENGINES.map((option) => (
            <button
              key={option.id}
              role="radio"
              aria-checked={engine === option.id}
              onClick={() => setEngine(option.id)}
              className={`p-3 rounded-lg text-left transition-all ${
                engine === option.id
                  ? "bg-sky-500/20 border-2 border-sky-500"
                  : "bg-slate-800 border-2 border-transparent hover:border-slate-600"
              }`}
            >
              <p className="font-medium text-sm text-slate-100">{option.name}</p>
              <p className="text-xs text-slate-400">{option.detail}</p>
            </button>
          ))}
        </div>
        {needsDownload && (
          <p className="text-xs text-amber-400">
            Phonon isn't downloaded yet. Get "Phonon 31m" (99 MB) in the Models tab.
          </p>
        )}
      </div>

      {/* Voice Selection */}
      {voices.length > 1 && (
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
      )}

      {/* Speed Control */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <label htmlFor="tts-speed" className="text-sm font-medium text-slate-300">
            Speed
          </label>
          <span className="text-sm text-slate-400">{speed.toFixed(1)}x</span>
        </div>
        <input
          id="tts-speed"
          type="range"
          min="0.5"
          max="3"
          step="0.1"
          value={speed}
          onChange={(e) => setSpeed(parseFloat(e.target.value))}
          className="w-full h-2 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-sky-500"
        />
        <div className="flex justify-between text-xs text-slate-500">
          <span>0.5x</span>
          <span>1.0x</span>
          <span>2.0x</span>
          <span>3.0x</span>
        </div>
        <p className="text-xs text-slate-500">The voice keeps its pitch at any speed.</p>
      </div>

      {/* Voice Cloning — Coming Soon */}
      <div className="p-4 bg-slate-800/50 border border-slate-700/50 rounded-lg">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-medium text-slate-300">Voice Cloning</h3>
            <p className="text-xs text-slate-500 mt-1">
              Clone any voice from a 5+ second audio sample. Create custom voice avatars for personalized TTS.
            </p>
          </div>
          <span className="px-2 py-1 text-xs font-medium bg-amber-500/20 text-amber-400 rounded-full whitespace-nowrap">
            Coming Soon
          </span>
        </div>
      </div>

      {/* Hotkey Hint */}
      <div className="text-center">
        <p className="text-xs text-slate-500">
          Tip: Select text anywhere and press{" "}
          <kbd className="px-1.5 py-0.5 bg-slate-700 rounded text-slate-300">{keys}</kbd> to read it
          aloud. Press it again to stop.
        </p>
      </div>
    </div>
  );
}

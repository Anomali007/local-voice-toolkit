import { useState } from "react";
import { formatHotkey, useTTS, type TtsEngine } from "../hooks/useTTS";
import VoicePreview from "./VoicePreview";
import { PageHeader, PlayIcon, Section, StopIcon } from "./ui";

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

  const canSpeak = text.trim() !== "" && !needsDownload;

  return (
    <div className="space-y-6">
      <PageHeader
        title="Read aloud"
        lede="Paste text to hear it, or select text in any app and use the shortcut."
      />

      {/* Text input */}
      <div className="card p-3">
        <label htmlFor="tts-text" className="sr-only">
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
          className="h-32 w-full resize-none bg-transparent p-1 text-[14px] leading-relaxed text-fg placeholder:text-subtle focus-visible:outline-none"
        />
        <div className="flex items-center justify-between pt-2">
          <p className="pl-1 text-xs text-subtle">
            Tip: select text anywhere and press <kbd className="kbd">{keys}</kbd> to read it. Press it again to stop.
          </p>
          <button
            onClick={isSpeaking ? stop : handleSpeak}
            disabled={!canSpeak && !isSpeaking}
            className={isSpeaking ? "btn-danger-solid" : "btn-primary"}
          >
            {isSpeaking ? (
              <>
                <StopIcon className="h-4 w-4" /> Stop
              </>
            ) : (
              <>
                <PlayIcon className="h-4 w-4" /> Speak
              </>
            )}
          </button>
        </div>
      </div>

      {error && (
        <p role="alert" className="alert-danger">
          {error}
        </p>
      )}

      {/* Engine */}
      <Section title="Engine">
        <div role="radiogroup" aria-label="Speech engine" className="grid grid-cols-2 gap-3">
          {ENGINES.map((option) => (
            <button
              key={option.id}
              role="radio"
              aria-checked={engine === option.id}
              onClick={() => setEngine(option.id)}
              className="tile"
            >
              <p className="text-[13px] font-semibold">{option.name}</p>
              <p className="text-xs text-muted">{option.detail}</p>
            </button>
          ))}
        </div>
        {needsDownload && (
          <p className="mt-2 px-1 text-xs text-warn">
            Phonon isn't downloaded yet. Get "Phonon 31m" (99 MB) in the Models tab.
          </p>
        )}
      </Section>

      {/* Voice */}
      {voices.length > 1 && (
        <Section title="Voice">
          <div className="grid grid-cols-3 gap-2">
            {voices.map((voice) => (
              <VoicePreview
                key={voice.id}
                voice={voice}
                isSelected={selectedVoice === voice.id}
                onSelect={() => setSelectedVoice(voice.id)}
              />
            ))}
          </div>
        </Section>
      )}

      {/* Speed */}
      <Section>
        <div className="card p-4">
          <div className="mb-3 flex items-center justify-between">
            <label htmlFor="tts-speed" className="text-[13px] font-semibold">
              Speed
            </label>
            <span className="text-[13px] tabular-nums text-muted">{speed.toFixed(1)}x</span>
          </div>
          <input
            id="tts-speed"
            type="range"
            min="0.5"
            max="3"
            step="0.1"
            value={speed}
            onChange={(e) => setSpeed(parseFloat(e.target.value))}
          />
          <div className="mt-2 flex justify-between text-[11px] text-subtle">
            <span>0.5x</span>
            <span>1.0x</span>
            <span>2.0x</span>
            <span>3.0x</span>
          </div>
          <p className="mt-2 text-xs text-muted">The voice keeps its pitch at any speed.</p>
        </div>
      </Section>

      {/* Voice cloning, not available yet */}
      <div className="card flex items-center justify-between gap-4 p-4">
        <div>
          <h3 className="text-[13px] font-semibold">Voice cloning</h3>
          <p className="mt-0.5 text-xs text-muted">Make a custom voice from a 5-second sample.</p>
        </div>
        <span className="badge-neutral whitespace-nowrap">Coming soon</span>
      </div>
    </div>
  );
}

import { useEffect, useState, useRef } from "react";

type OverlayMode = "recording" | "transcribing" | "speaking" | null;

interface FloatingOverlayProps {
  mode: OverlayMode;
  onStop: () => void;
  silenceProgress?: number; // 0-1 progress toward silence auto-stop
}

export default function FloatingOverlay({ mode, onStop, silenceProgress = 0 }: FloatingOverlayProps) {
  const [elapsedTime, setElapsedTime] = useState(0);
  const [isVisible, setIsVisible] = useState(false);
  const [audioLevel, setAudioLevel] = useState(0);
  const startTimeRef = useRef<number | null>(null);
  const animationFrameRef = useRef<number>();

  // Handle visibility with animation
  useEffect(() => {
    if (mode) {
      setIsVisible(true);
      startTimeRef.current = Date.now();
      setElapsedTime(0);
    } else {
      // Delay hiding for exit animation
      const timeout = setTimeout(() => setIsVisible(false), 200);
      return () => clearTimeout(timeout);
    }
  }, [mode]);

  // Update elapsed time
  useEffect(() => {
    if (!mode || !startTimeRef.current) return;

    const updateTime = () => {
      if (startTimeRef.current) {
        setElapsedTime(Math.floor((Date.now() - startTimeRef.current) / 1000));
      }
      animationFrameRef.current = requestAnimationFrame(updateTime);
    };

    animationFrameRef.current = requestAnimationFrame(updateTime);

    return () => {
      if (animationFrameRef.current) {
        cancelAnimationFrame(animationFrameRef.current);
      }
    };
  }, [mode]);

  // Simulate audio levels for recording mode
  useEffect(() => {
    if (mode !== "recording") {
      setAudioLevel(0);
      return;
    }

    const interval = setInterval(() => {
      // Simulate varying audio levels with some smoothing
      setAudioLevel((prev) => {
        const target = Math.random() * 0.7 + 0.1;
        return prev + (target - prev) * 0.3;
      });
    }, 50);

    return () => clearInterval(interval);
  }, [mode]);

  if (!isVisible) return null;

  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const getModeConfig = () => {
    switch (mode) {
      case "recording":
        return {
          bgColor: "bg-red-500",
          pulseColor: "bg-red-400",
          icon: <MicIcon className="w-5 h-5" />,
          label: "Recording",
          showWaveform: true,
        };
      case "transcribing":
        return {
          bgColor: "bg-amber-500",
          pulseColor: "bg-amber-400",
          icon: <SpinnerIcon className="w-5 h-5 animate-spin" />,
          label: "Transcribing",
          showWaveform: false,
        };
      case "speaking":
        return {
          bgColor: "bg-sky-500",
          pulseColor: "bg-sky-400",
          icon: <SpeakerIcon className="w-5 h-5" />,
          label: "Speaking",
          showWaveform: true,
        };
      default:
        return {
          bgColor: "bg-slate-600",
          pulseColor: "bg-slate-500",
          icon: null,
          label: "",
          showWaveform: false,
        };
    }
  };

  const config = getModeConfig();

  return (
    <div
      className={`fixed bottom-6 left-1/2 -translate-x-1/2 z-50 transition-all duration-200 ${
        mode ? "opacity-100 translate-y-0" : "opacity-0 translate-y-4"
      }`}
    >
      <div className="relative">
        {/* Pulse ring for recording */}
        {mode === "recording" && (
          <div className="absolute inset-0 rounded-full animate-ping opacity-30 bg-red-500" />
        )}

        {/* Main pill */}
        <div
          className={`relative flex items-center gap-3 px-4 py-3 rounded-full shadow-2xl ${config.bgColor} text-white`}
        >
          {/* Icon */}
          <div className="flex-shrink-0">{config.icon}</div>

          {/* Status and time */}
          <div className="flex flex-col min-w-[80px]">
            <span className="text-sm font-medium leading-tight">{config.label}</span>
            <span className="text-xs opacity-80 font-mono">{formatTime(elapsedTime)}</span>
          </div>

          {/* Audio level bars */}
          {config.showWaveform && (
            <div className="flex items-center gap-0.5 h-6">
              {[...Array(5)].map((_, i) => {
                const barLevel = mode === "speaking"
                  ? Math.sin(Date.now() / 200 + i) * 0.3 + 0.5
                  : audioLevel * (0.5 + Math.random() * 0.5);
                return (
                  <div
                    key={i}
                    className="w-1 bg-white/80 rounded-full transition-all duration-75"
                    style={{
                      height: `${Math.max(4, barLevel * 24)}px`,
                    }}
                  />
                );
              })}
            </div>
          )}

          {/* Silence progress indicator */}
          {mode === "recording" && silenceProgress > 0 && (
            <div className="w-8 h-1 bg-white/30 rounded-full overflow-hidden">
              <div
                className="h-full bg-white/80 transition-all duration-100"
                style={{ width: `${silenceProgress * 100}%` }}
              />
            </div>
          )}

          {/* Stop button */}
          <button
            onClick={onStop}
            className="flex-shrink-0 w-8 h-8 flex items-center justify-center rounded-full bg-white/20 hover:bg-white/30 transition-colors"
            title="Stop"
          >
            <StopIcon className="w-4 h-4" />
          </button>
        </div>
      </div>
    </div>
  );
}

// Compact floating indicator for hotkey-triggered actions (non-intrusive)
export function CompactOverlay({ mode, onStop }: { mode: OverlayMode; onStop: () => void }) {
  const [elapsedTime, setElapsedTime] = useState(0);
  const startTimeRef = useRef<number | null>(null);

  useEffect(() => {
    if (mode) {
      startTimeRef.current = Date.now();
      setElapsedTime(0);

      const interval = setInterval(() => {
        if (startTimeRef.current) {
          setElapsedTime(Math.floor((Date.now() - startTimeRef.current) / 1000));
        }
      }, 1000);

      return () => clearInterval(interval);
    }
  }, [mode]);

  if (!mode) return null;

  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const config = {
    recording: { bg: "bg-red-500", icon: "🎤", label: "Recording" },
    transcribing: { bg: "bg-amber-500", icon: "⏳", label: "Transcribing" },
    speaking: { bg: "bg-sky-500", icon: "🔊", label: "Speaking" },
  }[mode];

  return (
    <div className="fixed top-4 right-4 z-50 animate-slide-in-from-top">
      <div
        className={`flex items-center gap-2 px-3 py-2 rounded-full shadow-lg ${config.bg} text-white text-sm font-medium`}
      >
        <span className={mode === "recording" ? "animate-pulse" : ""}>{config.icon}</span>
        <span>{config.label}</span>
        <span className="font-mono text-xs opacity-80">{formatTime(elapsedTime)}</span>
        <button
          onClick={onStop}
          className="ml-1 w-6 h-6 flex items-center justify-center rounded-full bg-white/20 hover:bg-white/30"
        >
          <StopIcon className="w-3 h-3" />
        </button>
      </div>
    </div>
  );
}

function MicIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z"
      />
    </svg>
  );
}

function SpeakerIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z"
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

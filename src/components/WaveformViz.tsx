import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

interface WaveformVizProps {
  isActive: boolean;
}

/**
 * Scrolling RMS bar visualization driven by real microphone levels.
 * The backend emits "stt-audio-level" (~20 fps) from the actual audio
 * callback; each event pushes one bar. No self-animation: silence renders
 * as a visible flatline.
 */
export default function WaveformViz({ isActive }: WaveformVizProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const historyRef = useRef<number[]>(new Array(32).fill(0));

  useEffect(() => {
    if (!isActive) return;

    historyRef.current = new Array(32).fill(0);

    const draw = () => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      const bars = 32;
      const gap = 2;
      const barWidth = canvas.width / bars - gap;
      const history = historyRef.current;

      ctx.clearRect(0, 0, canvas.width, canvas.height);
      for (let i = 0; i < bars; i++) {
        // Normalize: typical speech RMS is 0..0.3
        const normalized = Math.min((history[i] || 0) / 0.15, 1.0);
        const height = Math.max(2, normalized * canvas.height * 0.9);

        const x = i * (barWidth + gap);
        const y = (canvas.height - height) / 2;

        ctx.fillStyle = "#0ea5e9";
        ctx.fillRect(x, y, barWidth, height);
      }
    };

    draw(); // Initial flatline

    let unlisten: (() => void) | null = null;
    listen<number>("stt-audio-level", (event) => {
      historyRef.current.push(event.payload || 0);
      if (historyRef.current.length > 32) {
        historyRef.current.shift();
      }
      draw(); // Redraw only when a real level arrives (~20 fps)
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, [isActive]);

  return (
    <canvas
      ref={canvasRef}
      width={400}
      height={60}
      className="w-full h-16 rounded"
    />
  );
}

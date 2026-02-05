import { useEffect, useRef } from "react";

interface WaveformVizProps {
  isActive: boolean;
}

export default function WaveformViz({ isActive }: WaveformVizProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>();

  useEffect(() => {
    if (!isActive || !canvasRef.current) return;

    const canvas = canvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const bars = 32;
    const barWidth = canvas.width / bars - 2;

    const draw = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);

      for (let i = 0; i < bars; i++) {
        // Simulate audio levels with random values
        const height = isActive
          ? Math.random() * canvas.height * 0.8 + canvas.height * 0.1
          : canvas.height * 0.1;

        const x = i * (barWidth + 2);
        const y = (canvas.height - height) / 2;

        ctx.fillStyle = "#0ea5e9";
        ctx.fillRect(x, y, barWidth, height);
      }

      animationRef.current = requestAnimationFrame(draw);
    };

    draw();

    return () => {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
      }
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

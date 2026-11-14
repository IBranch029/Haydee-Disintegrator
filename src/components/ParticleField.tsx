import { useEffect, useRef } from "react";

interface Particle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  life: number;
  size: number;
  hue: number;
}

/**
 * Canvas based disintegration effect.
 *
 * Particles are emitted from the centre of the viewport and drift outwards
 * while fading out - the visual metaphor of a target being vaporised.
 */
export default function ParticleField({
  active,
  intensity = 1,
}: {
  active: boolean;
  intensity?: number;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const particlesRef = useRef<Particle[]>([]);
  const frameRef = useRef<number>(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }

    const context = canvas.getContext("2d");
    if (!context) {
      return;
    }

    const resize = () => {
      const ratio = window.devicePixelRatio || 1;
      canvas.width = canvas.clientWidth * ratio;
      canvas.height = canvas.clientHeight * ratio;
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
    };

    resize();
    window.addEventListener("resize", resize);

    const spawn = () => {
      const amount = Math.round(3 * intensity);
      for (let index = 0; index < amount; index += 1) {
        const angle = Math.random() * Math.PI * 2;
        const speed = 1.4 + Math.random() * 3.6;
        particlesRef.current.push({
          x: canvas.clientWidth / 2 + (Math.random() - 0.5) * 120,
          y: canvas.clientHeight / 2 + (Math.random() - 0.5) * 120,
          vx: Math.cos(angle) * speed,
          vy: Math.sin(angle) * speed - 0.6,
          life: 1,
          size: 1 + Math.random() * 2.6,
          hue: 24 + Math.random() * 22,
        });
      }
    };

    const tick = () => {
      context.clearRect(0, 0, canvas.clientWidth, canvas.clientHeight);

      if (active) {
        spawn();
      }

      const particles = particlesRef.current;
      for (let index = particles.length - 1; index >= 0; index -= 1) {
        const particle = particles[index];
        particle.x += particle.vx;
        particle.y += particle.vy;
        particle.vy += 0.035;
        particle.life -= active ? 0.016 : 0.03;

        if (particle.life <= 0) {
          particles.splice(index, 1);
          continue;
        }

        context.beginPath();
        context.fillStyle = `hsla(${particle.hue}, 100%, 62%, ${Math.max(
          0,
          particle.life,
        )})`;
        context.shadowBlur = 8;
        context.shadowColor = `hsla(${particle.hue}, 100%, 55%, 0.7)`;
        context.fillRect(particle.x, particle.y, particle.size, particle.size);
      }

      context.shadowBlur = 0;
      frameRef.current = window.requestAnimationFrame(tick);
    };

    frameRef.current = window.requestAnimationFrame(tick);

    return () => {
      window.cancelAnimationFrame(frameRef.current);
      window.removeEventListener("resize", resize);
      particlesRef.current = [];
    };
  }, [active, intensity]);

  return <canvas className="particle-canvas" ref={canvasRef} />;
}

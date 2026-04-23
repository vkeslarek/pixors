import { useEffect, useRef, useState, useCallback } from 'react';
import init, { PixorsViewport } from 'pixors-viewport';

interface UseWasmViewportReturn {
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  viewportRef: React.RefObject<PixorsViewport | null>;
  gpuError: string | null;
  fit: () => void;
  pan: (dx: number, dy: number) => void;
  zoom: (factor: number, x: number, y: number) => void;
  isReady: boolean;
}

export function useWasmViewport(canvasId: string): UseWasmViewportReturn {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewportRef = useRef<PixorsViewport | null>(null);
  const [gpuError, setGpuError] = useState<string | null>(null);
  const [isReady, setIsReady] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let rafId: number | null = null;
    let resizeObserver: ResizeObserver | null = null;

    const boot = async () => {
      try {
        await init();
        if (cancelled || !canvasRef.current) return;

        const canvas = canvasRef.current;
        const viewport = await PixorsViewport.create(canvasId);
        if (cancelled) {
          viewport.free();
          return;
        }
        viewportRef.current = viewport;
        setIsReady(true);

        // 60 fps render loop
        let lastTime = 0;
        const renderLoop = (ts: number) => {
          if (cancelled) return;
          if (ts - lastTime >= 1000 / 60) {
            try {
              viewportRef.current?.render();
            } catch (e) {
              console.error('Render error:', e);
            }
            lastTime = ts;
          }
          rafId = requestAnimationFrame(renderLoop);
        };
        rafId = requestAnimationFrame(renderLoop);

        // Resize observer
        let resizeTimeout: ReturnType<typeof setTimeout>;
        resizeObserver = new ResizeObserver((entries) => {
          clearTimeout(resizeTimeout);
          resizeTimeout = setTimeout(() => {
            if (!viewportRef.current || !canvasRef.current) return;
            const entry = entries[0];
            if (!entry) return;
            const w = Math.max(1, Math.floor(entry.contentRect.width));
            const h = Math.max(1, Math.floor(entry.contentRect.height));
            if (canvasRef.current.width !== w || canvasRef.current.height !== h) {
              canvasRef.current.width = w;
              canvasRef.current.height = h;
              viewportRef.current.resize(w, h);
            }
          }, 100);
        });

        const { width, height } = canvas.getBoundingClientRect();
        const w = Math.max(1, Math.floor(width));
        const h = Math.max(1, Math.floor(height));
        canvas.width = w;
        canvas.height = h;
        viewport.resize(w, h);
        resizeObserver.observe(canvas);

      } catch (err: any) {
        console.error('Failed to init PixorsViewport:', err);
        setGpuError(String(err));
      }
    };

    boot();

    return () => {
      cancelled = true;
      if (rafId !== null) cancelAnimationFrame(rafId);
      if (resizeObserver && canvasRef.current) resizeObserver.unobserve(canvasRef.current);
      if (viewportRef.current) {
        viewportRef.current.free();
        viewportRef.current = null;
      }
      setIsReady(false);
    };
  }, [canvasId]);

  const fit = useCallback(() => {
    viewportRef.current?.fit();
  }, []);

  const pan = useCallback((dx: number, dy: number) => {
    viewportRef.current?.pan(dx, dy);
  }, []);

  const zoom = useCallback((factor: number, x: number, y: number) => {
    viewportRef.current?.zoom(factor, x, y);
  }, []);

  return { canvasRef, viewportRef, gpuError, fit, pan, zoom, isReady };
}

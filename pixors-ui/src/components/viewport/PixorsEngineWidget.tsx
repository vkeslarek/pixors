import { useEffect, useRef, useState, useCallback } from 'react';
import { initWasmEngine } from '@/engine/wasm-loader';
import type { PixorsEngine } from '../../../pixors-wasm/pkg/pixors_wasm';

export function PixorsEngineWidget() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const engineRef = useRef<PixorsEngine | null>(null);
  const draggingRef = useRef(false);
  const lastPosRef = useRef({ x: 0, y: 0 });
  const rafRef = useRef<number>(0);

  const [status, setStatus] = useState('Loading WASM...');
  const readyRef = useRef(false);

  // Init engine + viewport
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const engine = await initWasmEngine();
      if (cancelled || !engine) { setStatus('WASM unavailable'); return; }
      engineRef.current = engine;

      const canvas = canvasRef.current;
      if (!canvas) return;

      const w = Math.floor(canvas.clientWidth * devicePixelRatio);
      const h = Math.floor(canvas.clientHeight * devicePixelRatio);
      canvas.width = w;
      canvas.height = h;

      try {
        await engine.init_viewport(w, h);
        readyRef.current = true;
        setStatus('Checkerboard 2048×1536');
      } catch (e) {
        console.error('[WASM] init failed', e);
        setStatus('WGPU init failed');
      }
    })();
    return () => { cancelled = true; };
  }, []);

  // RAF render loop
  useEffect(() => {
    const loop = async () => {
      if (readyRef.current && engineRef.current) {
        try {
          const pixels = await engineRef.current.render();
          const canvas = canvasRef.current;
          if (!canvas) return;

          const ctx = canvas.getContext('2d');
          if (!ctx) return;

          const imgData = new ImageData(
            new Uint8ClampedArray(pixels),
            canvas.width, canvas.height,
          );
          ctx.putImageData(imgData, 0, 0);
        } catch (e) {
          // Surface errors during resize, silently ignore
        }
      }
      rafRef.current = requestAnimationFrame(loop);
    };
    rafRef.current = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(rafRef.current);
  }, []);

  // Resize observer
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        const w = Math.floor(width * devicePixelRatio);
        const h = Math.floor(height * devicePixelRatio);
        canvas.width = w;
        canvas.height = h;
        engineRef.current?.resize(w, h);
      }
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  // Gesture handlers
  const onMouseDown = useCallback((e: React.MouseEvent) => {
    const isPan = e.button === 1 || (e.ctrlKey && e.button === 0);
    if (!isPan) return;
    draggingRef.current = true;
    lastPosRef.current = { x: e.clientX, y: e.clientY };
    e.preventDefault();
  }, []);

  const onMouseMove = useCallback((e: React.MouseEvent) => {
    if (!draggingRef.current) return;
    const dx = e.clientX - lastPosRef.current.x;
    const dy = e.clientY - lastPosRef.current.y;
    lastPosRef.current = { x: e.clientX, y: e.clientY };
    engineRef.current?.pan(dx, dy);
  }, []);

  const onMouseUp = useCallback(() => { draggingRef.current = false; }, []);

  const onWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ax = e.nativeEvent.offsetX / canvas.width;
    const ay = e.nativeEvent.offsetY / canvas.height;
    engineRef.current?.zoom_at(e.deltaY > 0 ? 1.1 : 0.9, ax, ay);
  }, []);

  // Window shortcut events
  useEffect(() => {
    const handler = (e: Event) => {
      if (e instanceof CustomEvent) {
        if (e.detail === 'fit') engineRef.current?.fit();
      }
    };
    window.addEventListener('viewport:fit', handler);
    return () => window.removeEventListener('viewport:fit', handler);
  }, []);

  return (
    <div className="canvas-area" style={{ position: 'relative', width: '100%', height: '100%' }}>
      <canvas
        ref={canvasRef}
        style={{ display: 'block', width: '100%', height: '100%', cursor: 'grab' }}
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={onMouseUp}
        onMouseLeave={onMouseUp}
        onWheel={onWheel}
        onContextMenu={(e) => e.preventDefault()}
      />
      <div style={{ position: 'absolute', bottom: 16, left: 16 }}>
        <div style={{ padding: '6px 12px', background: 'rgba(20,20,20,0.8)', color: '#fff', fontSize: 12, fontFamily: 'monospace', borderRadius: 4 }}>
          ● WGPU {status}
        </div>
      </div>
    </div>
  );
}

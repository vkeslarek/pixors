import { useEffect, useRef, useState } from 'react';
import { initWasmEngine } from '@/engine/wasm-loader';
import type { PixorsEngine } from 'pixors-wasm';

export function PixorsEngineWidget() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const engineRef = useRef<PixorsEngine | null>(null);
  const readyRef = useRef(false);
  const rafRef = useRef<number>(0);

  const [status, setStatus] = useState('Loading WASM...');

  useEffect(() => {
    let cancelled = false;
    const canvas = canvasRef.current;
    if (!canvas) return;

    (async () => {
      const engine = await initWasmEngine();
      if (cancelled) return;
      if (!engine) { setStatus('WASM unavailable'); return; }
      engineRef.current = engine;

      try {
        await engine.init_viewport(canvas, canvas.width, canvas.height);
        if (cancelled) return;
        readyRef.current = true;
        setStatus('WGPU Live');
      } catch (e) {
        if (cancelled) return;
        console.error('[WASM] init failed', e);
        setStatus('WGPU init failed');
      }
    })();

    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const loop = () => {
      if (cancelled) return;
      const canvas = canvasRef.current;
      if (engineRef.current && readyRef.current && canvas && canvas.width > 0 && canvas.height > 0) {
        engineRef.current.render().then((pixels) => {
          if (cancelled) return;
          if (pixels && pixels.length > 0 && canvas) {
            const ctx = canvas.getContext('2d');
            if (ctx && pixels.length === canvas.width * canvas.height * 4) {
                const imgData = new ImageData(new Uint8ClampedArray(pixels.buffer, pixels.byteOffset, pixels.byteLength), canvas.width, canvas.height);
                ctx.putImageData(imgData, 0, 0);
            }
          }
          if (!cancelled) rafRef.current = requestAnimationFrame(loop);
        }).catch((err) => {
            console.error("Render loop err:", err);
            if (!cancelled) rafRef.current = requestAnimationFrame(loop);
        });
      } else {
        if (!cancelled) rafRef.current = requestAnimationFrame(loop);
      }
    };
    rafRef.current = requestAnimationFrame(loop);
    return () => {
        cancelled = true;
        cancelAnimationFrame(rafRef.current);
    };
  }, []);

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
        if (readyRef.current) {
          engineRef.current?.resize_viewport(w, h);
        }
      }
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  return (
    <div className="canvas-area" style={{ position: 'relative', width: '100%', height: '100%' }}>
      <canvas
        ref={canvasRef}
        width={640}
        height={480}
        style={{ display: 'block', width: '100%', height: '100%' }}
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

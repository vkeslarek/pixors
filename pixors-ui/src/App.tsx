import { useEffect, useRef, useState } from 'react'
import init, { PixorsViewport } from 'pixors-viewport'

function App() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewportRef = useRef<PixorsViewport | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const imageInfoRef = useRef<{ width: number; height: number } | null>(null);
  const [connected, setConnected] = useState(false);
  const [imageInfo, setImageInfo] = useState<{ width: number; height: number } | null>(null);
  const [gpuError, setGpuError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let rafId: number | null = null;
    let resizeObserver: ResizeObserver | null = null;

    const bootWasm = async () => {
      try {
        await init();
        if (cancelled || !canvasRef.current) return;

        const canvas = canvasRef.current;
        const viewport = await PixorsViewport.create("main-viewport");
        if (cancelled) {
          viewport.free();
          return;
        }
        viewportRef.current = viewport;

        // Render loop — stopped via `cancelled` flag
        let lastTime = 0;
        const frameInterval = 1000 / 60;
        const renderLoop = (timestamp: number) => {
          if (cancelled) return;
          if (timestamp - lastTime >= frameInterval) {
            if (viewportRef.current) {
              try {
                viewportRef.current.render();
              } catch (e) {
                console.error("Render error:", e);
              }
            }
            lastTime = timestamp;
          }
          rafId = requestAnimationFrame(renderLoop);
        };
        rafId = requestAnimationFrame(renderLoop);

        // Connect to engine WebSocket
        const ws = new WebSocket("ws://127.0.0.1:8080/ws");
        ws.binaryType = "arraybuffer";
        wsRef.current = ws;

        ws.onopen = () => {
          if (cancelled) { ws.close(); return; }
          console.log("Connected to engine");
          setConnected(true);
          setTimeout(() => {
            if (ws.readyState === WebSocket.OPEN) {
              ws.send(JSON.stringify({ type: "load_image", path: "example1.png" }));
            }
          }, 500);
        };

        ws.onmessage = (event) => {
          if (typeof event.data === 'string') {
            const msg = JSON.parse(event.data);
            if (msg.type === 'image_loaded') {
              const info = { width: msg.width, height: msg.height };
              imageInfoRef.current = info;
              setImageInfo(info);
              console.log(`Image loaded: ${msg.width}x${msg.height}`);
            } else if (msg.type === 'binary_data') {
              console.log(`Expecting binary data: ${msg.size} bytes`);
            }
          } else if (event.data instanceof ArrayBuffer) {
            const data = new Uint8Array(event.data);
            const info = imageInfoRef.current;
            if (viewportRef.current && info) {
              viewportRef.current.update_texture(info.width, info.height, data);
              console.log(`Texture updated with ${data.length} bytes`);
            }
          }
        };

        ws.onerror = (error) => console.error("WebSocket error:", error);
        ws.onclose = () => { setConnected(false); };

        // Mouse pan/zoom
        let isDragging = false;
        let lastX = 0;
        let lastY = 0;

        canvas.onmousedown = (e) => { isDragging = true; lastX = e.clientX; lastY = e.clientY; };
        canvas.onmousemove = (e) => {
          if (!isDragging || !viewportRef.current) return;
          viewportRef.current.pan(e.clientX - lastX, e.clientY - lastY);
          lastX = e.clientX; lastY = e.clientY;
        };
        canvas.onmouseup = () => { isDragging = false; };
        canvas.onwheel = (e) => {
          e.preventDefault();
          if (!viewportRef.current) return;
          const rect = canvas.getBoundingClientRect();
          viewportRef.current.zoom(
            e.deltaY > 0 ? 1.1 : 0.9,
            (e.clientX - rect.left) / rect.width,
            (e.clientY - rect.top) / rect.height,
          );
        };

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
        canvas.width = Math.max(1, Math.floor(width));
        canvas.height = Math.max(1, Math.floor(height));
        resizeObserver.observe(canvas);

      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        console.error("Failed to initialize viewport:", msg);
        setGpuError(msg);
      }
    };

    bootWasm();

    return () => {
      cancelled = true;
      if (rafId !== null) cancelAnimationFrame(rafId);
      resizeObserver?.disconnect();
      wsRef.current?.close();
      wsRef.current = null;
      if (viewportRef.current) {
        viewportRef.current.free();
        viewportRef.current = null;
      }
    };
  }, []);

  return (
    <div style={{ display: 'flex', gap: '20px', padding: '20px', fontFamily: 'sans-serif' }}>
      {/* UI em React */}
      <div style={{ width: '250px' }}>
        <h2>Pixors Editor</h2>
        <div style={{ marginBottom: '20px' }}>
          <div style={{ 
            display: 'inline-block', 
            width: '12px', 
            height: '12px', 
            borderRadius: '50%', 
            backgroundColor: connected ? '#4CAF50' : '#F44336',
            marginRight: '8px'
          }} />
          <span>{connected ? 'Connected to engine' : 'Disconnected'}</span>
        </div>
        
        {imageInfo && (
          <div style={{ marginBottom: '20px' }}>
            <h3>Image Info</h3>
            <p>Size: {imageInfo.width} × {imageInfo.height}</p>
          </div>
        )}

        <div style={{ marginBottom: '20px' }}>
          <h3>Controls</h3>
          <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
            <button onClick={() => {
              if (wsRef.current?.readyState === WebSocket.OPEN) {
                wsRef.current.send(JSON.stringify({ type: "load_image", path: "example1.png" }));
              }
            }}>
              Load Image
            </button>
            <button onClick={() => viewportRef.current?.fit()}>
              Fit Image
            </button>
          </div>
        </div>

        <div>
          <h3>Operations</h3>
          <button style={{ marginRight: '8px' }}>Brightness</button>
          <button>Contrast</button>
        </div>
      </div>

      {/* Viewport renderizado pelo Rust */}
      <div style={{
        width: '800px',
        height: '600px',
        overflow: 'hidden',
        position: 'relative'
      }}>
        {gpuError ? (
          <div style={{
            width: '100%',
            height: '100%',
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            background: '#1a1a1a',
            border: '2px solid #555',
            borderRadius: '8px',
            color: '#ccc',
            gap: '12px',
            padding: '24px',
            boxSizing: 'border-box',
          }}>
            <span style={{ fontSize: '32px' }}>⚠️</span>
            <strong>WebGL unavailable</strong>
            <span style={{ fontSize: '12px', color: '#888', textAlign: 'center' }}>
              {gpuError}
            </span>
            <span style={{ fontSize: '11px', color: '#666', textAlign: 'center' }}>
              Enable hardware acceleration in your browser settings.
            </span>
          </div>
        ) : (
          <>
            <canvas
              id="main-viewport"
              ref={canvasRef}
              width={800}
              height={600}
              style={{
                border: '2px solid #ccc',
                borderRadius: '8px',
                cursor: 'grab',
                display: 'block'
              }}
            />
            <div style={{ marginTop: '8px', fontSize: '12px', color: '#666' }}>
              Drag to pan • Scroll to zoom
            </div>
          </>
        )}
      </div>
    </div>
  )
}

export default App
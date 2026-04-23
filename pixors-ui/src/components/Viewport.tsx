
import { useWasmViewport } from './viewport/useWasmViewport';
import { useViewportGestures } from './viewport/useViewportGestures';
import { useTileStream } from './viewport/useTileStream';

export interface ViewportComponentProps {
  activeTool: string;
  zoom: number;
  onMouseMove: (x: number, y: number) => void;
  tabId: string | null;
}

export function Viewport({ activeTool, onMouseMove, tabId }: ViewportComponentProps) {
  const { canvasRef, viewportRef, gpuError, fit, pan, zoom: viewportZoom, isReady } = useWasmViewport('main-viewport');
  
  const { fitZoomRef, currentZoomRef } = useViewportGestures({
    canvasRef,
    fit,
    pan,
    zoom: viewportZoom,
    isReady,
  });

  const { connected } = useTileStream({
    tabId,
    viewportRef,
    canvasRef,
    isReady,
    fitZoomRef,
    currentZoomRef,
  });

  if (gpuError) {
    return (
      <div style={{ padding: '2rem', color: 'red' }}>
        <h2>WebGPU Initialization Failed</h2>
        <pre>{gpuError}</pre>
        <p>Ensure hardware acceleration is enabled in your browser.</p>
      </div>
    );
  }

  return (
    <div 
      className={`canvas-area tool-${activeTool}`}
      onMouseMove={(e) => {
        const rect = e.currentTarget.getBoundingClientRect();
        onMouseMove(e.clientX - rect.left, e.clientY - rect.top);
      }}
    >
      <canvas
        id="main-viewport"
        ref={canvasRef}
        className="viewport-canvas"
        onContextMenu={e => e.preventDefault()}
      />
      
      {/* HUD overlays */}
      <div style={{ position: 'absolute', bottom: '16px', left: '16px', display: 'flex', gap: '8px' }}>
        <div style={{ padding: '6px 12px', background: 'rgba(20,20,20,0.8)', color: '#fff', fontSize: '12px', fontFamily: 'monospace', borderRadius: '4px' }}>
          {tabId ? (connected ? '● Connected' : '○ Connecting...') : 'No Image'}
        </div>
        <div style={{ padding: '6px 12px', background: 'rgba(20,20,20,0.8)', color: '#fff', fontSize: '12px', fontFamily: 'monospace', borderRadius: '4px' }}>
          {activeTool.toUpperCase()}
        </div>
      </div>
    </div>
  );
}

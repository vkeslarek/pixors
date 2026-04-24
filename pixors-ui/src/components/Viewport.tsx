
import React, { useEffect } from 'react';
import { useViewportGestures } from './viewport/useViewportGestures';
import type { PixorsViewport } from 'pixors-viewport';
import type { EngineCommand } from '../engine/types';

export interface ViewportComponentProps {
  activeTool: string;
  onMouseMove: (x: number, y: number) => void;
  sendCommand: (cmd: EngineCommand) => void;
  requestTiles: (tabId: string, x: number, y: number, w: number, h: number, zoom: number) => void;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  viewportRef: React.RefObject<PixorsViewport | null>;
  gpuError: string | null;
  fit: () => void;
  pan: (dx: number, dy: number) => void;
  zoom: (factor: number, x: number, y: number) => void;
  isReady: boolean;
  tabId: string | null;
  connected: boolean;
  imageWidth?: number;
  imageHeight?: number;
}

export function Viewport({ 
  activeTool, 
  onMouseMove, 
  sendCommand,
  requestTiles,
  canvasRef,
  viewportRef: _viewportRef,
  gpuError,
  fit,
  pan,
  zoom: viewportZoom,
  isReady,
  tabId,
  connected,
  imageWidth,
  imageHeight
}: ViewportComponentProps) {
  
  const { emitChange } = useViewportGestures({
    canvasRef,
    fit,
    pan,
    zoom: viewportZoom,
    isReady,
    imageWidth,
    imageHeight,
    onViewportChange: (x, y, w, h, zoom) => {
      sendCommand({ type: 'viewport_update', x, y, w, h, zoom });
      if (tabId) {
        requestTiles(tabId, x, y, w, h, zoom);
      }
    }
  });

  // Emit initial viewport size
  useEffect(() => {
    if (isReady && connected && canvasRef.current && tabId) {
      emitChange();
    }
  }, [isReady, connected, canvasRef, tabId]);

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

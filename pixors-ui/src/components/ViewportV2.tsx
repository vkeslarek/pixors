/**
 * ViewportV2 — Canvas 2D viewport, no WASM/WebGPU.
 * Acts as the drop-in React wrapper for the native canvas renderer.
 * Handles mouse events, zooming, and coordinating tile requests with the Engine.
 */
import { useEffect, useRef } from 'react';
import { useCanvas2DViewport } from './viewport/ViewportCanvas2D';
import { useEngineEvent } from '../engine';
import type { EngineCommand } from '../engine/types';

interface Props {
  tabId: string | null;
  imageWidth?: number;
  imageHeight?: number;
  activeTool: string;
  connected: boolean;
  sendCommand: (cmd: EngineCommand) => void;
  onMouseMove: (x: number, y: number) => void;
}

export function ViewportV2({ tabId, imageWidth, imageHeight, activeTool, connected, sendCommand, onMouseMove }: Props) {
  const { canvasRef, isReady, fit, pan, zoom, getCamera, clearOldTiles } = useCanvas2DViewport(tabId);

  // Debounce ref to prevent flooding the engine with tile requests during rapid panning
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  
  // Pending flag to ensure we don't request tiles if a previous request is still streaming
  const pendingRef = useRef(false);
  
  // Tracks the timestamp of the last mouse move event dispatched to the StatusBar to throttle React updates
  const lastMousePosRef = useRef(0);

  /**
   * Dispatches a command to the engine to generate/stream tiles for the current visible area.
   */
  const requestTiles = () => {
    if (!tabId || !canvasRef.current || !imageWidth || !imageHeight) return;
    const { panX, panY, zoom: z } = getCamera();
    const canvas = canvasRef.current;
    
    // Convert screen dimensions to image space
    const w = canvas.width / z;
    const h = canvas.height / z;
    
    sendCommand({ type: 'request_tiles', tab_id: tabId, x: panX, y: panY, w, h, zoom: z });
    pendingRef.current = true;
  };

  /**
   * Debounces tile requests. Only actually triggers 120ms after the user stops moving the canvas.
   * This is critical to maintain high 60fps pan performance, as the viewport natively translates
   * existing tiles until movement stops.
   */
  const requestDebounced = () => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(requestTiles, 120);
  };

  // Canvas event handlers
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !isReady) return;

    let dragging = false;
    let lastX = 0;
    let lastY = 0;
    let spaceDown = false;

    const onMouseDown = (e: MouseEvent) => {
      const isPan = e.button === 1 || ((e.ctrlKey || spaceDown) && e.button === 0);
      if (!isPan) return;
      dragging = true;
      lastX = e.clientX;
      lastY = e.clientY;
      e.preventDefault();
    };
    const onMouseMoveHandler = (e: MouseEvent) => {
      const rect = canvas.getBoundingClientRect();
      const now = Date.now();
      
      // Throttle mouse coordinates event (used by StatusBar) to ~20 FPS.
      // This massively improves performance by decoupling native mouse move speed
      // from heavy React component re-renders higher in the DOM tree.
      if (now - lastMousePosRef.current > 50) {
        onMouseMove(e.clientX - rect.left, e.clientY - rect.top);
        lastMousePosRef.current = now;
      }
      
      if (!dragging) return;
      
      // Translate the camera (pan) using the delta movement
      const dx = e.clientX - lastX;
      const dy = e.clientY - lastY;
      pan(dx, dy);
      
      lastX = e.clientX;
      lastY = e.clientY;
      
      // Defer loading new tiles until movement stops
      requestDebounced();
    };
    const onMouseUp = () => { dragging = false; };
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const ax = (e.clientX - rect.left) / rect.width;
      const ay = (e.clientY - rect.top) / rect.height;
      zoom(e.deltaY > 0 ? 1.1 : 0.9, ax, ay);
      requestDebounced();
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.code === 'Space') spaceDown = true;
      if (e.key === 'Home') {
        if (imageWidth && imageHeight) fit(imageWidth, imageHeight);
        requestDebounced();
        e.preventDefault();
      }
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.code === 'Space') spaceDown = false;
    };

    canvas.addEventListener('mousedown', onMouseDown);
    canvas.addEventListener('mousemove', onMouseMoveHandler);
    canvas.addEventListener('mouseup', onMouseUp);
    canvas.addEventListener('mouseleave', onMouseUp);
    canvas.addEventListener('wheel', onWheel, { passive: false });
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    return () => {
      canvas.removeEventListener('mousedown', onMouseDown);
      canvas.removeEventListener('mousemove', onMouseMoveHandler);
      canvas.removeEventListener('mouseup', onMouseUp);
      canvas.removeEventListener('mouseleave', onMouseUp);
      canvas.removeEventListener('wheel', onWheel);
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
    };
  }, [canvasRef, isReady, pan, zoom, fit, imageWidth, imageHeight]);

  // Fit and request tiles when image dimensions arrive
  useEffect(() => {
    if (!isReady || !imageWidth || !imageHeight) return;
    fit(imageWidth, imageHeight);
    requestTiles();
  }, [isReady, imageWidth, imageHeight]);

  // Re-request on new MIP
  useEngineEvent('mip_level_ready', (msg) => {
    if (msg.tab_id === tabId && !pendingRef.current) {
      pendingRef.current = true;
      requestTiles();
    }
  });
  useEngineEvent('tiles_complete', () => { 
    pendingRef.current = false; 
    clearOldTiles();
  });
  useEngineEvent('tiles_dirty', (msg) => {
    if (msg.tab_id === tabId) { pendingRef.current = false; requestTiles(); }
  });

  return (
    <div className={`canvas-area tool-${activeTool}`} style={{ position: 'relative', width: '100%', height: '100%' }}>
      <canvas
        ref={canvasRef}
        style={{ display: 'block', width: '100%', height: '100%', cursor: activeTool === 'hand' ? 'grab' : 'default' }}
        onContextMenu={(e) => e.preventDefault()}
      />
      <div style={{ position: 'absolute', bottom: 16, left: 16, display: 'flex', gap: 8 }}>
        <div style={{ padding: '6px 12px', background: 'rgba(20,20,20,0.8)', color: '#fff', fontSize: 12, fontFamily: 'monospace', borderRadius: 4 }}>
          {tabId ? (connected ? '● Canvas2D' : '○ Connecting...') : 'No Image'}
        </div>
        <div style={{ padding: '6px 12px', background: 'rgba(20,20,20,0.8)', color: '#fff', fontSize: 12, fontFamily: 'monospace', borderRadius: 4 }}>
          {activeTool.toUpperCase()}
        </div>
      </div>
    </div>
  );
}

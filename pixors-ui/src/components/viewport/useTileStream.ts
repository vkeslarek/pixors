import { useEffect, useRef, useState, type MutableRefObject } from 'react';
import type { PixorsViewport } from 'pixors-viewport';
import type { EngineEvent, EngineCommand } from '../../engine/types';

const ENGINE_WS = 'ws://127.0.0.1:8080';

interface UseTileStreamProps {
  tabId: string | null;
  viewportRef: React.RefObject<PixorsViewport | null>;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  isReady: boolean;
  fitZoomRef: MutableRefObject<number>;
  currentZoomRef: MutableRefObject<number>;
}

export function useTileStream({
  tabId,
  viewportRef,
  canvasRef,
  isReady,
  fitZoomRef,
  currentZoomRef,
}: UseTileStreamProps) {
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const hasTextureRef = useRef(false);
  const pendingTileRef = useRef<{ x: number; y: number; width: number; height: number; mip_level: number } | null>(null);

  useEffect(() => {
    if (!tabId || !isReady || !canvasRef.current) {
      console.log('No tabId or viewport not ready, skipping WebSocket connection');
      return;
    }

    const canvas = canvasRef.current;
    const ws = new WebSocket(`${ENGINE_WS}/ws?tab_id=${tabId}`);
    ws.binaryType = 'arraybuffer';
    wsRef.current = ws;
    let cancelled = false;

    ws.onopen = () => {
      if (cancelled) {
        ws.close();
        return;
      }
      console.log('Connected to engine for tab', tabId);
      setConnected(true);
      const { width, height } = canvas.getBoundingClientRect();
      ws.send(JSON.stringify({
        type: 'viewport_update',
        x: 0, y: 0, w: width, h: height, zoom: 1.0
      } as EngineCommand));
    };

    ws.onmessage = async (event) => {
      if (typeof event.data === 'string') {
        try {
          const msg = JSON.parse(event.data) as EngineEvent;
          if (msg.type === 'image_loaded') {
            console.log(`Image loaded: ${msg.width}x${msg.height}`);
            viewportRef.current?.create_empty_texture(msg.width, msg.height);
            hasTextureRef.current = true;

            if (ws.readyState === WebSocket.OPEN) {
              const rect = canvas.getBoundingClientRect();
              const fitZoom = Math.min(rect.width / msg.width, rect.height / msg.height);
              fitZoomRef.current = fitZoom > 0 ? fitZoom : 1;
              currentZoomRef.current = fitZoomRef.current;
              ws.send(JSON.stringify({
                type: 'viewport_update',
                x: 0,
                y: 0,
                w: rect.width,
                h: rect.height,
                zoom: fitZoomRef.current,
              } as EngineCommand));
            }
          } else if (msg.type === 'tile_data') {
            if (msg.tab_id !== tabId || !hasTextureRef.current) {
              console.warn('Dropping tile_data: tab mismatch or no texture', msg.tab_id, tabId, hasTextureRef.current);
              return;
            }
            pendingTileRef.current = { x: msg.x, y: msg.y, width: msg.width, height: msg.height, mip_level: msg.mip_level };
          } else if (msg.type === 'error') {
            console.error('Engine error:', msg.message);
          } else if (msg.type === 'tiles_dirty') {
            if (ws.readyState === WebSocket.OPEN) {
              const { width, height } = canvas.getBoundingClientRect();
              ws.send(JSON.stringify({
                type: 'viewport_update',
                x: 0, y: 0, w: width, h: height, zoom: 1.0
              } as EngineCommand));
            }
          }
        } catch (err) {
          console.error('Failed to parse message:', err);
        }
      } else {
        // Binary tile pixel data
        if (!viewportRef.current || !hasTextureRef.current) {
          console.warn('Binary data: no viewport or texture');
          return;
        }
        const tile = pendingTileRef.current;
        if (!tile) {
          console.warn('Binary data: no pending tile metadata');
          return;
        }
        try {
          let bytes: Uint8Array;
          if (event.data instanceof ArrayBuffer) {
            bytes = new Uint8Array(event.data.byteLength);
            bytes.set(new Uint8Array(event.data));
          } else if (event.data instanceof Blob) {
            const ab = await event.data.arrayBuffer();
            bytes = new Uint8Array(ab);
          } else {
            console.error('Unknown binary data type:', typeof event.data, event.data);
            return;
          }

          if (bytes.length === 0) {
            console.error('Tile data is empty after copy! Original byteLength:', 
              event.data instanceof ArrayBuffer ? event.data.byteLength : 'Blob');
            return;
          }

          viewportRef.current.write_tile(
            tile.x,
            tile.y,
            tile.width,
            tile.height,
            tile.mip_level,
            bytes,
          );
        } catch (err) {
          console.error('write_tile error:', err);
        } finally {
          pendingTileRef.current = null;
        }
      }
    };

    ws.onclose = () => {
      setConnected(false);
    };

    ws.onerror = (err) => {
      console.error('TileStream WebSocket error:', err);
    };

    return () => {
      cancelled = true;
      ws.close();
    };
  }, [tabId, viewportRef, canvasRef, isReady, fitZoomRef, currentZoomRef]);

  return { connected };
}

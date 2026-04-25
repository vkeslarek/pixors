/**
 * ViewportCanvas2D — pure Canvas 2D tile renderer, no WASM/WebGPU.
 *
 * Receives RGBA8 tiles from the engine WebSocket and composites them
 * on a standard 2D canvas with pan/zoom. Drop-in replacement for the
 * WASM viewport for comparison testing.
 *
 * Tile coordinate space: px/py are in MIP-level space. To draw on
 * the image-space canvas, scale by 2^mip_level.
 */
import { useEffect, useRef, useCallback, useState } from 'react';
import { engineClient, MSG_TILE } from '../../engine/client';
import { LRUTileCache } from './LRUTileCache';

// --- Types ---

interface Camera {
  panX: number;   // image-space X at canvas top-left
  panY: number;   // image-space Y at canvas top-left
  zoom: number;   // screen pixels per image pixel
}

interface TileEntry {
  bitmap: ImageBitmap;
  imgX: number;   // image-space top-left X
  imgY: number;   // image-space top-left Y
  imgW: number;   // image-space width
  imgH: number;   // image-space height
  mipLevel: number;
}

// --- Hook ---

export interface UseCanvas2DViewportReturn {
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  isReady: boolean;
  error: string | null;
  fit: (imgW: number, imgH: number) => void;
  pan: (dx: number, dy: number) => void;
  zoom: (factor: number, anchorScreenX: number, anchorScreenY: number) => void;
  getCamera: () => Camera;
  clearOldTiles: () => void;
  hasAllTiles: (keys: string[]) => boolean;
}

/**
 * useCanvas2DViewport handles the core native 2D Canvas rendering logic.
 * It manages the camera state (pan/zoom) locally via refs to avoid React renders,
 * handles incoming WebSockets tiles, and runs a high-performance requestAnimationFrame loop.
 *
 * @param tabId The ID of the currently active tab.
 * @returns Ref to the canvas element and control methods (pan, zoom, fit, getCamera).
 */
export function useCanvas2DViewport(tabId: string | null) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [isReady, setIsReady] = useState(false);
  const [error] = useState<string | null>(null);

  const cameraRef = useRef<Camera>({ panX: 0, panY: 0, zoom: 1 });
  const tilesRef = useRef<LRUTileCache>(new LRUTileCache(256));
  const rafRef = useRef<number | null>(null);
  const tabIdRef = useRef(tabId);
  const renderRef = useRef<() => void>(() => {});

  useEffect(() => { tabIdRef.current = tabId; }, [tabId]);

  const render = useCallback(() => {

    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const { panX, panY, zoom } = cameraRef.current;
    const W = canvas.width;
    const H = canvas.height;

    ctx.clearRect(0, 0, W, H);

    // Draw solid background instead of checkerboard
    ctx.fillStyle = '#2a2a2a';
    ctx.fillRect(0, 0, W, H);

    // Disable interpolation for pixel-perfect rendering
    ctx.imageSmoothingEnabled = false;

    // Collect tiles sorted by (key, tile) pairs: higher mip first so higher-res overwrites.
    // Sorting creates a new array; LRU entries() iteration does NOT change LRU order.
    const sorted = Array.from(tilesRef.current.entries())
      .sort(([, a], [, b]) => b.mipLevel - a.mipLevel);

    for (const [key, tile] of sorted) {
      const screenX = Math.floor((tile.imgX - panX) * zoom);
      const screenY = Math.floor((tile.imgY - panY) * zoom);
      const screenW = Math.ceil(tile.imgW * zoom);
      const screenH = Math.ceil(tile.imgH * zoom);

      // Cull off-screen tiles
      if (screenX + screenW < 0 || screenY + screenH < 0 || screenX > W || screenY > H) continue;

      ctx.drawImage(tile.bitmap, screenX, screenY, screenW, screenH);

      // Touch on-screen tile in LRU so visible tiles stay cached
      tilesRef.current.get(key);
    }

    // Pixel grid overlay when zoomed in enough to see individual pixels
    if (zoom >= 6) {
      const pxStartX = Math.floor(panX);
      const pxEndX = Math.ceil(panX + W / zoom);
      const pxStartY = Math.floor(panY);
      const pxEndY = Math.ceil(panY + H / zoom);

      ctx.strokeStyle = 'rgba(255,255,255,0.08)';
      ctx.lineWidth = 1;

      ctx.beginPath();
      for (let x = pxStartX; x <= pxEndX; x++) {
        const sx = (x - panX) * zoom + 0.5;
        ctx.moveTo(sx, 0);
        ctx.lineTo(sx, H);
      }
      for (let y = pxStartY; y <= pxEndY; y++) {
        const sy = (y - panY) * zoom + 0.5;
        ctx.moveTo(0, sy);
        ctx.lineTo(W, sy);
      }
      ctx.stroke();
    }
  }, []);

  // Animation loop
  useEffect(() => {
    let cancelled = false;
    const loop = () => {
      if (cancelled) return;
      render();
      rafRef.current = requestAnimationFrame(loop);
    };
    rafRef.current = requestAnimationFrame(loop);
    return () => {
      cancelled = true;
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    };
  }, [render]);

  useEffect(() => { renderRef.current = render; }, [render]);

  // Canvas setup + resize observer
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const w = Math.max(1, Math.floor(rect.width));
      const h = Math.max(1, Math.floor(rect.height));
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
    };

    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);
    setIsReady(true);

    return () => ro.disconnect();
  }, []);

  // Binary tile reception
  useEffect(() => {
    const unsub = engineClient.onBinary((type, payload) => {
      if (type !== MSG_TILE) return;

      const view = new DataView(payload.buffer, payload.byteOffset);
      const px = view.getUint32(0, true);
      const py = view.getUint32(4, true);
      const w = view.getUint32(8, true);
      const h = view.getUint32(12, true);
      const mipLevel = view.getUint32(16, true);
      // bytes 20..36 = tab_id UUID (ignored for routing, handled by engine)

      if (w === 0 || h === 0) return;

      const rgba = new Uint8ClampedArray(
        payload.buffer,
        payload.byteOffset + 36,
        w * h * 4,
      );

      const imageData = new ImageData(rgba.slice(), w, h);
      const mipScale = Math.pow(2, mipLevel);
      const imgX = px * mipScale;
      const imgY = py * mipScale;
      const imgW = w * mipScale;
      const imgH = h * mipScale;

      createImageBitmap(imageData).then((bitmap) => {
        const key = `${mipLevel}_${px}_${py}`;
        tilesRef.current.set(key, { bitmap, imgX, imgY, imgW, imgH, mipLevel });
        latestMipRef.current = mipLevel;
        renderRef.current();  // render immediately on tile ready (tailing effect)
      });
    });
    return unsub;
  }, []);

  // Clear tiles when tab changes
  useEffect(() => {
    tilesRef.current.clear();
  }, [tabId]);

  // --- Camera controls ---

  const fit = useCallback((imgW: number, imgH: number) => {
    const canvas = canvasRef.current;
    if (!canvas || imgW === 0 || imgH === 0) return;
    const scaleX = canvas.width / imgW;
    const scaleY = canvas.height / imgH;
    const z = Math.min(scaleX, scaleY);
    cameraRef.current = {
      zoom: z,
      panX: -(canvas.width / z - imgW) / 2,
      panY: -(canvas.height / z - imgH) / 2,
    };
  }, []);

  const pan = useCallback((dx: number, dy: number) => {
    const cam = cameraRef.current;
    cameraRef.current = {
      ...cam,
      panX: cam.panX - dx / cam.zoom,
      panY: cam.panY - dy / cam.zoom,
    };
  }, []);

  const zoom = useCallback((factor: number, anchorScreenX: number, anchorScreenY: number) => {
    const cam = cameraRef.current;
    const canvas = canvasRef.current;
    if (!canvas) return;

    // anchor in image space before zoom
    const anchorImgX = anchorScreenX * canvas.width / cam.zoom + cam.panX;
    const anchorImgY = anchorScreenY * canvas.height / cam.zoom + cam.panY;

    const newZoom = Math.max(0.02, Math.min(100, cam.zoom * factor));

    // keep anchor fixed
    cameraRef.current = {
      zoom: newZoom,
      panX: anchorImgX - (anchorScreenX * canvas.width) / newZoom,
      panY: anchorImgY - (anchorScreenY * canvas.height) / newZoom,
    };
  }, []);

  // Track the most recently received MIP level to clean up stale tiles
  const latestMipRef = useRef<number>(-1);

  const clearOldTiles = useCallback(() => {
    const targetMip = latestMipRef.current;
    if (targetMip === -1) return;
    const { panX, panY, zoom: z } = cameraRef.current;
    const canvas = canvasRef.current;
    if (!canvas) return;

    // Viewport bounds in image space
    const vW = canvas.width / z;
    const vH = canvas.height / z;
    // Add margin (e.g. 1 viewport size) to keep tiles just off-screen
    const minX = panX - vW;
    const minY = panY - vH;
    const maxX = panX + vW * 2;
    const maxY = panY + vH * 2;

    for (const [key, tile] of Array.from(tilesRef.current.entries())) {
      const isWrongMip = tile.mipLevel !== targetMip;
      const isOffscreen = tile.imgX + tile.imgW < minX || tile.imgY + tile.imgH < minY || 
                          tile.imgX > maxX || tile.imgY > maxY;
      
      if (isWrongMip || isOffscreen) {
        tilesRef.current.delete(key);
      }
    }
  }, []);

  const getCamera = useCallback(() => cameraRef.current, []);

  const hasAllTiles = useCallback((keys: string[]): boolean => {
    for (const key of keys) {
      if (!tilesRef.current.has(key)) return false;
    }
    return true;
  }, []);

  return { canvasRef, isReady, error, fit, pan, zoom, getCamera, clearOldTiles, hasAllTiles };
}

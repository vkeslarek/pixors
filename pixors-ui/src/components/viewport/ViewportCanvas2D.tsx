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

  // Holds the camera state. We use a ref instead of useState to ensure
  // 60fps panning without triggering React component tree reconciliation.
  const cameraRef = useRef<Camera>({ panX: 0, panY: 0, zoom: 1 });
  
  // Stores the actual bitmap data for each tile. Keyed by `${mipLevel}_${px}_${py}`
  const tilesRef = useRef<Map<string, TileEntry>>(new Map());
  const rafRef = useRef<number | null>(null);
  const tabIdRef = useRef(tabId);

  useEffect(() => { tabIdRef.current = tabId; }, [tabId]);

  /**
   * The core render loop. Called on every animation frame.
   * Clears the canvas, sorts tiles by MIP level, and composites them.
   */
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

    // Collect tiles and sort: higher mip (lower res) first so higher-res overwrites
    const tiles = Array.from(tilesRef.current.values())
      .sort((a, b) => b.mipLevel - a.mipLevel);

    for (const tile of tiles) {
      const screenX = (tile.imgX - panX) * zoom;
      const screenY = (tile.imgY - panY) * zoom;
      const screenW = tile.imgW * zoom;
      const screenH = tile.imgH * zoom;

      // Cull off-screen tiles
      if (screenX + screenW < 0 || screenY + screenH < 0 || screenX > W || screenY > H) continue;

      ctx.drawImage(tile.bitmap, screenX, screenY, screenW, screenH);
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
        // Evict lower-res tile if a higher-res tile arrives for same region
        const existing = tilesRef.current.get(key);
        if (existing) existing.bitmap.close();
        tilesRef.current.set(key, { bitmap, imgX, imgY, imgW, imgH, mipLevel });
        latestMipRef.current = mipLevel;
      });
    });
    return unsub;
  }, []);

  // Clear tiles when tab changes
  useEffect(() => {
    tilesRef.current.forEach((t) => t.bitmap.close());
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
        tile.bitmap.close();
        tilesRef.current.delete(key);
      }
    }
  }, []);

  const getCamera = useCallback(() => cameraRef.current, []);

  return { canvasRef, isReady, error, fit, pan, zoom, getCamera, clearOldTiles };
}

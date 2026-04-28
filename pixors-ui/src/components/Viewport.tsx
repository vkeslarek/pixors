import { useEffect, useRef, useState } from 'react';
import { useCanvas2DViewport } from '@/components/viewport/ViewportCanvas2D';
import { useEvent, useCommand, useConnected } from '@/engine/events';
import { useUIStore } from '@/ui/uiStore';

export function Viewport() {
  const [tabId, setTabId] = useState<string | null>(null);
  const [imageW, setImageW] = useState(0);
  const [imageH, setImageH] = useState(0);
  const [activeTool, setActiveTool] = useState('pan');
  const connected = useConnected();

  useEvent('tab_state', (ev) => setTabId(ev.active_tab_id));
  useEvent('tab_activated', (ev) => setTabId(ev.tab_id));
  useEvent('image_loaded', (ev) => {
    if (ev.tab_id === tabId) { setImageW(ev.width); setImageH(ev.height); }
  });
  useEvent('tool_state', (ev) => setActiveTool(ev.tool));
  useEvent('tool_changed', (ev) => setActiveTool(ev.tool));

  const { canvasRef, isReady, fit, pan, zoom, getCamera, hasAllTiles } = useCanvas2DViewport(tabId);

  const tabIdRef = useRef(tabId);
  tabIdRef.current = tabId;
  const imageWRef = useRef(imageW);
  imageWRef.current = imageW;
  const imageHRef = useRef(imageH);
  imageHRef.current = imageH;

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRef = useRef(false);

  const TILE_MARGIN = 0.5;
  const TILE_SIZE = 256;

  const mipLevelForZoom = (z: number): number => {
    if (z >= 1) return 0;
    return Math.ceil(Math.log2(1 / z));
  };

  const tileKeysForViewport = (panX: number, panY: number, vpW: number, vpH: number, mipLevel: number): string[] => {
    const mipScale = Math.pow(2, mipLevel);
    const mipX = panX / mipScale;
    const mipY = panY / mipScale;
    const mipW = vpW / mipScale;
    const mipH = vpH / mipScale;
    const txMin = Math.floor(mipX / TILE_SIZE) * TILE_SIZE;
    const tyMin = Math.floor(mipY / TILE_SIZE) * TILE_SIZE;
    const txMax = Math.floor((mipX + mipW) / TILE_SIZE) * TILE_SIZE;
    const tyMax = Math.floor((mipY + mipH) / TILE_SIZE) * TILE_SIZE;
    const keys: string[] = [];
    for (let py = tyMin; py <= tyMax; py += TILE_SIZE) {
      for (let px = txMin; px <= txMax; px += TILE_SIZE) {
        keys.push(`${mipLevel}_${px}_${py}`);
      }
    }
    return keys;
  };

  const requestTilesCmd = useCommand('request_tiles');

  const requestTiles = (force = false) => {
    const id = tabIdRef.current;
    const iw = imageWRef.current;
    const ih = imageHRef.current;
    const canvas = canvasRef.current;
    if (!id || !canvas || !iw || !ih) return;

    const { panX, panY, zoom: z } = getCamera();
    const w = canvas.width / z;
    const h = canvas.height / z;

    if (!force) {
      const mipLevel = mipLevelForZoom(z);
      const keys = tileKeysForViewport(panX, panY, w, h, mipLevel);
      if (keys.length > 0 && hasAllTiles(keys)) return;
    }

    const mx = w * TILE_MARGIN;
    const my = h * TILE_MARGIN;
    requestTilesCmd({ tab_id: id, x: panX - mx, y: panY - my, w: w + 2 * mx, h: h + 2 * my, zoom: z });
    pendingRef.current = true;
  };

  const requestDebounced = () => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(requestTiles, 120);
  };

  // Canvas mouse event handlers
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
      useUIStore.getState().setMousePos({
        x: Math.round(e.clientX - rect.left),
        y: Math.round(e.clientY - rect.top),
      });
      if (!dragging) return;
      const dx = e.clientX - lastX;
      const dy = e.clientY - lastY;
      pan(dx, dy);
      lastX = e.clientX;
      lastY = e.clientY;
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
        const iw = imageWRef.current;
        const ih = imageHRef.current;
        if (iw && ih) fit(iw, ih);
        requestDebounced();
        e.preventDefault();
      }
    };
    const onKeyUp = (e: KeyboardEvent) => { if (e.code === 'Space') spaceDown = false; };

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
  }, [canvasRef, isReady, pan, zoom, fit]);

  // Window menu / shortcut event listeners
  useEffect(() => {
    const handleZoomIn = () => { zoom(1.2, 0.5, 0.5); requestDebounced(); };
    const handleZoomOut = () => { zoom(1/1.2, 0.5, 0.5); requestDebounced(); };
    const handleFit = () => {
      const iw = imageWRef.current;
      const ih = imageHRef.current;
      if (iw && ih) fit(iw, ih);
      requestDebounced();
    };
    const handleActualSize = () => {
      zoom(1.0 / getCamera().zoom, 0.5, 0.5);
      requestDebounced();
    };

    window.addEventListener('viewport:zoomIn', handleZoomIn);
    window.addEventListener('viewport:zoomOut', handleZoomOut);
    window.addEventListener('viewport:fit', handleFit);
    window.addEventListener('viewport:actualSize', handleActualSize);

    return () => {
      window.removeEventListener('viewport:zoomIn', handleZoomIn);
      window.removeEventListener('viewport:zoomOut', handleZoomOut);
      window.removeEventListener('viewport:fit', handleFit);
      window.removeEventListener('viewport:actualSize', handleActualSize);
    };
  }, [zoom, fit, getCamera]);

  // Engine event subscriptions
  useEvent('image_loaded', (msg) => {
    fit(msg.width, msg.height);
    requestTiles();
  });
  useEvent('layer_changed', () => requestTiles());
  useEvent('doc_size_changed', () => requestTiles());
  useEvent('mip_level_ready', () => {
    if (!pendingRef.current) { pendingRef.current = true; requestTiles(); }
  });
  useEvent('tiles_complete', () => { pendingRef.current = false; });
  useEvent('tiles_dirty', () => { pendingRef.current = false; requestTiles(true); });

  // Initial tile request when image props arrive
  useEffect(() => {
    const iw = imageWRef.current;
    const ih = imageHRef.current;
    if (!isReady || !iw || !ih) return;
    fit(iw, ih);
    requestTiles();
  }, [isReady, imageW, imageH]);

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

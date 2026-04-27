import { useEffect, useRef } from 'react';
import { useCanvas2DViewport } from '@/components/viewport/ViewportCanvas2D';
import { useActiveTabId, useActiveTab, useTool, useConnected, engine } from '@/engine';
import { useUIStore } from '@/ui/uiStore';

export function Viewport() {
  const tabId = useActiveTabId();
  const activeTab = useActiveTab();
  const activeTool = useTool();
  const connected = useConnected();

  const { canvasRef, isReady, fit, pan, zoom, getCamera, hasAllTiles } = useCanvas2DViewport(tabId);

  const tabIdRef = useRef(tabId);
  tabIdRef.current = tabId;
  const imageWidthRef = useRef(activeTab?.width);
  imageWidthRef.current = activeTab?.width;
  const imageHeightRef = useRef(activeTab?.height);
  imageHeightRef.current = activeTab?.height;

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

  const requestTiles = (force = false) => {
    const id = tabIdRef.current;
    const iw = imageWidthRef.current;
    const ih = imageHeightRef.current;
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
    engine.dispatch({ type: 'request_tiles', tab_id: id, x: panX - mx, y: panY - my, w: w + 2 * mx, h: h + 2 * my, zoom: z });
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
        const iw = imageWidthRef.current;
        const ih = imageHeightRef.current;
        if (iw && ih) fit(iw, ih);
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
  }, [canvasRef, isReady, pan, zoom, fit]);

  // Window menu / shortcut event listeners
  useEffect(() => {
    const handleZoomIn = () => { zoom(1.2, 0.5, 0.5); requestDebounced(); };
    const handleZoomOut = () => { zoom(1/1.2, 0.5, 0.5); requestDebounced(); };
    const handleFit = () => {
      const iw = imageWidthRef.current;
      const ih = imageHeightRef.current;
      if (iw && ih) fit(iw, ih);
      requestDebounced();
    };
    const handleActualSize = () => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      // To center exactly, we could figure out current viewport center, but for now just set zoom to 1
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

  // Engine event subscriptions — stable, uses refs for latest values
  useEffect(() => {
    const unsubs = [
      engine.subscribe('image_loaded', (msg) => {
        if (msg.tab_id !== tabIdRef.current) return;
        console.log(`[ImageLoaded] tab=${msg.tab_id} ${msg.width}x${msg.height} layers=${msg.layer_count}`);
        fit(msg.width, msg.height);
        requestTiles();
      }),
      engine.subscribe('layer_changed', (msg) => {
        if (msg.tab_id !== tabIdRef.current) return;
        console.log(`[LayerChanged] tab=${msg.tab_id} layer=${msg.layer_id} field=${msg.field} sig=${msg.composition_sig}`);
        requestTiles();
      }),
      engine.subscribe('doc_size_changed', (msg) => {
        if (msg.tab_id !== tabIdRef.current) return;
        console.log(`[DocSizeChanged] tab=${msg.tab_id} ${msg.width}x${msg.height}`);
        requestTiles();
      }),
      engine.subscribe('mip_level_ready', (msg) => {
        if (msg.tab_id === tabIdRef.current && !pendingRef.current) {
          pendingRef.current = true;
          requestTiles();
        }
      }),
      engine.subscribe('tiles_complete', () => {
        pendingRef.current = false;
      }),
      engine.subscribe('tiles_dirty', (msg) => {
        if (msg.tab_id === tabIdRef.current) { pendingRef.current = false; requestTiles(true); }
      }),
    ];
    return () => unsubs.forEach(fn => fn());
  }, []);

  // Initial tile request when image props arrive
  useEffect(() => {
    const iw = imageWidthRef.current;
    const ih = imageHeightRef.current;
    if (!isReady || !iw || !ih) return;
    fit(iw, ih);
    requestTiles();
  }, [isReady, activeTab?.width, activeTab?.height]);

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

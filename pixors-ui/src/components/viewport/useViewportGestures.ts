import { useEffect, useRef } from 'react';

interface UseViewportGesturesProps {
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  fit: () => void;
  pan: (dx: number, dy: number) => void;
  zoom: (factor: number, x: number, y: number) => void;
  isReady: boolean;
  onViewportChange: (x: number, y: number, w: number, h: number, zoom: number) => void;
  imageWidth?: number;
  imageHeight?: number;
}

export function useViewportGestures({ canvasRef, fit, pan, zoom, isReady, onViewportChange, imageWidth, imageHeight }: UseViewportGesturesProps) {
  // Track camera state identical to WASM Camera
  const centerRef = useRef({ x: 0.5, y: 0.5 });
  const zoomRef = useRef(1.0);
  const spacePressedRef = useRef(false);

  const onViewportChangeRef = useRef(onViewportChange);
  useEffect(() => {
    onViewportChangeRef.current = onViewportChange;
  }, [onViewportChange]);

  const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Immediate: updates WASM camera. Debounced: sends request_tiles to backend.
  const emitChange = (immediate = false) => {
    if (!canvasRef.current || !imageWidth || !imageHeight) return;
    const rect = canvasRef.current.getBoundingClientRect();
    
    const img_ar = imageWidth / imageHeight;
    const vp_ar = rect.width / rect.height;

    let bw = 1.0;
    let bh = 1.0;
    if (img_ar >= vp_ar) {
      bh = img_ar / vp_ar;
    } else {
      bw = vp_ar / img_ar;
    }

    const scaleX = bw / zoomRef.current;
    const scaleY = bh / zoomRef.current;

    const offsetX = centerRef.current.x - scaleX * 0.5;
    const offsetY = centerRef.current.y - scaleY * 0.5;

    const x = offsetX * imageWidth;
    const y = offsetY * imageHeight;
    const w = scaleX * imageWidth;
    const h = scaleY * imageHeight;

    if (immediate) {
      onViewportChangeRef.current(x, y, w, h, zoomRef.current);
    } else {
      if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = setTimeout(() => {
        onViewportChangeRef.current(x, y, w, h, zoomRef.current);
      }, 120);
    }
  };

  useEffect(() => {
    if (isReady && canvasRef.current && imageWidth && imageHeight) {
      centerRef.current = { x: 0.5, y: 0.5 };
      zoomRef.current = 1.0;
      emitChange(true); // new image: immediate
    }
  }, [imageWidth, imageHeight, isReady]);

  useEffect(() => {
    if (!isReady || !canvasRef.current) return;
    const canvas = canvasRef.current;

    let dragging = false;
    let lastX = 0;
    let lastY = 0;

    const onMouseDown = (e: MouseEvent) => {
      const panGesture = e.button === 1 || ((e.ctrlKey || spacePressedRef.current) && e.button === 0);
      if (!panGesture) return;
      dragging = true;
      lastX = e.clientX;
      lastY = e.clientY;
      e.preventDefault();
    };

    const onMouseMove = (e: MouseEvent) => {
      if (!dragging) return;
      const dx = e.clientX - lastX;
      const dy = e.clientY - lastY;
      pan(dx, dy);
      
      if (imageWidth && imageHeight) {
        const rect = canvas.getBoundingClientRect();
        const img_ar = imageWidth / imageHeight;
        const vp_ar = rect.width / rect.height;
        let bw = 1.0; let bh = 1.0;
        if (img_ar >= vp_ar) { bh = img_ar / vp_ar; } else { bw = vp_ar / img_ar; }
        const scaleX = bw / zoomRef.current;
        const scaleY = bh / zoomRef.current;
        
        centerRef.current.x -= (dx / rect.width) * scaleX;
        centerRef.current.y -= (dy / rect.height) * scaleY;
      }
      
      lastX = e.clientX;
      lastY = e.clientY;
      emitChange(true); // pan: immediate
    };

    const onMouseUp = () => { dragging = false; };
    const onMouseLeave = () => { dragging = false; };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = canvas.getBoundingClientRect();
      const factor = e.deltaY > 0 ? 1.1 : 0.9;
      
      const anchorScreenX = (e.clientX - r.left) / r.width;
      const anchorScreenY = (e.clientY - r.top) / r.height;
      
      zoom(factor, anchorScreenX, anchorScreenY);
      
      if (imageWidth && imageHeight) {
        const img_ar = imageWidth / imageHeight;
        const vp_ar = r.width / r.height;
        let bw = 1.0; let bh = 1.0;
        if (img_ar >= vp_ar) { bh = img_ar / vp_ar; } else { bw = vp_ar / img_ar; }
        
        const scaleX = bw / zoomRef.current;
        const scaleY = bh / zoomRef.current;
        
        const anchorUV_X = (centerRef.current.x - scaleX * 0.5) + anchorScreenX * scaleX;
        const anchorUV_Y = (centerRef.current.y - scaleY * 0.5) + anchorScreenY * scaleY;
        
        let newZoom = zoomRef.current * factor;
        if (newZoom < 0.05) newZoom = 0.05;
        if (newZoom > 100.0) newZoom = 100.0;
        zoomRef.current = newZoom;
        
        const newScaleX = bw / zoomRef.current;
        const newScaleY = bh / zoomRef.current;
        
        const newOffsetX = anchorUV_X - anchorScreenX * newScaleX;
        const newOffsetY = anchorUV_Y - anchorScreenY * newScaleY;
        
        centerRef.current.x = newOffsetX + newScaleX * 0.5;
        centerRef.current.y = newOffsetY + newScaleY * 0.5;
      }
      
      emitChange();
    };

    const onDoubleClick = (e: MouseEvent) => {
      if (e.button !== 1) return;
      fit();
      centerRef.current = { x: 0.5, y: 0.5 };
      zoomRef.current = 1.0;
      emitChange(false); // wheel: debounced — avoids flooding backend per scroll step
    };

    canvas.addEventListener('mousedown', onMouseDown);
    canvas.addEventListener('mousemove', onMouseMove);
    canvas.addEventListener('mouseup', onMouseUp);
    canvas.addEventListener('mouseleave', onMouseLeave);
    canvas.addEventListener('wheel', onWheel, { passive: false });
    canvas.addEventListener('dblclick', onDoubleClick);

    const onWindowKeyDown = (e: KeyboardEvent) => {
      if (e.code === 'Space') {
        spacePressedRef.current = true;
      }
      if (e.key === 'Home') {
        fit();
        centerRef.current = { x: 0.5, y: 0.5 };
        zoomRef.current = 1.0;
        emitChange();
        e.preventDefault();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === '1') {
        const factor = 1 / zoomRef.current;
        zoom(factor, 0.5, 0.5);
        centerRef.current = { x: 0.5, y: 0.5 }; // actually we should simulate zoom_at... but 1.0 means fit?
        zoomRef.current = 1.0;
        emitChange();
        e.preventDefault();
      }
    };

    const onWindowKeyUp = (e: KeyboardEvent) => {
      if (e.code === 'Space') {
        spacePressedRef.current = false;
      }
    };

    window.addEventListener('keydown', onWindowKeyDown);
    window.addEventListener('keyup', onWindowKeyUp);

    return () => {
      canvas.removeEventListener('mousedown', onMouseDown);
      canvas.removeEventListener('mousemove', onMouseMove);
      canvas.removeEventListener('mouseup', onMouseUp);
      canvas.removeEventListener('mouseleave', onMouseLeave);
      canvas.removeEventListener('wheel', onWheel);
      canvas.removeEventListener('dblclick', onDoubleClick);
      window.removeEventListener('keydown', onWindowKeyDown);
      window.removeEventListener('keyup', onWindowKeyUp);
    };
  }, [canvasRef, fit, pan, zoom, isReady, imageWidth, imageHeight]);

  return { emitChange };
}

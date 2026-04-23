import { useEffect, useRef } from 'react';

interface UseViewportGesturesProps {
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  fit: () => void;
  pan: (dx: number, dy: number) => void;
  zoom: (factor: number, x: number, y: number) => void;
  isReady: boolean;
}

export function useViewportGestures({ canvasRef, fit, pan, zoom, isReady }: UseViewportGesturesProps) {
  const currentZoomRef = useRef(1);
  const fitZoomRef = useRef(1);
  const spacePressedRef = useRef(false);

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
      pan(e.clientX - lastX, e.clientY - lastY);
      lastX = e.clientX;
      lastY = e.clientY;
    };

    const onMouseUp = () => { dragging = false; };
    const onMouseLeave = () => { dragging = false; };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = canvas.getBoundingClientRect();
      const factor = e.deltaY > 0 ? 1.1 : 0.9;
      zoom(
        factor,
        (e.clientX - r.left) / r.width,
        (e.clientY - r.top) / r.height
      );
      currentZoomRef.current *= factor;
      if (currentZoomRef.current <= 0) currentZoomRef.current = 0.0001;
    };

    const onDoubleClick = (e: MouseEvent) => {
      if (e.button !== 1) return;
      fit();
      currentZoomRef.current = fitZoomRef.current;
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
        currentZoomRef.current = fitZoomRef.current;
        e.preventDefault();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === '1') {
        const factor = 1 / currentZoomRef.current;
        zoom(factor, 0.5, 0.5);
        currentZoomRef.current = 1;
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
  }, [canvasRef, fit, pan, zoom, isReady]);

  return { fitZoomRef, currentZoomRef };
}

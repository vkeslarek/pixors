import { useEffect, useState } from 'react';

interface LoadingOverlayProps {
  percent: number;
}

export function LoadingOverlay({ percent }: LoadingOverlayProps) {
  const [show, setShow] = useState(true);

  useEffect(() => {
    if (percent >= 100) {
      const timer = setTimeout(() => setShow(false), 500);
      return () => clearTimeout(timer);
    }
    setShow(true);
  }, [percent]);

  if (!show) return null;

  return (
    <div
      style={{
        position: 'absolute',
        top: 0,
        left: 0,
        right: 0,
        zIndex: 50,
        background: 'rgba(0, 0, 0, 0.7)',
        padding: '12px 16px',
        display: 'flex',
        flexDirection: 'column',
        gap: '8px',
        opacity: percent >= 100 ? 0 : 1,
        transition: 'opacity 0.5s ease-out',
      }}
    >
      <div style={{ fontSize: '12px', color: '#fff', fontFamily: 'system-ui' }}>
        Carregando... {percent}%
      </div>
      <div
        style={{
          height: '2px',
          background: 'rgba(255, 255, 255, 0.2)',
          borderRadius: '1px',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            height: '100%',
            background: 'linear-gradient(90deg, #4ade80, #22c55e)',
            width: `${percent}%`,
            transition: 'width 0.15s ease-out',
          }}
        />
      </div>
    </div>
  );
}

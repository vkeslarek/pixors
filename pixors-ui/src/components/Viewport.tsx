import { useEffect, useRef, useState } from 'react'
import init, { PixorsViewport } from 'pixors-viewport'

const ENGINE_BASE = 'http://127.0.0.1:8080'
const ENGINE_WS   = 'ws://127.0.0.1:8080'

interface ViewportProps {
  activeTool: string
  zoom: number
  onMouseMove: (x: number, y: number) => void
}

export function Viewport({ activeTool, zoom, onMouseMove }: ViewportProps) {
  const canvasRef    = useRef<HTMLCanvasElement>(null)
  const viewportRef  = useRef<PixorsViewport | null>(null)
  const wsRef        = useRef<WebSocket | null>(null)

  // State for each pending tile: after receiving tile_data JSON we wait for the binary
  const pendingTileRef = useRef<{ x: number; y: number; width: number; height: number } | null>(null)

  const [gpuError,   setGpuError]   = useState<string | null>(null)
  const [connected,  setConnected]  = useState(false)

  useEffect(() => {
    let cancelled = false
    let rafId: number | null = null
    let resizeObserver: ResizeObserver | null = null

    const boot = async () => {
      try {
        // ── 1. Init WASM ─────────────────────────────────────────────
        await init()
        if (cancelled || !canvasRef.current) return

        const canvas = canvasRef.current
        const viewport = await PixorsViewport.create('main-viewport')
        if (cancelled) { viewport.free(); return }
        viewportRef.current = viewport

        // ── 2. 60 fps render loop ────────────────────────────────────
        let lastTime = 0
        const renderLoop = (ts: number) => {
          if (cancelled) return
          if (ts - lastTime >= 1000 / 60) {
            try { viewportRef.current?.render() } catch (e) { console.error('Render error:', e) }
            lastTime = ts
          }
          rafId = requestAnimationFrame(renderLoop)
        }
        rafId = requestAnimationFrame(renderLoop)

        // ── 3. Pan / zoom via native handlers ────────────────────────
        let dragging = false, lastX = 0, lastY = 0
        canvas.onmousedown  = (e) => { dragging = true; lastX = e.clientX; lastY = e.clientY }
        canvas.onmousemove  = (e) => {
          if (!dragging || !viewportRef.current) return
          viewportRef.current.pan(e.clientX - lastX, e.clientY - lastY)
          lastX = e.clientX; lastY = e.clientY
        }
        canvas.onmouseup    = () => { dragging = false }
        canvas.onmouseleave = () => { dragging = false }
        canvas.onwheel      = (e) => {
          e.preventDefault()
          if (!viewportRef.current) return
          const r = canvas.getBoundingClientRect()
          viewportRef.current.zoom(
            e.deltaY > 0 ? 1.1 : 0.9,
            (e.clientX - r.left) / r.width,
            (e.clientY - r.top)  / r.height,
          )
        }

        // ── 4. Resize observer ────────────────────────────────────────
        let resizeTimeout: ReturnType<typeof setTimeout>
        resizeObserver = new ResizeObserver((entries) => {
          clearTimeout(resizeTimeout)
          resizeTimeout = setTimeout(() => {
            if (!viewportRef.current || !canvasRef.current) return
            const entry = entries[0]
            if (!entry) return
            const w = Math.max(1, Math.floor(entry.contentRect.width))
            const h = Math.max(1, Math.floor(entry.contentRect.height))
            if (canvasRef.current.width !== w || canvasRef.current.height !== h) {
              canvasRef.current.width  = w
              canvasRef.current.height = h
              viewportRef.current.resize(w, h)
            }
          }, 100)
        })
        const { width, height } = canvas.getBoundingClientRect()
        const w = Math.max(1, Math.floor(width))
        const h = Math.max(1, Math.floor(height))
        canvas.width  = w
        canvas.height = h
        viewport.resize(w, h)  // sync WASM viewport to actual canvas size
        resizeObserver.observe(canvas)

        // ── 5. Create session via REST ────────────────────────────────
        const sessionRes = await fetch(`${ENGINE_BASE}/api/session`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({}),
        })
        if (!sessionRes.ok || cancelled) return
        const { session_id } = await sessionRes.json()
        console.log('Session created:', session_id)

        // ── 6. Open WebSocket with session_id ─────────────────────────
        const ws = new WebSocket(`${ENGINE_WS}/ws?session_id=${session_id}`)
        ws.binaryType = 'arraybuffer'
        wsRef.current = ws

        ws.onopen = () => {
          if (cancelled) { ws.close(); return }
          console.log('Connected to engine')
          setConnected(true)
          // Send load_image command after brief delay
          setTimeout(() => {
            if (ws.readyState === WebSocket.OPEN) {
              ws.send(JSON.stringify({ type: 'load_image', path: 'example1.png' }))
            }
          }, 200)
        }

        ws.onmessage = (event) => {
          if (typeof event.data === 'string') {
            const msg = JSON.parse(event.data)
            if (msg.type === 'image_loaded') {
              // Create the full-size empty texture first, then tiles fill it
              console.log(`Image loaded: ${msg.width}x${msg.height}`)
              viewportRef.current?.create_empty_texture(msg.width, msg.height)
            } else if (msg.type === 'tile_data') {
              // Next binary message is this tile's pixel data
              pendingTileRef.current = { x: msg.x, y: msg.y, width: msg.width, height: msg.height }
            } else if (msg.type === 'error') {
              console.error('Engine error:', msg.message)
            }
          } else if (event.data instanceof ArrayBuffer) {
            // Binary tile pixel data → write into the existing texture at (x, y)
            const tile = pendingTileRef.current
            if (!tile || !viewportRef.current) return
            const data = new Uint8Array(event.data)
            viewportRef.current.write_tile(tile.x, tile.y, tile.width, tile.height, data)
            pendingTileRef.current = null
          }
        }

        ws.onerror = (e) => console.error('WebSocket error:', e)
        ws.onclose = () => { setConnected(false); console.log('WS closed') }

      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        console.error('Failed to initialize viewport:', msg)
        setGpuError(msg)
      }
    }

    boot()

    return () => {
      cancelled = true
      if (rafId !== null) cancelAnimationFrame(rafId)
      resizeObserver?.disconnect()
      wsRef.current?.close()
      wsRef.current = null
      viewportRef.current?.free()
      viewportRef.current = null
    }
  }, [])

  const handleMouseMove = (e: React.MouseEvent) => {
    const r = e.currentTarget.getBoundingClientRect()
    onMouseMove(Math.round(e.clientX - r.left), Math.round(e.clientY - r.top))
  }

  return (
    <div className={`canvas-area tool-${activeTool}`} onMouseMove={handleMouseMove}>
      {gpuError ? (
        <div className="gpu-error">
          <span style={{ fontSize: 32 }}>⚠️</span>
          <strong>WebGPU unavailable</strong>
          <span style={{ fontSize: 12, color: 'var(--text-muted)', textAlign: 'center' }}>{gpuError}</span>
          <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Enable hardware acceleration in your browser settings.</span>
        </div>
      ) : (
        <canvas id="main-viewport" ref={canvasRef} className="viewport-canvas" />
      )}

      <div className="zoom-indicator">{zoom}%</div>

      {/* Engine connection indicator */}
      <div
        title={connected ? 'Engine connected' : 'Engine disconnected'}
        style={{
          position: 'absolute', top: 8, right: 8,
          width: 8, height: 8, borderRadius: '50%',
          background: connected ? 'oklch(0.72 0.15 145)' : 'var(--text-muted)',
          boxShadow: connected ? '0 0 6px oklch(0.72 0.15 145 / 0.7)' : 'none',
          transition: 'all 0.3s',
        }}
      />
    </div>
  )
}

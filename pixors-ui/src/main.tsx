import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@/tokens.css'
import App from '@/App'

// engine.boot() // WebSocket legado — desabilitado durante migração WASM

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)

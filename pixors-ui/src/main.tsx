import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { engine } from '@/engine/engine'
import '@/tokens.css'
import App from '@/App'

engine.boot()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)

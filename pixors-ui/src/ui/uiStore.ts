import { create } from 'zustand'
import type { Layer, Adjustment } from '@/types'

const INIT_LAYERS: Layer[] = [
  { id: '1', name: 'Background',    type: 'image',      visible: true, locked: false, opacity: 100, blendMode: 'Normal',  color: '#888' },
  { id: '2', name: 'Portrait',      type: 'image',      visible: true, locked: false, opacity: 100, blendMode: 'Normal',  color: '#6fc' },
  { id: '3', name: 'Texture',       type: 'image',      visible: true, locked: false, opacity: 50,  blendMode: 'Overlay', color: '#fc6' },
  { id: '4', name: 'Curves 1',      type: 'adjustment', visible: true, locked: false, opacity: 100, blendMode: 'Normal',  color: '#6cf' },
  { id: '5', name: 'Color Overlay', type: 'adjustment', visible: true, locked: false, opacity: 100, blendMode: 'Color',   color: '#c6f' },
]

const INIT_ADJ: Adjustment[] = [
  { id: 'exposure',  label: 'Exposure',    min: -5,   max: 5,   step: 0.01, value: 0 },
  { id: 'contrast',  label: 'Contrast',    min: -100, max: 100, step: 1,    value: 0 },
  { id: 'highlights',label: 'Highlights',  min: -100, max: 100, step: 1,    value: 0 },
  { id: 'shadows',   label: 'Shadows',     min: -100, max: 100, step: 1,    value: 0 },
  { id: 'temp',      label: 'Temperature', min: -100, max: 100, step: 1,    value: 0 },
  { id: 'vibrance',  label: 'Vibrance',    min: -100, max: 100, step: 1,    value: 0 },
  { id: 'saturation',label: 'Saturation',  min: -100, max: 100, step: 1,    value: 0 },
  { id: 'clarity',   label: 'Clarity',     min: -100, max: 100, step: 1,    value: 0 },
  { id: 'sharpening',label: 'Sharpening',  min: 0,    max: 150, step: 1,    value: 40 },
]

interface UIState {
  workspace: string
  setWorkspace: (w: string) => void
  mousePos: { x: number; y: number }
  setMousePos: (p: { x: number; y: number }) => void
  layers: Layer[]
  activeLayerId: string
  adjustments: Adjustment[]
  panelsOpen: { hist: boolean; props: boolean; adj: boolean; layers: boolean }
  toggleVisibility: (id: string) => void
  toggleLock: (id: string) => void
  deleteLayer: (id: string) => void
  addLayer: () => void
  duplicateLayer: () => void
  changeBlend: (id: string, mode: string) => void
  changeOpacity: (id: string, v: number) => void
  setActiveLayerId: (id: string) => void
  changeAdj: (id: string, v: number) => void
  resetAdj: () => void
  togglePanel: (key: keyof UIState['panelsOpen']) => void
}

export const useUIStore = create<UIState>((set) => ({
  workspace: 'editor',
  setWorkspace: (w) => set({ workspace: w }),
  mousePos: { x: 0, y: 0 },
  setMousePos: (p) => set({ mousePos: p }),

  layers: INIT_LAYERS,
  activeLayerId: '4',
  adjustments: INIT_ADJ,
  panelsOpen: { hist: true, props: true, adj: true, layers: true },

  toggleVisibility: (id) => set(s => ({
    layers: s.layers.map(l => l.id === id ? { ...l, visible: !l.visible } : l),
  })),
  toggleLock: (id) => set(s => ({
    layers: s.layers.map(l => l.id === id ? { ...l, locked: !l.locked } : l),
  })),
  deleteLayer: (id) => set(s => ({
    layers: s.layers.filter(l => l.id !== id),
    activeLayerId: s.activeLayerId === id
      ? (s.layers.filter(l => l.id !== id).find(() => true)?.id ?? '')
      : s.activeLayerId,
  })),
  addLayer: () => set(s => {
    const nl: Layer = { id: Date.now().toString(), name: 'New Layer', type: 'image', visible: true, locked: false, opacity: 100, blendMode: 'Normal', color: '#ccc' }
    return { layers: [nl, ...s.layers], activeLayerId: nl.id }
  }),
  duplicateLayer: () => set(s => {
    const src = s.layers.find(l => l.id === s.activeLayerId)
    if (!src) return {}
    const dup: Layer = { ...src, id: Date.now().toString(), name: src.name + ' copy' }
    return { layers: [dup, ...s.layers], activeLayerId: dup.id }
  }),
  changeBlend: (id, mode) => set(s => ({
    layers: s.layers.map(l => l.id === id ? { ...l, blendMode: mode } : l),
  })),
  changeOpacity: (id, v) => set(s => ({
    layers: s.layers.map(l => l.id === id ? { ...l, opacity: v } : l),
  })),
  setActiveLayerId: (id) => set({ activeLayerId: id }),
  changeAdj: (id, v) => set(s => ({
    adjustments: s.adjustments.map(a => a.id === id ? { ...a, value: v } : a),
  })),
  resetAdj: () => set({ adjustments: INIT_ADJ }),
  togglePanel: (key) => set(s => ({
    panelsOpen: { ...s.panelsOpen, [key]: !s.panelsOpen[key] },
  })),
}))

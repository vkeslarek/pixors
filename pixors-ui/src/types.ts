// Shared types for the application
export interface Layer {
  id: string
  name: string
  type: 'image' | 'adjustment'
  visible: boolean
  locked: boolean
  opacity: number
  blendMode: string
  color: string
}

export interface Adjustment {
  id: string
  label: string
  min: number
  max: number
  step: number
  value: number
}

export interface Tab {
  id: string
  name: string
  color: string
  modified: boolean
}

export interface MousePos {
  x: number
  y: number
}

import { create } from 'zustand'
import type { EngineEvent, UITab } from '@/engine/types'

const TAB_COLORS = ['#ff4d4d', '#4dff4d', '#4d4dff', '#ffff4d', '#ff4dff', '#6ffff']

export interface EngineState {
  connected: boolean
  sessionId: string
  tabs: UITab[]
  activeTabId: string | null
  tool: string
  viewport: Record<string, { zoom: number; panX: number; panY: number }>
  loading: Record<string, { active: boolean; percent: number }>
  lastError: string | null
}

export const useEngineStore = create<EngineState>(() => ({
  connected: false,
  sessionId: '',
  tabs: [],
  activeTabId: null,
  tool: 'Select',
  viewport: {},
  loading: {},
  lastError: null,
}))

export function applyEvent(prev: EngineState, ev: EngineEvent): Partial<EngineState> {
  switch (ev.type) {
    case 'session_state':
      return {
        sessionId: ev.session_id,
        activeTabId: ev.active_tab_id,
        tabs: ev.tabs.map((td, i) => ({
          id: td.id,
          name: td.name,
          color: TAB_COLORS[i % TAB_COLORS.length],
          modified: false,
          hasImage: td.has_image,
          width: td.width,
          height: td.height,
        })),
      }
    case 'tab_created': {
      if (prev.tabs.find(t => t.id === ev.tab_id)) return {}
      return {
        tabs: [...prev.tabs, {
          id: ev.tab_id, name: ev.name,
          color: TAB_COLORS[prev.tabs.length % TAB_COLORS.length],
          modified: false, hasImage: false, width: 0, height: 0,
        }],
      }
    }
    case 'tab_closed': {
      const tabs = prev.tabs.filter(t => t.id !== ev.tab_id)
      const activeTabId = prev.activeTabId === ev.tab_id
        ? (tabs.at(-1)?.id ?? null)
        : prev.activeTabId
      return { tabs, activeTabId }
    }
    case 'tab_activated':
      return { activeTabId: ev.tab_id }
    case 'image_loaded': {
      const exists = prev.tabs.find(t => t.id === ev.tab_id)
      const tabs = exists
        ? prev.tabs.map(t => t.id === ev.tab_id ? { ...t, hasImage: true, width: ev.width, height: ev.height } : t)
        : [...prev.tabs, {
            id: ev.tab_id, name: `Image ${ev.width}x${ev.height}`,
            color: TAB_COLORS[prev.tabs.length % TAB_COLORS.length],
            modified: false, hasImage: true, width: ev.width, height: ev.height,
          }]
      return {
        tabs,
        loading: { ...prev.loading, [ev.tab_id]: { active: false, percent: 100 } },
      }
    }
    case 'image_load_progress':
      return {
        loading: { ...prev.loading, [ev.tab_id]: { active: true, percent: ev.percent } },
      }
    case 'image_closed':
      return {
        loading: { ...prev.loading, [ev.tab_id]: { active: false, percent: 0 } },
      }
    case 'tool_changed':
      return { tool: ev.tool }
    case 'viewport_updated':
      return {
        viewport: {
          ...prev.viewport,
          [ev.tab_id]: { zoom: ev.zoom, panX: ev.pan_x, panY: ev.pan_y },
        },
      }
    case 'error':
      return { lastError: ev.message }
    default:
      return {}
  }
}

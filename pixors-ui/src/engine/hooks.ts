import { useEngineStore } from '@/engine/store'
import { engine } from '@/engine/engine'

export const useConnected = () => useEngineStore(s => s.connected)
export const useTabs = () => useEngineStore(s => s.tabs)
export const useActiveTabId = () => useEngineStore(s => s.activeTabId)
export const useActiveTab = () => useEngineStore(s => {
  const id = s.activeTabId
  return id ? s.tabs.find(t => t.id === id) ?? null : null
})
export const useTool = () => useEngineStore(s => s.tool)

const EMPTY_LOADING = { active: false, percent: 0 } as const
export const useLoadingFor = (tabId: string | null) =>
  useEngineStore(s => tabId ? s.loading[tabId] ?? EMPTY_LOADING : EMPTY_LOADING)

export const useViewportFor = (tabId: string | null) =>
  useEngineStore(s => tabId ? s.viewport[tabId] ?? null : null)

export { engine }
export { MSG_EVENT, MSG_TILE, MSG_TILES_COMPLETE } from '@/engine/client'

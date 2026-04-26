import { describe, it, expect } from 'vitest'
import { applyEvent, type EngineState } from '@/engine/store'
import type { EngineEvent } from '@/engine/types'

function makeState(overrides?: Partial<EngineState>): EngineState {
  return {
    connected: false,
    sessionId: '',
    tabs: [],
    activeTabId: null,
    tool: 'Select',
    viewport: {},
    loading: {},
    lastError: null,
    ...overrides,
  }
}

const T = (id: string, name: string, w = 0, h = 0, hasImage = false) => ({
  id, name, color: '#ff4d4d', modified: false, hasImage, width: w, height: h,
})

describe('applyEvent reducer', () => {
  it('session_state replaces tabs and activeTabId', () => {
    const prev = makeState()
    const ev: EngineEvent = {
      type: 'session_state',
      session_id: 's1',
      active_tab_id: 't1',
      status: 'Connected',
      tabs: [{ id: 't1', name: 'tab1', created_at: 0, has_image: true, width: 100, height: 200 }],
    }
    const next = applyEvent(prev, ev)
    expect(next.sessionId).toBe('s1')
    expect(next.activeTabId).toBe('t1')
    expect(next.tabs).toHaveLength(1)
    expect(next.tabs![0]).toMatchObject({ id: 't1', name: 'tab1', hasImage: true, width: 100, height: 200 })
  })

  it('tab_created adds a new tab', () => {
    const prev = makeState()
    const ev: EngineEvent = { type: 'tab_created', tab_id: 't1', name: 'New' }
    const next = applyEvent(prev, ev)
    expect(next.tabs).toHaveLength(1)
    expect(next.tabs![0]).toMatchObject({ id: 't1', name: 'New', hasImage: false, width: 0, height: 0 })
  })

  it('tab_created is idempotent', () => {
    const prev = makeState({ tabs: [T('t1', 'Already')] })
    const ev: EngineEvent = { type: 'tab_created', tab_id: 't1', name: 'Dup' }
    const next = applyEvent(prev, ev)
    expect(next.tabs).toBeUndefined() // no change
  })

  it('tab_closed removes tab and falls back active', () => {
    const prev = makeState({
      tabs: [T('t1', 'a'), T('t2', 'b')],
      activeTabId: 't2',
    })
    const ev: EngineEvent = { type: 'tab_closed', tab_id: 't2' }
    const next = applyEvent(prev, ev)
    expect(next.tabs).toHaveLength(1)
    expect(next.activeTabId).toBe('t1')
  })

  it('tab_closed without fallback', () => {
    const prev = makeState({
      tabs: [T('t1', 'a')],
      activeTabId: 't1',
    })
    const ev: EngineEvent = { type: 'tab_closed', tab_id: 't1' }
    const next = applyEvent(prev, ev)
    expect(next.tabs).toHaveLength(0)
    expect(next.activeTabId).toBeNull()
  })

  it('tab_activated sets active id', () => {
    const prev = makeState()
    const ev: EngineEvent = { type: 'tab_activated', tab_id: 't2' }
    const next = applyEvent(prev, ev)
    expect(next.activeTabId).toBe('t2')
  })

  it('image_loaded updates existing tab', () => {
    const prev = makeState({ tabs: [T('t1', 'a')] })
    const ev: EngineEvent = { type: 'image_loaded', tab_id: 't1', width: 400, height: 300, format: 'rgba8', layer_count: 1 }
    const next = applyEvent(prev, ev)
    expect(next.tabs![0]).toMatchObject({ hasImage: true, width: 400, height: 300 })
    expect(next.loading!['t1']).toEqual({ active: false, percent: 100 })
  })

  it('image_loaded creates tab if unknown (out-of-order)', () => {
    const prev = makeState()
    const ev: EngineEvent = { type: 'image_loaded', tab_id: 't99', width: 100, height: 100, format: 'rgba8', layer_count: 1 }
    const next = applyEvent(prev, ev)
    expect(next.tabs).toHaveLength(1)
    expect(next.tabs![0]).toMatchObject({ id: 't99', hasImage: true, width: 100, height: 100 })
  })

  it('image_load_progress sets loading state', () => {
    const prev = makeState()
    const ev: EngineEvent = { type: 'image_load_progress', tab_id: 't1', percent: 42 }
    const next = applyEvent(prev, ev)
    expect(next.loading!['t1']).toEqual({ active: true, percent: 42 })
  })

  it('image_closed resets loading', () => {
    const prev = makeState({ loading: { t1: { active: true, percent: 50 } } })
    const ev: EngineEvent = { type: 'image_closed', tab_id: 't1' }
    const next = applyEvent(prev, ev)
    expect(next.loading!['t1']).toEqual({ active: false, percent: 0 })
  })

  it('tool_changed updates tool', () => {
    const prev = makeState()
    const ev: EngineEvent = { type: 'tool_changed', tool: 'brush' }
    const next = applyEvent(prev, ev)
    expect(next.tool).toBe('brush')
  })

  it('viewport_updated stores per-tab state', () => {
    const prev = makeState()
    const ev: EngineEvent = { type: 'viewport_updated', tab_id: 't1', zoom: 2.5, pan_x: 100, pan_y: 200 }
    const next = applyEvent(prev, ev)
    expect(next.viewport!['t1']).toEqual({ zoom: 2.5, panX: 100, panY: 200 })
  })

  it('viewport_updated preserves other tabs', () => {
    const prev = makeState({ viewport: { t0: { zoom: 1, panX: 0, panY: 0 } } })
    const ev: EngineEvent = { type: 'viewport_updated', tab_id: 't1', zoom: 3, pan_x: 10, pan_y: 20 }
    const next = applyEvent(prev, ev)
    expect(next.viewport!['t0']).toEqual({ zoom: 1, panX: 0, panY: 0 })
    expect(next.viewport!['t1']).toEqual({ zoom: 3, panX: 10, panY: 20 })
  })

  it('viewport_updated accepts unknown tab (out-of-order)', () => {
    const prev = makeState()
    const ev: EngineEvent = { type: 'viewport_updated', tab_id: 'unknown', zoom: 1, pan_x: 0, pan_y: 0 }
    const next = applyEvent(prev, ev)
    expect(next.viewport!['unknown']).toBeDefined()
  })

  it('error sets lastError', () => {
    const prev = makeState()
    const ev: EngineEvent = { type: 'error', message: 'boom' }
    const next = applyEvent(prev, ev)
    expect(next.lastError).toBe('boom')
  })

  it('tab_activated before tab_created is accepted (out-of-order)', () => {
    const prev = makeState()
    const ev: EngineEvent = { type: 'tab_activated', tab_id: 'phantom' }
    const next = applyEvent(prev, ev)
    expect(next.activeTabId).toBe('phantom')
    // tabs untouched — store still returns only the changed slice
    expect(next.tabs).toBeUndefined()
  })

  it('unknown event type returns empty partial', () => {
    const prev = makeState()
    const next = applyEvent(prev, { type: 'tiles_complete' } as EngineEvent)
    expect(next).toEqual({})
  })
})

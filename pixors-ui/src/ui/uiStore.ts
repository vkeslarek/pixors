import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { PanelLayout, PanelId, DockSide, PanelState } from './panelLayout'
import { DEFAULT_LAYOUT, TOOLBAR_WIDTH } from './panelLayout'

export type DropTarget =
  | { kind: 'into-column'; columnId: string; rect: DOMRect }
  | { kind: 'before-column'; columnId: string; rect: DOMRect }
  | { kind: 'after-column'; columnId: string; rect: DOMRect }
  | { kind: 'new-column-in-area'; side: DockSide; rect: DOMRect };

interface UIState {
  workspace: string
  setWorkspace: (w: string) => void
  mousePos: { x: number; y: number }
  setMousePos: (p: { x: number; y: number }) => void

  // Panel layout
  panelLayout: PanelLayout
  setLayout: (layout: PanelLayout) => void
  resizeColumn: (columnId: string, delta: number) => void
  movePanel: (panelId: PanelId, target: DropTarget) => void
  togglePanelVisibility: (id: PanelId) => void

  // Drag state
  draggingPanel: PanelId | null
  setDraggingPanel: (id: PanelId | null) => void
  dropTarget: DropTarget | null
  setDropTarget: (target: DropTarget | null) => void
}

export const useUIStore = create<UIState>()(
  persist(
    (set) => ({
      workspace: 'editor',
      setWorkspace: (w) => set({ workspace: w }),
      mousePos: { x: 0, y: 0 },
      setMousePos: (p) => set({ mousePos: p }),

      draggingPanel: null,
      setDraggingPanel: (id) => set({ draggingPanel: id }),
      dropTarget: null,
      setDropTarget: (target) => set({ dropTarget: target }),

      panelLayout: DEFAULT_LAYOUT,
      setLayout: (layout) => set({ panelLayout: layout }),

      resizeColumn: (columnId, newSize) => set((s) => ({
        panelLayout: {
          ...s.panelLayout,
          columns: s.panelLayout.columns.map(c =>
            c.id === columnId ? { ...c, size: Math.max(78, newSize) } : c
          ),
        },
      })),

      movePanel: (panelId, target) => set((s) => {
        const layout = s.panelLayout;
        const panel = layout.panels[panelId];
        if (!panel) return { panelLayout: layout };

        let newColumnId: string | null = null;
        let newOrder = 0;
        let newColumns = [...layout.columns];

        const sizeFor = (side: DockSide) =>
          panelId === 'toolbar' ? TOOLBAR_WIDTH : (side === 'bottom' ? 200 : 280);

        if (target.kind === 'into-column') {
          newColumnId = target.columnId;
          const existing = Object.values(layout.panels).filter(p => p.columnId === target.columnId && p.id !== panelId);
          newOrder = existing.length;
        } else if (target.kind === 'before-column' || target.kind === 'after-column') {
          const col = layout.columns.find(c => c.id === target.columnId);
          if (!col) return { panelLayout: layout };
          const newColId = `${col.side}-${Date.now()}`;
          newColumnId = newColId;
          const idx = layout.columns.findIndex(c => c.id === target.columnId);
          const insertIdx = target.kind === 'before-column' ? idx : idx + 1;
          newColumns.splice(insertIdx, 0, { id: newColId, side: col.side, size: sizeFor(col.side) });
        } else if (target.kind === 'new-column-in-area') {
          const newColId = `${target.side}-${Date.now()}`;
          newColumnId = newColId;
          newColumns.push({ id: newColId, side: target.side, size: sizeFor(target.side) });
        }

        if (!newColumnId) return { panelLayout: layout };

        const newPanels: Record<PanelId, PanelState> = {} as Record<PanelId, PanelState>;
        (Object.entries(layout.panels) as [PanelId, PanelState][]).forEach(([id, p]) => {
          if (id === panelId) {
            newPanels[id] = { ...p, columnId: newColumnId!, order: newOrder, lastColumnId: newColumnId! };
          } else if (target.kind === 'into-column' && p.columnId === target.columnId && p.order >= newOrder) {
            newPanels[id] = { ...p, order: p.order + 1 };
          } else {
            newPanels[id] = p;
          }
        });

        // GC empty columns
        const usedColIds = new Set(
          Object.values(newPanels).map(p => p.columnId).filter(Boolean)
        );
        newColumns = newColumns.filter(c => usedColIds.has(c.id));

        return { panelLayout: { ...layout, columns: newColumns, panels: newPanels } };
      }),

      togglePanelVisibility: (id) => set((s) => {
        const p = s.panelLayout.panels[id];
        if (p.columnId === null) {
          // SHOW: restore to last column (create one if it was GC'd)
          const colExists = s.panelLayout.columns.some(c => c.id === p.lastColumnId);
          const colId = colExists ? p.lastColumnId : `right-${Date.now()}`;
          const newColumns = colExists
            ? s.panelLayout.columns
            : [...s.panelLayout.columns, { id: colId, side: 'right' as DockSide, size: 280 }];
          return {
            panelLayout: {
              ...s.panelLayout,
              columns: newColumns,
              panels: { ...s.panelLayout.panels, [id]: { ...p, columnId: colId, lastColumnId: colId } },
            },
          };
        }
        // HIDE: set columnId to null, GC empty column
        return {
          panelLayout: {
            ...s.panelLayout,
            panels: { ...s.panelLayout.panels, [id]: { ...p, lastColumnId: p.columnId, columnId: null } },
            columns: s.panelLayout.columns.filter(c =>
              c.id !== p.columnId || Object.values(s.panelLayout.panels).some(
                other => other.id !== id && other.columnId === c.id
              )
            ),
          },
        };
      }),
    }),
    {
      name: 'pixors.panelLayout.v7',
      partialize: (state) => ({ panelLayout: state.panelLayout }),
      onRehydrateStorage: () => (state) => {
        if (state && state.panelLayout?.version !== 7) {
          state.setLayout(DEFAULT_LAYOUT);
        }
      },
    }
  )
);

export type PanelId = 'toolbar' | 'layers' | 'filters';
export type DockSide = 'left' | 'right' | 'bottom';

export interface DockColumn {
  id: string;
  side: DockSide;
  size: number;
}

export interface PanelState {
  id: PanelId;
  columnId: string | null;
  order: number;
  lastColumnId: string;
}

export interface PanelLayout {
  version: 7;
  columns: DockColumn[];
  panels: Record<PanelId, PanelState>;
}

export const TOOLBAR_WIDTH = 78;

export const DEFAULT_LAYOUT: PanelLayout = {
  version: 7,
  columns: [
    { id: 'left-0', side: 'left', size: TOOLBAR_WIDTH },
    { id: 'right-0', side: 'right', size: 280 },
  ],
  panels: {
    toolbar: { id: 'toolbar', columnId: 'left-0', order: 0, lastColumnId: 'left-0' },
    layers: { id: 'layers', columnId: 'right-0', order: 0, lastColumnId: 'right-0' },
    filters: { id: 'filters', columnId: 'right-0', order: 1, lastColumnId: 'right-0' },
  },
};

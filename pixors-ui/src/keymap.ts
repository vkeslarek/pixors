import { engine } from '@/engine';

export type ShortcutAction = (activeTabId: string | null) => void;

export interface ShortcutDef {
  label: string;
  shortcut: string;
  action: ShortcutAction;
  requiresTab: boolean;
}

export const SHORTCUTS: Record<string, ShortcutDef> = {
  openFile: { label: 'Open...', shortcut: 'Ctrl+O', action: () => engine.dispatch({ type: 'open_file_dialog' }), requiresTab: false },
  closeTab: { label: 'Close Tab', shortcut: 'Ctrl+W', action: (tabId) => tabId && engine.dispatch({ type: 'close_tab', tab_id: tabId }), requiresTab: true },
  zoomIn: { label: 'Zoom In', shortcut: 'Ctrl+=', action: () => window.dispatchEvent(new CustomEvent('viewport:zoomIn')), requiresTab: true },
  zoomOut: { label: 'Zoom Out', shortcut: 'Ctrl+-', action: () => window.dispatchEvent(new CustomEvent('viewport:zoomOut')), requiresTab: true },
  fitToScreen: { label: 'Fit to Screen', shortcut: 'Ctrl+0', action: () => window.dispatchEvent(new CustomEvent('viewport:fit')), requiresTab: true },
  actualSize: { label: 'Actual Size', shortcut: 'Ctrl+1', action: () => window.dispatchEvent(new CustomEvent('viewport:actualSize')), requiresTab: true },
};

export function registerShortcuts(activeTabId: string | null) {
  const toolMap: Record<string, string> = {
    v:'move', m:'select', l:'lasso', w:'wand', c:'crop', i:'eyedropper',
    b:'brush', e:'eraser', j:'heal', g:'gradient', t:'text', u:'shape', h:'hand', z:'zoom',
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;

    // Check tools (single keys without modifiers)
    if (!e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
      const tool = toolMap[e.key.toLowerCase()];
      if (tool) { 
        engine.dispatch({ type: 'select_tool', tool }); 
        e.preventDefault(); 
        return;
      }
    }

    // Check shortcuts
    for (const def of Object.values(SHORTCUTS)) {
      if (!def.shortcut) continue;
      
      const parts = def.shortcut.split('+');
      const key = parts[parts.length - 1].toLowerCase();
      const needsCtrl = parts.includes('Ctrl');
      
      // Map shortcut keys to event keys
      let matchKey = false;
      if (key === '=') matchKey = (e.key === '=' || e.key === '+');
      else if (key === '-') matchKey = (e.key === '-');
      else if (key === '0') matchKey = (e.key === '0');
      else if (key === '1') matchKey = (e.key === '1');
      else matchKey = (e.key.toLowerCase() === key);

      if (matchKey && (needsCtrl === e.ctrlKey || needsCtrl === e.metaKey)) {
        e.preventDefault();
        if (!def.requiresTab || activeTabId) {
          def.action(activeTabId);
        }
        return;
      }
    }
  };

  window.addEventListener('keydown', onKey);
  return () => window.removeEventListener('keydown', onKey);
}

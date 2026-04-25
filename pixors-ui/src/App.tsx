import { useEffect } from 'react'
import * as Tooltip from '@radix-ui/react-tooltip'
import { MenuBar, TabBar } from '@/components/MenuBar'
import { ActivityBar } from '@/components/ActivityBar'
import { Toolbar } from '@/components/Toolbar'
import { Viewport } from '@/components/Viewport'
import { Sidebar } from '@/components/Sidebar'
import { StatusBar } from '@/components/StatusBar'
import { ProgressBar } from '@/components/ProgressBar'
import { engine, useActiveTabId } from '@/engine'
import '@/App.css'

function useKeymap() {
  const activeTabId = useActiveTabId()

  useEffect(() => {
    const toolMap: Record<string, string> = {
      v:'move', m:'select', l:'lasso', w:'wand', c:'crop', i:'eyedropper',
      b:'brush', e:'eraser', j:'heal', g:'gradient', t:'text', u:'shape', h:'hand', z:'zoom',
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
      const tool = toolMap[e.key.toLowerCase()];
      if (tool) { engine.dispatch({ type: 'select_tool', tool }); e.preventDefault(); }
      if (e.ctrlKey && e.key === 'o') {
        e.preventDefault();
        engine.dispatch({ type: 'open_file_dialog', tab_id: activeTabId || undefined });
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [activeTabId]);
}

export default function App() {
  useKeymap()

  return (
    <Tooltip.Provider>
      <div className="app-container">
        <MenuBar />
        <div className="workspace">
          <ActivityBar />
          <Toolbar />
          <div className="canvas-column">
            <TabBar />
            <Viewport />
          </div>
          <Sidebar />
        </div>
        <ProgressBar />
        <StatusBar />
      </div>
    </Tooltip.Provider>
  )
}

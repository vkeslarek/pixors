import { useEffect, useRef } from 'react'
// Importa o inicializador e a sua função direto do seu pacote Rust!
import init, { start_engine } from 'pixors-viewport'

function App() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    // Função assíncrona para inicializar o binário Wasm
    const bootWasm = async () => {
      // 1. Carrega o binário na memória do navegador
      await init();

      // 2. Chama a função em Rust passando o ID do canvas
      if (canvasRef.current) {
        start_engine("main-viewport");
      }

      // 3. A Mágica: Remove o splash screen estático com um pequeno fade out (opcional)
      const splash = document.getElementById('native-splash');
      if (splash) {
        splash.style.opacity = '0';

        // 2. Remove o elemento do DOM apenas depois que o fade terminar (500ms)
        setTimeout(() => {
          splash.remove();
        }, 500);
      }
    };

    bootWasm();
  }, []);

  return (
    <div style={{ display: 'flex', gap: '20px', padding: '20px', fontFamily: 'sans-serif' }}>
      {/* UI em React */}
      <div style={{ width: '250px' }}>
        <h2>Painel de Controle</h2>
        <button>Filtro 1</button>
        <button>Filtro 2</button>
      </div>

      {/* Viewport renderizado pelo Rust */}
      <div>
        <canvas
          id="main-viewport"
          ref={canvasRef}
          width={800}
          height={600}
          style={{ border: '2px solid #ccc', borderRadius: '8px' }}
        />
      </div>
    </div>
  )
}

export default App
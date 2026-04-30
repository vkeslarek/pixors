import init, { PixorsEngine } from 'pixors-wasm';

let engineInstance: PixorsEngine | null = null;

export async function initWasmEngine(): Promise<PixorsEngine | null> {
  if (engineInstance) return engineInstance;

  try {
    await init();
    engineInstance = new PixorsEngine();
    return engineInstance;
  } catch (err) {
    console.warn('[WASM] module not found, running in legacy mode', err);
    return null;
  }
}

export function getWasmEngine(): PixorsEngine | null {
  return engineInstance;
}

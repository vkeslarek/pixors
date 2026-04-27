import { decode, encode } from '@msgpack/msgpack';
import type { EngineCommand, EngineEvent } from '@/engine/types';

// Toggle this flag if you need to debug WebSocket payloads (can be noisy)
const DEBUG = import.meta.env.DEV;
const DEBUG_WS = false; 

// UUID wire format helpers
export function binToHex(b: Uint8Array): string {
  return Array.from(b).map(x => x.toString(16).padStart(2, '0')).join('');
}

export function hexToBin(h: string): Uint8Array {
  const a = new Uint8Array(h.length / 2);
  for (let i = 0; i < a.length; i++) a[i] = parseInt(h.slice(i * 2, i * 2 + 2), 16);
  return a;
}

function generateUUID(): string {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  return binToHex(b);
}

export const MSG_EVENT = 0x00;
export const MSG_TILE = 0x01;
export const MSG_TILES_COMPLETE = 0x02;

type EventCallback<T extends EngineEvent['type']> = (event: Extract<EngineEvent, { type: T }>) => void;
type BinaryCallback = (type: number, payload: Uint8Array, len: number) => void;

export class EngineClient {
  private ws: WebSocket | null = null;
  private eventListeners: Map<string, Set<Function>> = new Map();
  private wildcardListeners: Set<(e: EngineEvent) => void> = new Set();
  private binaryListeners: Set<BinaryCallback> = new Set();
  private connectionListeners: Set<(connected: boolean) => void> = new Set();
  
  public connected = false;
  public readonly sessionId: string = generateUUID();

  private wsUrl(): string {
    return `ws://127.0.0.1:8399/ws?session_id=${this.sessionId}`;
  }

  constructor() {
    // heartbeat auto-reply moved to engine.boot()
  }

  public connect() {
    if (this.ws) {
      if (DEBUG) console.log('[Engine] connect() called but already have a WS instance');
      return;
    }
    if (DEBUG) console.log('[Engine] Connecting with session', this.sessionId);
    this.ws = new WebSocket(this.wsUrl());
    this.ws.binaryType = 'arraybuffer';

    this.ws.onopen = () => {
      if (DEBUG) console.log('[Engine] WebSocket OPEN');
      this.connected = true;
      this.notifyConnection(true);
      this.sendCommand({ type: 'get_state' });
      this.sendCommand({ type: 'get_session_state' });
    };

    this.ws.onclose = (e) => {
      if (DEBUG) console.log('[Engine] WebSocket CLOSED', e.code, e.reason);
      this.connected = false;
      this.ws = null;
      this.notifyConnection(false);
      setTimeout(() => this.connect(), 1000);
    };

    this.ws.onerror = (e) => {
      if (DEBUG) console.error('[Engine] WebSocket ERROR', e);
    };

    this.ws.onmessage = (event) => {
      const buf = event.data as ArrayBuffer;
      const view = new DataView(buf);
      const type = view.getUint8(0);
      const payloadLen = view.getUint32(1, true);
      const payload = new Uint8Array(buf, 5, payloadLen);

      if (type === MSG_EVENT) {
        let msg: EngineEvent;
        try {
          const raw = decode(payload) as Record<string, unknown>;
          if (raw.tab_id instanceof Uint8Array) raw.tab_id = binToHex(raw.tab_id);
          msg = raw as unknown as EngineEvent;
        } catch (err) {
          console.error('[Engine] Failed to decode event payload:', err);
          return;
        }
        if (DEBUG_WS) console.log('[Engine] RECV EVENT:', msg.type, msg);
        this.emit(msg);
      } else if (type === MSG_TILES_COMPLETE) {
        if (DEBUG_WS) console.log('[Engine] RECV TILES_COMPLETE');
        this.emit({ type: 'tiles_complete' } as EngineEvent);
      } else {
        this.emitBinary(type, payload, payloadLen);
      }
    };
  }

  public sendCommand(cmd: EngineCommand) {
    if (this.ws?.readyState === WebSocket.OPEN) {
      if (DEBUG_WS) console.log('[Engine] SEND CMD:', cmd.type, cmd);
      const wire = { ...cmd } as Record<string, unknown>;
      if (typeof wire.tab_id === 'string' && wire.tab_id.length === 32) {
        wire.tab_id = hexToBin(wire.tab_id);
      } else if (wire.tab_id === undefined || wire.tab_id === null) {
        delete wire.tab_id;
      }
      this.ws.send(encode(wire));
    } else {
      console.warn('[Engine] WebSocket not open, cannot send command:', cmd, 'readyState:', this.ws?.readyState);
    }
  }

  public onAnyEvent(cb: (e: EngineEvent) => void): () => void {
    this.wildcardListeners.add(cb);
    return () => { this.wildcardListeners.delete(cb); };
  }

  public on<T extends EngineEvent['type']>(type: T, cb: EventCallback<T>) {
    if (!this.eventListeners.has(type)) {
      this.eventListeners.set(type, new Set());
    }
    this.eventListeners.get(type)!.add(cb);
    return () => this.off(type, cb);
  }

  public off<T extends EngineEvent['type']>(type: T, cb: EventCallback<T>) {
    this.eventListeners.get(type)?.delete(cb);
  }

  public onBinary(cb: BinaryCallback) {
    this.binaryListeners.add(cb);
    return () => { this.binaryListeners.delete(cb); };
  }

  public onConnection(cb: (connected: boolean) => void) {
    this.connectionListeners.add(cb);
    return () => { this.connectionListeners.delete(cb); };
  }

  private emit(event: EngineEvent) {
    for (const cb of this.wildcardListeners) cb(event);
    const listeners = this.eventListeners.get(event.type);
    if (listeners) {
      for (const cb of listeners) {
        cb(event);
      }
    }
  }

  private emitBinary(type: number, payload: Uint8Array, len: number) {
    for (const cb of this.binaryListeners) {
      cb(type, payload, len);
    }
  }

  private notifyConnection(connected: boolean) {
    for (const cb of this.connectionListeners) {
      cb(connected);
    }
  }
}

export const engineClient = new EngineClient();

#!/usr/bin/env node
// Quick WebSocket test for the pixors engine tile streaming protocol.
// Uses the native WebSocket available in Node >= 22.

const WS_URL = 'ws://127.0.0.1:8080/ws';

async function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }

async function main() {
  console.log('--- Test 1: Create tab + open file ---');
  
  const ws1 = new WebSocket(WS_URL);
  let tabId = null;
  
  await new Promise((resolve, reject) => {
    ws1.addEventListener('open', () => {
      console.log('[ws1] Connected');
      ws1.send(JSON.stringify({ type: 'create_tab' }));
    });
    
    ws1.addEventListener('message', (event) => {
      const str = typeof event.data === 'string' ? event.data : event.data.toString();
      try {
        const msg = JSON.parse(str);
        console.log(`[ws1] Event: ${msg.type}`, msg.tab_id ? `tab=${msg.tab_id.substring(0,8)}` : '', 
          msg.width ? `${msg.width}x${msg.height}` : '');
        
        if (msg.type === 'tab_created') {
          tabId = msg.tab_id;
          ws1.send(JSON.stringify({ type: 'activate_tab', tab_id: tabId }));
          ws1.send(JSON.stringify({ type: 'open_file', tab_id: tabId, path: 'example1.png' }));
        }
        
        if (msg.type === 'image_loaded') {
          console.log(`[ws1] ✓ Image loaded: ${msg.width}x${msg.height}`);
          resolve();
        }
        
        if (msg.type === 'error') {
          console.error(`[ws1] ERROR: ${msg.message}`);
          reject(new Error(msg.message));
        }
      } catch (e) {
        // binary data on ws1 - shouldn't happen
      }
    });
    
    ws1.addEventListener('error', (e) => reject(new Error('ws1 error')));
    setTimeout(() => reject(new Error('Timeout waiting for image_loaded')), 30000);
  });
  
  console.log(`\n--- Test 2: Viewport tile streaming (tab_id=${tabId?.substring(0,8)}) ---`);
  
  const ws2 = new WebSocket(`${WS_URL}?tab_id=${tabId}`);
  let tileCount = 0;
  let totalBytes = 0;
  let gotTilesComplete = false;
  let pendingTile = null;
  
  await new Promise((resolve, reject) => {
    ws2.addEventListener('open', () => {
      console.log('[ws2] Connected (viewport)');
      ws2.send(JSON.stringify({
        type: 'viewport_update',
        x: 0, y: 0, w: 800, h: 600, zoom: 1.0,
      }));
    });
    
    ws2.addEventListener('message', async (event) => {
      const data = event.data;
      
      if (typeof data === 'string') {
        try {
          const msg = JSON.parse(data);
          if (msg.type === 'tile_data') {
            pendingTile = msg;
          } else if (msg.type === 'tiles_complete') {
            gotTilesComplete = true;
            console.log(`[ws2] ✓ tiles_complete — ${tileCount} tiles, ${(totalBytes/1024/1024).toFixed(2)} MB`);
            resolve();
          } else if (msg.type === 'image_loaded') {
            console.log(`[ws2] Initial image_loaded: ${msg.width}x${msg.height}`);
          } else {
            console.log(`[ws2] Event: ${msg.type}`);
          }
        } catch (e) {}
      } else {
        // Binary data (ArrayBuffer or Blob)
        let byteLen;
        if (data instanceof ArrayBuffer) {
          byteLen = data.byteLength;
        } else if (data instanceof Blob) {
          byteLen = data.size;
        } else {
          byteLen = data.length || 0;
        }
        tileCount++;
        totalBytes += byteLen;
        if (tileCount <= 3 || tileCount % 10 === 0) {
          const t = pendingTile;
          console.log(`[ws2] Tile #${tileCount}: (${t?.x},${t?.y}) ${t?.width}x${t?.height} = ${byteLen} bytes`);
        }
        pendingTile = null;
      }
    });
    
    ws2.addEventListener('error', (e) => reject(new Error('ws2 error')));
    setTimeout(() => {
      if (!gotTilesComplete) {
        console.log(`[ws2] ⏰ Timeout — got ${tileCount} tiles so far`);
      }
      resolve();
    }, 30000);
  });
  
  console.log(`\n--- Results ---`);
  console.log(`Tab ID: ${tabId}`);
  console.log(`Tiles received: ${tileCount}`);
  console.log(`Total data: ${(totalBytes/1024/1024).toFixed(2)} MB`);
  console.log(`Success: ${gotTilesComplete ? '✓' : '✗'}`);
  
  ws1.close();
  ws2.close();
  
  await sleep(500);
  process.exit(gotTilesComplete ? 0 : 1);
}

main().catch(e => { console.error(e); process.exit(1); });

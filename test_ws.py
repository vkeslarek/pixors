#!/usr/bin/env python3
import asyncio
import websockets
import json
import sys

async def test():
    uri = "ws://127.0.0.1:8080/ws"
    async with websockets.connect(uri) as websocket:
        # Listen for events
        async def receive():
            try:
                while True:
                    msg = await websocket.recv()
                    print(f"Received: {msg}")
            except websockets.exceptions.ConnectionClosed:
                pass

        recv_task = asyncio.create_task(receive())

        # Send create tab command
        cmd = {"type": "create_tab"}
        await websocket.send(json.dumps(cmd))
        print("Sent create_tab")

        # Wait a bit for response
        await asyncio.sleep(2)

        # List tabs via HTTP to get tab ID
        import urllib.request
        import json as json_module
        req = urllib.request.Request('http://127.0.0.1:8080/api/tabs')
        response = urllib.request.urlopen(req)
        tabs = json_module.loads(response.read().decode())
        print(f"Tabs: {tabs}")
        if tabs:
            tab_id = tabs[0]
            # Activate tab
            cmd = {"type": "activate_tab", "tab_id": tab_id}
            await websocket.send(json.dumps(cmd))
            print(f"Sent activate_tab for {tab_id}")
            # Open file
            cmd = {"type": "open_file", "tab_id": tab_id, "path": "example1.png"}
            await websocket.send(json.dumps(cmd))
            print(f"Sent open_file for {tab_id}")

        # Wait for events
        await asyncio.sleep(5)
        recv_task.cancel()

if __name__ == "__main__":
    asyncio.run(test())
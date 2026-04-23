import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { CallToolRequestSchema, ListToolsRequestSchema, } from "@modelcontextprotocol/sdk/types.js";
import WebSocket from "ws";
const ENGINE_BASE = "http://127.0.0.1:8080";
const ENGINE_WS_BASE = "ws://127.0.0.1:8080/ws";
// Initialize the MCP Server
const server = new Server({
    name: "pixors-mcp-bridge",
    version: "1.0.0",
}, {
    capabilities: {
        tools: {},
    },
});
let activeWsConnections = new Map();
// Helper to ensure WS connection for a tab
function getOrCreateWs(tabId) {
    if (activeWsConnections.has(tabId)) {
        const ws = activeWsConnections.get(tabId);
        if (ws.readyState === WebSocket.OPEN)
            return Promise.resolve(ws);
    }
    return new Promise((resolve, reject) => {
        const ws = new WebSocket(`${ENGINE_WS_BASE}?tab_id=${tabId}`);
        ws.on("open", () => {
            activeWsConnections.set(tabId, ws);
            resolve(ws);
        });
        ws.on("error", (err) => {
            reject(err);
        });
        ws.on("close", () => {
            activeWsConnections.delete(tabId);
        });
    });
}
// REST helpers
async function createTab() {
    const res = await fetch(`${ENGINE_BASE}/api/tabs`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({})
    });
    if (!res.ok)
        throw new Error(await res.text());
    return await res.json();
}
async function closeTab(tabId) {
    const res = await fetch(`${ENGINE_BASE}/api/tabs/${tabId}`, {
        method: "DELETE"
    });
    if (!res.ok)
        throw new Error(await res.text());
    const ws = activeWsConnections.get(tabId);
    if (ws) {
        ws.close();
        activeWsConnections.delete(tabId);
    }
    return { success: true };
}
async function openImage(tabId, path) {
    const res = await fetch(`${ENGINE_BASE}/api/tabs/${tabId}/open`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path })
    });
    if (!res.ok)
        throw new Error(await res.text());
    return await res.json();
}
async function getState() {
    const res = await fetch(`${ENGINE_BASE}/api/state`);
    if (!res.ok)
        throw new Error(await res.text());
    return await res.json();
}
// List Tools
server.setRequestHandler(ListToolsRequestSchema, async () => {
    return {
        tools: [
            {
                name: "pixors_create_tab",
                description: "Create a new editing tab",
                inputSchema: { type: "object", properties: {} },
            },
            {
                name: "pixors_close_tab",
                description: "Close an existing tab and free its resources",
                inputSchema: {
                    type: "object",
                    properties: {
                        tab_id: { type: "string" },
                    },
                    required: ["tab_id"],
                },
            },
            {
                name: "pixors_open_image",
                description: "Load an image file into a specific tab",
                inputSchema: {
                    type: "object",
                    properties: {
                        tab_id: { type: "string" },
                        path: { type: "string", description: "Absolute path to the image" },
                    },
                    required: ["tab_id", "path"],
                },
            },
            {
                name: "pixors_get_state",
                description: "Get full application state snapshot",
                inputSchema: { type: "object", properties: {} },
            },
            {
                name: "pixors_activate_tab",
                description: "Switch active tab in the UI",
                inputSchema: {
                    type: "object",
                    properties: {
                        tab_id: { type: "string" },
                    },
                    required: ["tab_id"],
                },
            },
            {
                name: "pixors_select_tool",
                description: "Change active editing tool",
                inputSchema: {
                    type: "object",
                    properties: {
                        tab_id: { type: "string" },
                        tool: { type: "string" },
                    },
                    required: ["tab_id", "tool"],
                },
            },
        ],
    };
});
// Call Tool
server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;
    try {
        if (name === "pixors_create_tab") {
            const result = await createTab();
            return { content: [{ type: "text", text: `Success: ${JSON.stringify(result)}` }] };
        }
        if (name === "pixors_close_tab") {
            const result = await closeTab(args.tab_id);
            return { content: [{ type: "text", text: `Success: ${JSON.stringify(result)}` }] };
        }
        if (name === "pixors_open_image") {
            const result = await openImage(args.tab_id, args.path);
            return { content: [{ type: "text", text: `Success: ${JSON.stringify(result)}` }] };
        }
        if (name === "pixors_get_state") {
            const result = await getState();
            return { content: [{ type: "text", text: `State: ${JSON.stringify(result)}` }] };
        }
        if (name === "pixors_activate_tab") {
            const tabId = args.tab_id;
            const ws = await getOrCreateWs(tabId);
            ws.send(JSON.stringify({ type: "ActivateTab", tab_id: tabId }));
            return { content: [{ type: "text", text: `Success: Tab activated` }] };
        }
        if (name === "pixors_select_tool") {
            const tabId = args.tab_id;
            const ws = await getOrCreateWs(tabId);
            ws.send(JSON.stringify({ type: "SelectTool", tool: args.tool }));
            return { content: [{ type: "text", text: `Success: Tool selected` }] };
        }
        throw new Error(`Unknown tool: ${name}`);
    }
    catch (error) {
        return {
            content: [{ type: "text", text: `Error: ${error.message}` }],
            isError: true,
        };
    }
});
// Start the server
async function run() {
    const transport = new StdioServerTransport();
    await server.connect(transport);
}
run().catch((e) => {
    process.stderr.write(`Error: ${e.message}\n`);
});
//# sourceMappingURL=index.js.map
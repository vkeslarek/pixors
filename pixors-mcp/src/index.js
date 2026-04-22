import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { CallToolRequestSchema, ListToolsRequestSchema, } from "@modelcontextprotocol/sdk/types.js";
import WebSocket from "ws";
// Initialize the MCP Server
const server = new Server({
    name: "pixors-mcp-bridge",
    version: "1.0.0",
}, {
    capabilities: {
        tools: {},
    },
});
// State
let ws = null;
let connectionPromise = null;
// Ensure WebSocket connection to engine
async function ensureConnection() {
    if (ws && ws.readyState === WebSocket.OPEN)
        return;
    if (connectionPromise)
        return connectionPromise;
    connectionPromise = new Promise((resolve, reject) => {
        ws = new WebSocket("ws://127.0.0.1:8080/ws");
        ws.on("open", () => {
            resolve();
        });
        ws.on("error", (err) => {
            // Don't log to stdout as it breaks MCP Stdio protocol
            reject(err);
        });
        ws.on("close", () => {
            ws = null;
            connectionPromise = null;
        });
    });
    return connectionPromise;
}
// Request helpers
async function sendCommandAndWait(command) {
    await ensureConnection();
    return new Promise((resolve, reject) => {
        if (!ws)
            return reject(new Error("No connection"));
        const handler = (data) => {
            try {
                const msg = JSON.parse(data.toString());
                if (msg.type === "error") {
                    ws?.removeListener("message", handler);
                    reject(new Error(msg.message));
                }
                else if (msg.type === "image_loaded" || msg.type === "image_info") {
                    ws?.removeListener("message", handler);
                    resolve(msg);
                }
            }
            catch (e) {
                // Ignore binary
            }
        };
        ws.on("message", handler);
        ws.send(JSON.stringify(command));
        // Timeout
        setTimeout(() => {
            ws?.removeListener("message", handler);
            reject(new Error("Request timed out"));
        }, 5000);
    });
}
// List Tools
server.setRequestHandler(ListToolsRequestSchema, async () => {
    return {
        tools: [
            {
                name: "pixors_load_image",
                description: "Load an image file into the Pixors Engine",
                inputSchema: {
                    type: "object",
                    properties: {
                        path: {
                            type: "string",
                            description: "Absolute path to the image",
                        },
                    },
                    required: ["path"],
                },
            },
            {
                name: "pixors_get_info",
                description: "Get information about the currently loaded image",
                inputSchema: {
                    type: "object",
                    properties: {},
                },
            },
        ],
    };
});
// Call Tool
server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;
    try {
        if (name === "pixors_load_image") {
            const path = args.path;
            const result = await sendCommandAndWait({
                type: "load_image",
                path,
            });
            return {
                content: [{ type: "text", text: `Success: ${JSON.stringify(result)}` }],
            };
        }
        if (name === "pixors_get_info") {
            const result = await sendCommandAndWait({
                type: "get_image_info",
            });
            return {
                content: [{ type: "text", text: `Info: ${JSON.stringify(result)}` }],
            };
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
    // Silent error, stderror is okay but stdout must be clean for MCP
    process.stderr.write(`Error: ${e.message}\n`);
});
//# sourceMappingURL=index.js.map
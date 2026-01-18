#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  ListResourcesRequestSchema,
  ReadResourceRequestSchema,
  ListPromptsRequestSchema,
  GetPromptRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import * as rustledger from "@rustledger/wasm";

// Import modular components
import { TOOLS } from "./tools.js";
import { handleToolCall } from "./handlers.js";
import { RESOURCES, getResourceContents } from "./resources.js";
import { PROMPTS, getPrompt } from "./prompts.js";
import type { ToolArguments } from "./types.js";

// Initialize WASM module
rustledger.init();

// Create server instance
const server = new Server(
  {
    name: "rustledger",
    version: rustledger.version(),
  },
  {
    capabilities: {
      tools: {},
      resources: {},
      prompts: {},
    },
  }
);

// List available tools
server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools: TOOLS };
});

// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  try {
    return handleToolCall(name, args as ToolArguments | undefined);
  } catch (error) {
    return {
      isError: true,
      content: [
        {
          type: "text" as const,
          text: `Error: ${error instanceof Error ? error.message : String(error)}`,
        },
      ],
    };
  }
});

// List available resources
server.setRequestHandler(ListResourcesRequestSchema, async () => {
  return { resources: RESOURCES };
});

// Read resource contents
server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
  const { uri } = request.params;
  const contents = getResourceContents(uri);

  if (!contents) {
    throw new Error(`Unknown resource: ${uri}`);
  }

  return { contents: [contents] };
});

// List available prompts
server.setRequestHandler(ListPromptsRequestSchema, async () => {
  return { prompts: PROMPTS };
});

// Get prompt content
server.setRequestHandler(GetPromptRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  return getPrompt(name, args);
});

// Start the server
async function main(): Promise<void> {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error(`rustledger MCP server v${rustledger.version()} started`);
}

main().catch((error) => {
  console.error("Fatal error:", error);
  process.exit(1);
});

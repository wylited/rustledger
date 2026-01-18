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
import * as fs from "fs";
import * as path from "path";

// Import rustledger WASM bindings
import * as rustledger from "@rustledger/wasm";

// Initialize WASM module
rustledger.init();

// ============================================================================
// Type Definitions for Directives
// ============================================================================

interface Amount {
  number: string;
  currency: string;
}

interface Posting {
  account: string;
  units?: Amount;
}

interface BaseDirective {
  type: string;
  date: string;
}

interface TransactionDirective extends BaseDirective {
  type: "transaction";
  flag: string;
  payee?: string;
  narration?: string;
  tags?: string[];
  links?: string[];
  postings: Posting[];
}

interface OpenDirective extends BaseDirective {
  type: "open";
  account: string;
  currencies?: string[];
  booking?: string;
}

interface CloseDirective extends BaseDirective {
  type: "close";
  account: string;
}

interface BalanceDirective extends BaseDirective {
  type: "balance";
  account: string;
  amount: Amount;
}

interface CommodityDirective extends BaseDirective {
  type: "commodity";
  currency: string;
}

interface PriceDirective extends BaseDirective {
  type: "price";
  currency: string;
  amount: Amount;
}

interface EventDirective extends BaseDirective {
  type: "event";
  event_type: string;
  value: string;
}

interface NoteDirective extends BaseDirective {
  type: "note";
  account: string;
  comment: string;
}

interface DocumentDirective extends BaseDirective {
  type: "document";
  account: string;
  path: string;
}

interface PadDirective extends BaseDirective {
  type: "pad";
  account: string;
  source_account: string;
}

interface QueryDirective extends BaseDirective {
  type: "query";
  name: string;
  query_string: string;
}

interface CustomDirective extends BaseDirective {
  type: "custom";
  custom_type: string;
}

type Directive =
  | TransactionDirective
  | OpenDirective
  | CloseDirective
  | BalanceDirective
  | CommodityDirective
  | PriceDirective
  | EventDirective
  | NoteDirective
  | DocumentDirective
  | PadDirective
  | QueryDirective
  | CustomDirective;

interface DocumentSymbol {
  name: string;
  kind: string;
  detail?: string;
  range: {
    start_line: number;
    end_line: number;
    start_character: number;
    end_character: number;
  };
}

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

// Tool definitions
const TOOLS = [
  // === Original Tools ===
  {
    name: "validate",
    description:
      "Validate a Beancount ledger. Returns validation errors and warnings if any issues are found.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text to validate",
        },
      },
      required: ["source"],
    },
  },
  {
    name: "query",
    description:
      "Run a BQL (Beancount Query Language) query on a ledger. Use this for account balances, filtering transactions, aggregations, etc.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
        query: {
          type: "string",
          description:
            'The BQL query to execute (e.g., "SELECT account, sum(position) GROUP BY account")',
        },
      },
      required: ["source", "query"],
    },
  },
  {
    name: "balances",
    description:
      "Get account balances from a ledger. This is a shorthand for running a BALANCES query.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
      },
      required: ["source"],
    },
  },
  {
    name: "format",
    description:
      "Format a Beancount ledger with consistent alignment and spacing.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text to format",
        },
      },
      required: ["source"],
    },
  },
  {
    name: "parse",
    description:
      "Parse a Beancount ledger and return the directives as structured data.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text to parse",
        },
      },
      required: ["source"],
    },
  },
  {
    name: "completions",
    description:
      "Get BQL query completions at a cursor position. Useful for building query editors.",
    inputSchema: {
      type: "object" as const,
      properties: {
        partial_query: {
          type: "string",
          description: "The partial BQL query text",
        },
        cursor_pos: {
          type: "number",
          description: "The cursor position (character offset) in the query",
        },
      },
      required: ["partial_query", "cursor_pos"],
    },
  },
  {
    name: "list_plugins",
    description: "List available native plugins that can process ledgers.",
    inputSchema: {
      type: "object" as const,
      properties: {},
      required: [],
    },
  },
  {
    name: "run_plugin",
    description: "Run a native plugin on a ledger.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
        plugin_name: {
          type: "string",
          description:
            "The name of the plugin to run (use list_plugins to see available plugins)",
        },
      },
      required: ["source", "plugin_name"],
    },
  },

  // === Editor Tools (LSP-like) ===
  {
    name: "editor_completions",
    description:
      "Get context-aware completions for Beancount source at a given position. Returns account names, currencies, directives, etc.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
        line: {
          type: "number",
          description: "Line number (0-indexed)",
        },
        character: {
          type: "number",
          description: "Character position in the line (0-indexed)",
        },
      },
      required: ["source", "line", "character"],
    },
  },
  {
    name: "editor_hover",
    description:
      "Get hover information for a symbol at the given position. Returns documentation for accounts, currencies, and directives.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
        line: {
          type: "number",
          description: "Line number (0-indexed)",
        },
        character: {
          type: "number",
          description: "Character position in the line (0-indexed)",
        },
      },
      required: ["source", "line", "character"],
    },
  },
  {
    name: "editor_definition",
    description:
      "Get the definition location for a symbol (account or currency). Returns the location of the Open or Commodity directive.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
        line: {
          type: "number",
          description: "Line number (0-indexed)",
        },
        character: {
          type: "number",
          description: "Character position in the line (0-indexed)",
        },
      },
      required: ["source", "line", "character"],
    },
  },
  {
    name: "editor_document_symbols",
    description:
      "Get all symbols in the document for an outline/structure view. Returns all directives with their positions.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
      },
      required: ["source"],
    },
  },

  // === Analysis Tools ===
  {
    name: "ledger_stats",
    description:
      "Get statistics about a ledger: counts of directives, accounts, currencies, date range, etc.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
      },
      required: ["source"],
    },
  },
  {
    name: "list_accounts",
    description:
      "List all accounts in the ledger with their open/close dates and currencies.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
      },
      required: ["source"],
    },
  },
  {
    name: "list_commodities",
    description:
      "List all currencies/commodities used in the ledger.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
      },
      required: ["source"],
    },
  },
  {
    name: "account_activity",
    description:
      "Get activity summary for a specific account: first/last transaction, total transactions, currencies used.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
        account: {
          type: "string",
          description: "The account name to analyze (e.g., 'Assets:Checking')",
        },
      },
      required: ["source", "account"],
    },
  },

  // === Utility Tools ===
  {
    name: "format_check",
    description:
      "Check if a ledger is properly formatted. Returns differences if formatting would change the file.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
      },
      required: ["source"],
    },
  },
  {
    name: "bql_tables",
    description:
      "Get documentation for BQL tables (entries, postings, etc.) and their available columns.",
    inputSchema: {
      type: "object" as const,
      properties: {},
      required: [],
    },
  },
  {
    name: "directive_at_line",
    description:
      "Get the directive at a specific line number in the source.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
        line: {
          type: "number",
          description: "Line number (1-indexed)",
        },
      },
      required: ["source", "line"],
    },
  },
  {
    name: "find_transactions",
    description:
      "Find transactions matching criteria: payee, narration, tags, or date range.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
        payee: {
          type: "string",
          description: "Filter by payee (substring match)",
        },
        narration: {
          type: "string",
          description: "Filter by narration (substring match)",
        },
        tag: {
          type: "string",
          description: "Filter by tag",
        },
        from_date: {
          type: "string",
          description: "Filter by start date (YYYY-MM-DD)",
        },
        to_date: {
          type: "string",
          description: "Filter by end date (YYYY-MM-DD)",
        },
        limit: {
          type: "number",
          description: "Maximum number of results (default: 50)",
        },
      },
      required: ["source"],
    },
  },

  // === Report Tool ===
  {
    name: "report",
    description:
      "Generate financial reports: balance sheet, income statement, holdings, or net worth.",
    inputSchema: {
      type: "object" as const,
      properties: {
        source: {
          type: "string",
          description: "The Beancount ledger source text",
        },
        report_type: {
          type: "string",
          enum: ["balsheet", "income", "balances", "holdings", "networth"],
          description: "Type of report to generate",
        },
        currency: {
          type: "string",
          description: "Convert all amounts to this currency (optional)",
        },
      },
      required: ["source", "report_type"],
    },
  },

  // === File Operation Tools ===
  {
    name: "validate_file",
    description:
      "Validate a Beancount file from the filesystem. Handles includes automatically.",
    inputSchema: {
      type: "object" as const,
      properties: {
        file_path: {
          type: "string",
          description: "Path to the Beancount file to validate",
        },
      },
      required: ["file_path"],
    },
  },
  {
    name: "query_file",
    description:
      "Run a BQL query on a Beancount file from the filesystem.",
    inputSchema: {
      type: "object" as const,
      properties: {
        file_path: {
          type: "string",
          description: "Path to the Beancount file",
        },
        query: {
          type: "string",
          description: "The BQL query to execute",
        },
      },
      required: ["file_path", "query"],
    },
  },
  {
    name: "format_file",
    description:
      "Format a Beancount file. Can optionally write the formatted output back to the file.",
    inputSchema: {
      type: "object" as const,
      properties: {
        file_path: {
          type: "string",
          description: "Path to the Beancount file to format",
        },
        write: {
          type: "boolean",
          description: "If true, write the formatted output back to the file",
        },
      },
      required: ["file_path"],
    },
  },
];

// List available tools
server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools: TOOLS };
});

// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  try {
    switch (name) {
      // === Original Tools ===
      case "validate": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const result = rustledger.validateSource(source);
        return {
          content: [
            {
              type: "text",
              text: result.valid
                ? "Ledger is valid."
                : `Found ${result.errors.length} error(s):\n${formatErrors(result.errors)}`,
            },
          ],
        };
      }

      case "query": {
        const source = args?.source as string;
        const query = args?.query as string;
        if (!source || !query) {
          return {
            isError: true,
            content: [
              { type: "text", text: "Missing required arguments: source and query" },
            ],
          };
        }
        const result = rustledger.query(source, query);
        if (result.errors?.length > 0) {
          return {
            isError: true,
            content: [{ type: "text", text: formatErrors(result.errors) }],
          };
        }
        return {
          content: [{ type: "text", text: formatQueryResult(result) }],
        };
      }

      case "balances": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const result = rustledger.balances(source);
        if (result.errors?.length > 0) {
          return {
            isError: true,
            content: [{ type: "text", text: formatErrors(result.errors) }],
          };
        }
        return {
          content: [{ type: "text", text: formatQueryResult(result) }],
        };
      }

      case "format": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const result = rustledger.format(source);
        if (result.errors?.length > 0) {
          return {
            isError: true,
            content: [{ type: "text", text: formatErrors(result.errors) }],
          };
        }
        return {
          content: [{ type: "text", text: result.formatted || "" }],
        };
      }

      case "parse": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const result = rustledger.parse(source);
        if (result.errors?.length > 0) {
          return {
            isError: true,
            content: [{ type: "text", text: formatErrors(result.errors) }],
          };
        }
        return {
          content: [
            {
              type: "text",
              text: JSON.stringify(result.ledger, null, 2),
            },
          ],
        };
      }

      case "completions": {
        const partialQuery = args?.partial_query as string;
        const cursorPos = args?.cursor_pos as number;
        if (partialQuery === undefined || cursorPos === undefined) {
          return {
            isError: true,
            content: [
              {
                type: "text",
                text: "Missing required arguments: partial_query and cursor_pos",
              },
            ],
          };
        }
        const result = rustledger.bqlCompletions(partialQuery, cursorPos);
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      }

      case "list_plugins": {
        const plugins = rustledger.listPlugins();
        return {
          content: [{ type: "text", text: JSON.stringify(plugins, null, 2) }],
        };
      }

      case "run_plugin": {
        const source = args?.source as string;
        const pluginName = args?.plugin_name as string;
        if (!source || !pluginName) {
          return {
            isError: true,
            content: [
              {
                type: "text",
                text: "Missing required arguments: source and plugin_name",
              },
            ],
          };
        }
        const result = rustledger.runPlugin(source, pluginName);
        if (result.errors?.length > 0) {
          return {
            isError: true,
            content: [{ type: "text", text: formatErrors(result.errors) }],
          };
        }
        return {
          content: [
            {
              type: "text",
              text: `Plugin processed ${result.directives.length} directives.`,
            },
          ],
        };
      }

      // === Editor Tools ===
      case "editor_completions": {
        const source = args?.source as string;
        const line = args?.line as number;
        const character = args?.character as number;
        if (!source || line === undefined || character === undefined) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required arguments: source, line, character" }],
          };
        }
        const ledger = new rustledger.ParsedLedger(source);
        const result = ledger.getCompletions(line, character);
        ledger.free();
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      }

      case "editor_hover": {
        const source = args?.source as string;
        const line = args?.line as number;
        const character = args?.character as number;
        if (!source || line === undefined || character === undefined) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required arguments: source, line, character" }],
          };
        }
        const ledger = new rustledger.ParsedLedger(source);
        const result = ledger.getHoverInfo(line, character);
        ledger.free();
        if (!result) {
          return {
            content: [{ type: "text", text: "No hover information available at this position." }],
          };
        }
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      }

      case "editor_definition": {
        const source = args?.source as string;
        const line = args?.line as number;
        const character = args?.character as number;
        if (!source || line === undefined || character === undefined) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required arguments: source, line, character" }],
          };
        }
        const ledger = new rustledger.ParsedLedger(source);
        const result = ledger.getDefinition(line, character);
        ledger.free();
        if (!result) {
          return {
            content: [{ type: "text", text: "No definition found at this position." }],
          };
        }
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      }

      case "editor_document_symbols": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const ledger = new rustledger.ParsedLedger(source);
        const result = ledger.getDocumentSymbols();
        ledger.free();
        return {
          content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        };
      }

      // === Analysis Tools ===
      case "ledger_stats": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const ledger = new rustledger.ParsedLedger(source);
        const directives = ledger.getDirectives();

        const stats = {
          total_directives: directives.length,
          transactions: 0,
          open_accounts: 0,
          close_accounts: 0,
          balance_assertions: 0,
          commodities: 0,
          prices: 0,
          events: 0,
          notes: 0,
          documents: 0,
          pads: 0,
          queries: 0,
          custom: 0,
          unique_accounts: new Set<string>(),
          unique_currencies: new Set<string>(),
          date_range: { first: "", last: "" },
          is_valid: ledger.isValid(),
          error_count: ledger.getErrors().length,
        };

        for (const d of directives as Directive[]) {
          if (!stats.date_range.first || d.date < stats.date_range.first) {
            stats.date_range.first = d.date;
          }
          if (!stats.date_range.last || d.date > stats.date_range.last) {
            stats.date_range.last = d.date;
          }

          switch (d.type) {
            case "transaction":
              stats.transactions++;
              for (const p of d.postings) {
                stats.unique_accounts.add(p.account);
                if (p.units?.currency) {
                  stats.unique_currencies.add(p.units.currency);
                }
              }
              break;
            case "open":
              stats.open_accounts++;
              stats.unique_accounts.add(d.account);
              break;
            case "close":
              stats.close_accounts++;
              break;
            case "balance":
              stats.balance_assertions++;
              break;
            case "commodity":
              stats.commodities++;
              stats.unique_currencies.add(d.currency);
              break;
            case "price":
              stats.prices++;
              break;
            case "event":
              stats.events++;
              break;
            case "note":
              stats.notes++;
              break;
            case "document":
              stats.documents++;
              break;
            case "pad":
              stats.pads++;
              break;
            case "query":
              stats.queries++;
              break;
            case "custom":
              stats.custom++;
              break;
          }
        }

        ledger.free();

        const output = {
          ...stats,
          unique_accounts: stats.unique_accounts.size,
          unique_currencies: Array.from(stats.unique_currencies),
        };
        delete (output as Record<string, unknown>).unique_accounts;

        return {
          content: [{ type: "text", text: JSON.stringify({
            ...output,
            account_count: stats.unique_accounts.size,
            currency_count: stats.unique_currencies.size,
            currencies: Array.from(stats.unique_currencies),
          }, null, 2) }],
        };
      }

      case "list_accounts": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const ledger = new rustledger.ParsedLedger(source);
        const directives = ledger.getDirectives();

        const accounts: Record<string, { open_date?: string; close_date?: string; currencies: string[]; booking?: string }> = {};

        for (const d of directives as Directive[]) {
          if (d.type === "open") {
            accounts[d.account] = {
              open_date: d.date,
              currencies: d.currencies || [],
              booking: d.booking,
            };
          } else if (d.type === "close") {
            if (accounts[d.account]) {
              accounts[d.account].close_date = d.date;
            } else {
              accounts[d.account] = { close_date: d.date, currencies: [] };
            }
          }
        }

        ledger.free();

        return {
          content: [{ type: "text", text: JSON.stringify(accounts, null, 2) }],
        };
      }

      case "list_commodities": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const ledger = new rustledger.ParsedLedger(source);
        const directives = ledger.getDirectives();

        const commodities = new Set<string>();

        for (const d of directives as Directive[]) {
          if (d.type === "commodity") {
            commodities.add(d.currency);
          } else if (d.type === "price") {
            commodities.add(d.currency);
            commodities.add(d.amount.currency);
          } else if (d.type === "transaction") {
            for (const p of d.postings) {
              if (p.units?.currency) {
                commodities.add(p.units.currency);
              }
            }
          }
        }

        ledger.free();

        return {
          content: [{ type: "text", text: JSON.stringify(Array.from(commodities).sort(), null, 2) }],
        };
      }

      case "account_activity": {
        const source = args?.source as string;
        const account = args?.account as string;
        if (!source || !account) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required arguments: source, account" }],
          };
        }
        const ledger = new rustledger.ParsedLedger(source);
        const directives = ledger.getDirectives();

        const activity = {
          account,
          open_date: null as string | null,
          close_date: null as string | null,
          first_transaction: null as string | null,
          last_transaction: null as string | null,
          transaction_count: 0,
          currencies_used: new Set<string>(),
        };

        for (const d of directives as Directive[]) {
          if (d.type === "open" && d.account === account) {
            activity.open_date = d.date;
          } else if (d.type === "close" && d.account === account) {
            activity.close_date = d.date;
          } else if (d.type === "transaction") {
            for (const p of d.postings) {
              if (p.account === account || p.account.startsWith(account + ":")) {
                activity.transaction_count++;
                if (!activity.first_transaction || d.date < activity.first_transaction) {
                  activity.first_transaction = d.date;
                }
                if (!activity.last_transaction || d.date > activity.last_transaction) {
                  activity.last_transaction = d.date;
                }
                if (p.units?.currency) {
                  activity.currencies_used.add(p.units.currency);
                }
                break;
              }
            }
          }
        }

        ledger.free();

        return {
          content: [{ type: "text", text: JSON.stringify({
            ...activity,
            currencies_used: Array.from(activity.currencies_used),
          }, null, 2) }],
        };
      }

      // === Utility Tools ===
      case "format_check": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const result = rustledger.format(source);
        if (result.errors?.length > 0) {
          return {
            isError: true,
            content: [{ type: "text", text: formatErrors(result.errors) }],
          };
        }
        const formatted = result.formatted || "";
        const isFormatted = source === formatted;
        return {
          content: [{
            type: "text",
            text: isFormatted
              ? "File is properly formatted."
              : `File needs formatting. ${formatted.split("\n").length - source.split("\n").length} line(s) would change.`,
          }],
        };
      }

      case "bql_tables": {
        return {
          content: [{ type: "text", text: BQL_TABLES_DOCS }],
        };
      }

      case "directive_at_line": {
        const source = args?.source as string;
        const line = args?.line as number;
        if (!source || line === undefined) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required arguments: source, line" }],
          };
        }
        const ledger = new rustledger.ParsedLedger(source);
        const symbols = ledger.getDocumentSymbols();
        ledger.free();

        // Find the symbol that contains this line
        for (const symbol of symbols as DocumentSymbol[]) {
          if (symbol.range.start_line <= line - 1 && symbol.range.end_line >= line - 1) {
            return {
              content: [{ type: "text", text: JSON.stringify(symbol, null, 2) }],
            };
          }
        }

        return {
          content: [{ type: "text", text: "No directive found at this line." }],
        };
      }

      case "find_transactions": {
        const source = args?.source as string;
        if (!source) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: source" }],
          };
        }
        const payee = args?.payee as string | undefined;
        const narration = args?.narration as string | undefined;
        const tag = args?.tag as string | undefined;
        const fromDate = args?.from_date as string | undefined;
        const toDate = args?.to_date as string | undefined;
        const limit = (args?.limit as number) || 50;

        const ledger = new rustledger.ParsedLedger(source);
        const directives = ledger.getDirectives();
        ledger.free();

        const results: unknown[] = [];

        for (const d of directives as Directive[]) {
          if (results.length >= limit) break;
          if (d.type !== "transaction") continue;

          if (fromDate && d.date < fromDate) continue;
          if (toDate && d.date > toDate) continue;
          if (payee && (!d.payee || !d.payee.toLowerCase().includes(payee.toLowerCase()))) continue;
          if (narration && (!d.narration || !d.narration.toLowerCase().includes(narration.toLowerCase()))) continue;
          if (tag && (!d.tags || !d.tags.includes(tag))) continue;

          results.push(d);
        }

        return {
          content: [{ type: "text", text: JSON.stringify(results, null, 2) }],
        };
      }

      // === Report Tool ===
      case "report": {
        const source = args?.source as string;
        const reportType = args?.report_type as string;
        if (!source || !reportType) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required arguments: source, report_type" }],
          };
        }

        let query: string;
        switch (reportType) {
          case "balsheet":
            query = `SELECT account, sum(position)
                     WHERE account ~ "^(Assets|Liabilities)"
                     GROUP BY account
                     ORDER BY account`;
            break;
          case "income":
            query = `SELECT account, sum(position)
                     WHERE account ~ "^(Income|Expenses)"
                     GROUP BY account
                     ORDER BY account`;
            break;
          case "balances":
            query = "BALANCES";
            break;
          case "holdings":
            query = `SELECT account, sum(position)
                     WHERE account ~ "^Assets"
                     GROUP BY account
                     ORDER BY account`;
            break;
          case "networth":
            query = `SELECT sum(position)
                     WHERE account ~ "^(Assets|Liabilities)"`;
            break;
          default:
            return {
              isError: true,
              content: [{ type: "text", text: `Unknown report type: ${reportType}` }],
            };
        }

        const result = rustledger.query(source, query);
        if (result.errors?.length > 0) {
          return {
            isError: true,
            content: [{ type: "text", text: formatErrors(result.errors) }],
          };
        }

        return {
          content: [{ type: "text", text: `# ${reportType.toUpperCase()} Report\n\n${formatQueryResult(result)}` }],
        };
      }

      // === File Operation Tools ===
      case "validate_file": {
        const filePath = args?.file_path as string;
        if (!filePath) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: file_path" }],
          };
        }
        try {
          const absolutePath = path.resolve(filePath);
          const source = fs.readFileSync(absolutePath, "utf-8");
          const result = rustledger.validateSource(source);
          return {
            content: [
              {
                type: "text",
                text: result.valid
                  ? `${absolutePath}: Ledger is valid.`
                  : `${absolutePath}: Found ${result.errors.length} error(s):\n${formatErrors(result.errors)}`,
              },
            ],
          };
        } catch (error) {
          return {
            isError: true,
            content: [{ type: "text", text: `Error reading file: ${error instanceof Error ? error.message : String(error)}` }],
          };
        }
      }

      case "query_file": {
        const filePath = args?.file_path as string;
        const query = args?.query as string;
        if (!filePath || !query) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required arguments: file_path, query" }],
          };
        }
        try {
          const absolutePath = path.resolve(filePath);
          const source = fs.readFileSync(absolutePath, "utf-8");
          const result = rustledger.query(source, query);
          if (result.errors?.length > 0) {
            return {
              isError: true,
              content: [{ type: "text", text: formatErrors(result.errors) }],
            };
          }
          return {
            content: [{ type: "text", text: formatQueryResult(result) }],
          };
        } catch (error) {
          return {
            isError: true,
            content: [{ type: "text", text: `Error: ${error instanceof Error ? error.message : String(error)}` }],
          };
        }
      }

      case "format_file": {
        const filePath = args?.file_path as string;
        const write = args?.write as boolean;
        if (!filePath) {
          return {
            isError: true,
            content: [{ type: "text", text: "Missing required argument: file_path" }],
          };
        }
        try {
          const absolutePath = path.resolve(filePath);
          const source = fs.readFileSync(absolutePath, "utf-8");
          const result = rustledger.format(source);
          if (result.errors?.length > 0) {
            return {
              isError: true,
              content: [{ type: "text", text: formatErrors(result.errors) }],
            };
          }
          if (write && result.formatted) {
            fs.writeFileSync(absolutePath, result.formatted);
            return {
              content: [{ type: "text", text: `Formatted and saved: ${absolutePath}` }],
            };
          }
          return {
            content: [{ type: "text", text: result.formatted || "" }],
          };
        } catch (error) {
          return {
            isError: true,
            content: [{ type: "text", text: `Error: ${error instanceof Error ? error.message : String(error)}` }],
          };
        }
      }

      default:
        return {
          isError: true,
          content: [{ type: "text", text: `Unknown tool: ${name}` }],
        };
    }
  } catch (error) {
    return {
      isError: true,
      content: [
        {
          type: "text",
          text: `Error: ${error instanceof Error ? error.message : String(error)}`,
        },
      ],
    };
  }
});

// Resources
const RESOURCES = [
  {
    uri: "rustledger://docs/bql",
    name: "BQL Query Language Reference",
    description: "Documentation for Beancount Query Language",
    mimeType: "text/markdown",
  },
  {
    uri: "rustledger://docs/validation-errors",
    name: "Validation Error Reference",
    description: "All 27 validation error codes with descriptions",
    mimeType: "text/markdown",
  },
  {
    uri: "rustledger://docs/bql-functions",
    name: "BQL Functions Reference",
    description: "Complete BQL function documentation",
    mimeType: "text/markdown",
  },
  {
    uri: "rustledger://docs/directives",
    name: "Beancount Directives Reference",
    description: "All Beancount directive types and their syntax",
    mimeType: "text/markdown",
  },
];

server.setRequestHandler(ListResourcesRequestSchema, async () => {
  return { resources: RESOURCES };
});

server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
  const { uri } = request.params;

  switch (uri) {
    case "rustledger://docs/bql":
      return {
        contents: [{ uri, mimeType: "text/markdown", text: BQL_DOCS }],
      };
    case "rustledger://docs/validation-errors":
      return {
        contents: [{ uri, mimeType: "text/markdown", text: VALIDATION_ERRORS_DOCS }],
      };
    case "rustledger://docs/bql-functions":
      return {
        contents: [{ uri, mimeType: "text/markdown", text: BQL_FUNCTIONS_DOCS }],
      };
    case "rustledger://docs/directives":
      return {
        contents: [{ uri, mimeType: "text/markdown", text: DIRECTIVES_DOCS }],
      };
    default:
      throw new Error(`Unknown resource: ${uri}`);
  }
});

// Prompts
const PROMPTS = [
  {
    name: "analyze_ledger",
    description: "Analyze a Beancount ledger for insights and potential issues",
    arguments: [
      {
        name: "focus",
        description: "What to focus on: spending, income, assets, or all",
        required: false,
      },
    ],
  },
  {
    name: "write_query",
    description: "Help write a BQL query based on natural language description",
    arguments: [
      {
        name: "description",
        description: "What you want to query in plain English",
        required: true,
      },
    ],
  },
  {
    name: "categorize_transaction",
    description: "Help categorize a transaction with appropriate accounts",
    arguments: [
      {
        name: "description",
        description: "Description of the transaction (payee, amount, context)",
        required: true,
      },
    ],
  },
];

server.setRequestHandler(ListPromptsRequestSchema, async () => {
  return { prompts: PROMPTS };
});

server.setRequestHandler(GetPromptRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  switch (name) {
    case "analyze_ledger": {
      const focus = args?.focus || "all";
      return {
        messages: [
          {
            role: "user",
            content: {
              type: "text",
              text: `Please analyze this Beancount ledger with a focus on ${focus}.

Use the following tools to gather information:
1. First use \`ledger_stats\` to get an overview
2. Use \`list_accounts\` to understand the account structure
3. Run appropriate BQL queries to analyze ${focus === "spending" ? "Expenses" : focus === "income" ? "Income" : focus === "assets" ? "Assets" : "all accounts"}
4. Look for any validation errors

Provide insights on:
- Overall financial health
- Spending patterns (if applicable)
- Account organization
- Any potential issues or improvements`,
            },
          },
        ],
      };
    }

    case "write_query": {
      const description = args?.description;
      if (!description) {
        throw new Error("Missing required argument: description");
      }
      return {
        messages: [
          {
            role: "user",
            content: {
              type: "text",
              text: `Help me write a BQL (Beancount Query Language) query for the following:

"${description}"

Please:
1. Write the BQL query
2. Explain what each part does
3. Provide any variations that might be useful

Reference the BQL documentation if needed using the rustledger://docs/bql resource.`,
            },
          },
        ],
      };
    }

    case "categorize_transaction": {
      const description = args?.description;
      if (!description) {
        throw new Error("Missing required argument: description");
      }
      return {
        messages: [
          {
            role: "user",
            content: {
              type: "text",
              text: `Help me categorize this transaction in Beancount format:

"${description}"

Please:
1. Suggest appropriate account names following Beancount conventions
2. Provide the full transaction entry
3. Explain the categorization choice
4. Suggest any relevant tags or links

If you have access to an existing ledger, use \`list_accounts\` to match existing account naming conventions.`,
            },
          },
        ],
      };
    }

    default:
      throw new Error(`Unknown prompt: ${name}`);
  }
});

// Helper functions
interface BeancountError {
  message: string;
  line?: number;
  column?: number;
  severity: "error" | "warning";
}

function formatErrors(errors: BeancountError[]): string {
  return errors
    .map((e) => {
      const loc = e.line ? `:${e.line}${e.column ? `:${e.column}` : ""}` : "";
      return `[${e.severity}]${loc} ${e.message}`;
    })
    .join("\n");
}

interface QueryResult {
  columns: string[];
  rows: unknown[][];
}

function formatQueryResult(result: QueryResult): string {
  if (!result.columns || result.columns.length === 0) {
    return "No results.";
  }

  const { columns, rows } = result;

  // Calculate column widths
  const widths = columns.map((col, i) => {
    const maxRowWidth = Math.max(
      ...rows.map((row) => formatCell(row[i]).length)
    );
    return Math.max(col.length, maxRowWidth);
  });

  // Format header
  const header = columns.map((col, i) => col.padEnd(widths[i])).join(" | ");
  const separator = widths.map((w) => "-".repeat(w)).join("-+-");

  // Format rows
  const formattedRows = rows.map((row) =>
    row.map((cell, i) => formatCell(cell).padEnd(widths[i])).join(" | ")
  );

  return [header, separator, ...formattedRows].join("\n");
}

function formatCell(value: unknown): string {
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "object") {
    // Handle Amount type
    if ("number" in value && "currency" in value) {
      const amount = value as { number: string; currency: string };
      return `${amount.number} ${amount.currency}`;
    }
    // Handle Inventory type
    if ("positions" in value) {
      const inv = value as { positions: Array<{ units: { number: string; currency: string } }> };
      return inv.positions
        .map((p) => `${p.units.number} ${p.units.currency}`)
        .join(", ");
    }
    return JSON.stringify(value);
  }
  return String(value);
}

// Documentation constants
const BQL_DOCS = `# BQL - Beancount Query Language

BQL is a SQL-like query language for querying Beancount ledgers.

## Basic Syntax

\`\`\`sql
SELECT [DISTINCT] <target-spec>, ...
[FROM <from-spec>]
[WHERE <where-expression>]
[GROUP BY <group-spec>, ...]
[ORDER BY <order-spec>, ...]
[LIMIT <limit>]
\`\`\`

## Common Queries

### Account Balances
\`\`\`sql
BALANCES
-- or equivalently:
SELECT account, sum(position) GROUP BY account
\`\`\`

### Filter by Account
\`\`\`sql
SELECT date, narration, position
WHERE account ~ "Expenses:Food"
\`\`\`

### Filter by Date Range
\`\`\`sql
SELECT date, account, position
WHERE date >= 2024-01-01 AND date < 2024-02-01
\`\`\`

### Monthly Summary
\`\`\`sql
SELECT year(date), month(date), sum(position)
WHERE account ~ "Expenses"
GROUP BY year(date), month(date)
ORDER BY year(date), month(date)
\`\`\`

### Journal Entries
\`\`\`sql
JOURNAL "Assets:Checking"
\`\`\`

## Available Functions

- \`year(date)\`, \`month(date)\`, \`day(date)\` - Extract date parts
- \`sum(position)\` - Sum positions
- \`count()\` - Count entries
- \`first(x)\`, \`last(x)\` - First/last values
- \`min(x)\`, \`max(x)\` - Min/max values

## Operators

- \`~\` - Regex match (e.g., \`account ~ "Expenses:.*"\`)
- \`=\`, \`!=\`, \`<\`, \`>\`, \`<=\`, \`>=\` - Comparisons
- \`AND\`, \`OR\`, \`NOT\` - Boolean operators
`;

const BQL_TABLES_DOCS = `# BQL Tables

BQL queries run against these implicit tables:

## entries (default)
The main table containing all postings from transactions.

| Column | Type | Description |
|--------|------|-------------|
| date | date | Transaction date |
| flag | string | Transaction flag (* or !) |
| payee | string | Transaction payee |
| narration | string | Transaction narration |
| account | string | Posting account |
| position | position | Posting amount with cost |
| balance | inventory | Running balance |
| tags | set | Transaction tags |
| links | set | Transaction links |

## balances
Pre-aggregated account balances.

| Column | Type | Description |
|--------|------|-------------|
| account | string | Account name |
| balance | inventory | Total balance |
`;

const VALIDATION_ERRORS_DOCS = `# Validation Errors Reference

rustledger validates ledgers and reports these error types:

## Account Errors
- **E0001**: Account not opened before use
- **E0002**: Account already opened
- **E0003**: Account closed but still has transactions
- **E0004**: Invalid account name format

## Balance Errors
- **E0101**: Balance assertion failed
- **E0102**: Negative balance not allowed

## Transaction Errors
- **E0201**: Transaction does not balance
- **E0202**: Missing posting amount (only one allowed)
- **E0203**: Currency mismatch in transaction

## Booking Errors
- **E0301**: Ambiguous lot matching
- **E0302**: Insufficient lots for reduction
- **E0303**: Cost basis mismatch

## Date Errors
- **E0401**: Future date not allowed
- **E0402**: Date out of order

## Document/Note Errors
- **E0501**: Document file not found
- **E0502**: Invalid document path

## Plugin Errors
- **E0601**: Plugin not found
- **E0602**: Plugin execution error

## Parse Errors
- **E0701**: Syntax error
- **E0702**: Invalid directive format
- **E0703**: Duplicate option
`;

const BQL_FUNCTIONS_DOCS = `# BQL Functions Reference

## Aggregate Functions
| Function | Description |
|----------|-------------|
| \`sum(x)\` | Sum of values |
| \`count()\` | Count of rows |
| \`first(x)\` | First value |
| \`last(x)\` | Last value |
| \`min(x)\` | Minimum value |
| \`max(x)\` | Maximum value |

## Date Functions
| Function | Description |
|----------|-------------|
| \`year(date)\` | Extract year (integer) |
| \`month(date)\` | Extract month (1-12) |
| \`day(date)\` | Extract day of month |
| \`quarter(date)\` | Extract quarter (1-4) |
| \`weekday(date)\` | Day of week (0=Monday) |

## String Functions
| Function | Description |
|----------|-------------|
| \`length(s)\` | String length |
| \`upper(s)\` | Uppercase string |
| \`lower(s)\` | Lowercase string |

## Account Functions
| Function | Description |
|----------|-------------|
| \`root(account, n)\` | First n components |
| \`leaf(account)\` | Last component |
| \`parent(account)\` | Parent account |

## Conversion Functions
| Function | Description |
|----------|-------------|
| \`cost(position)\` | Convert to cost basis |
| \`value(position)\` | Market value (needs prices) |
| \`units(position)\` | Just the units |
`;

const DIRECTIVES_DOCS = `# Beancount Directives Reference

## Transaction
\`\`\`beancount
2024-01-15 * "Payee" "Narration"
  Assets:Checking  -50.00 USD
  Expenses:Food     50.00 USD
\`\`\`

## Open Account
\`\`\`beancount
2024-01-01 open Assets:Checking USD,EUR
\`\`\`

## Close Account
\`\`\`beancount
2024-12-31 close Assets:OldAccount
\`\`\`

## Balance Assertion
\`\`\`beancount
2024-01-31 balance Assets:Checking 1000.00 USD
\`\`\`

## Pad
\`\`\`beancount
2024-01-01 pad Assets:Checking Equity:Opening-Balances
\`\`\`

## Commodity
\`\`\`beancount
2024-01-01 commodity USD
  name: "US Dollar"
\`\`\`

## Price
\`\`\`beancount
2024-01-15 price AAPL 185.50 USD
\`\`\`

## Event
\`\`\`beancount
2024-01-01 event "location" "New York"
\`\`\`

## Note
\`\`\`beancount
2024-01-15 note Assets:Checking "Called bank about fees"
\`\`\`

## Document
\`\`\`beancount
2024-01-15 document Assets:Checking "/path/to/statement.pdf"
\`\`\`

## Query
\`\`\`beancount
2024-01-01 query "monthly-expenses" "
  SELECT month, sum(position) WHERE account ~ 'Expenses'
  GROUP BY month
"
\`\`\`

## Custom
\`\`\`beancount
2024-01-01 custom "budget" "Expenses:Food" 500 USD
\`\`\`
`;

// Start the server
async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error(`rustledger MCP server v${rustledger.version()} started`);
}

main().catch((error) => {
  console.error("Fatal error:", error);
  process.exit(1);
});

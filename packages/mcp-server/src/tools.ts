// Tool definitions for the MCP server

export interface ToolDefinition {
  name: string;
  description: string;
  inputSchema: {
    type: "object";
    properties: Record<string, unknown>;
    required: string[];
  };
}

// === Original Tools ===

export const validateTool: ToolDefinition = {
  name: "validate",
  description:
    "Validate a Beancount ledger. Returns validation errors and warnings if any issues are found.",
  inputSchema: {
    type: "object",
    properties: {
      source: {
        type: "string",
        description: "The Beancount ledger source text to validate",
      },
    },
    required: ["source"],
  },
};

export const queryTool: ToolDefinition = {
  name: "query",
  description:
    "Run a BQL (Beancount Query Language) query on a ledger. Use this for account balances, filtering transactions, aggregations, etc.",
  inputSchema: {
    type: "object",
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
};

export const balancesTool: ToolDefinition = {
  name: "balances",
  description:
    "Get account balances from a ledger. This is a shorthand for running a BALANCES query.",
  inputSchema: {
    type: "object",
    properties: {
      source: {
        type: "string",
        description: "The Beancount ledger source text",
      },
    },
    required: ["source"],
  },
};

export const formatTool: ToolDefinition = {
  name: "format",
  description:
    "Format a Beancount ledger with consistent alignment and spacing.",
  inputSchema: {
    type: "object",
    properties: {
      source: {
        type: "string",
        description: "The Beancount ledger source text to format",
      },
    },
    required: ["source"],
  },
};

export const parseTool: ToolDefinition = {
  name: "parse",
  description:
    "Parse a Beancount ledger and return the directives as structured data.",
  inputSchema: {
    type: "object",
    properties: {
      source: {
        type: "string",
        description: "The Beancount ledger source text to parse",
      },
    },
    required: ["source"],
  },
};

export const completionsTool: ToolDefinition = {
  name: "completions",
  description:
    "Get BQL query completions at a cursor position. Useful for building query editors.",
  inputSchema: {
    type: "object",
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
};

export const listPluginsTool: ToolDefinition = {
  name: "list_plugins",
  description: "List available native plugins that can process ledgers.",
  inputSchema: {
    type: "object",
    properties: {},
    required: [],
  },
};

export const runPluginTool: ToolDefinition = {
  name: "run_plugin",
  description: "Run a native plugin on a ledger.",
  inputSchema: {
    type: "object",
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
};

// === Editor Tools (LSP-like) ===

export const editorCompletionsTool: ToolDefinition = {
  name: "editor_completions",
  description:
    "Get context-aware completions for Beancount source at a given position. Returns account names, currencies, directives, etc.",
  inputSchema: {
    type: "object",
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
};

export const editorHoverTool: ToolDefinition = {
  name: "editor_hover",
  description:
    "Get hover information for a symbol at the given position. Returns documentation for accounts, currencies, and directives.",
  inputSchema: {
    type: "object",
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
};

export const editorDefinitionTool: ToolDefinition = {
  name: "editor_definition",
  description:
    "Get the definition location for a symbol (account or currency). Returns the location of the Open or Commodity directive.",
  inputSchema: {
    type: "object",
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
};

export const editorDocumentSymbolsTool: ToolDefinition = {
  name: "editor_document_symbols",
  description:
    "Get all symbols in the document for an outline/structure view. Returns all directives with their positions.",
  inputSchema: {
    type: "object",
    properties: {
      source: {
        type: "string",
        description: "The Beancount ledger source text",
      },
    },
    required: ["source"],
  },
};

export const editorReferencesTool: ToolDefinition = {
  name: "editor_references",
  description:
    "Find all references to a symbol (account, currency, or payee) at the given position. Returns all locations where the symbol is used.",
  inputSchema: {
    type: "object",
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
};

// === Analysis Tools ===

export const ledgerStatsTool: ToolDefinition = {
  name: "ledger_stats",
  description:
    "Get statistics about a ledger: counts of directives, accounts, currencies, date range, etc.",
  inputSchema: {
    type: "object",
    properties: {
      source: {
        type: "string",
        description: "The Beancount ledger source text",
      },
    },
    required: ["source"],
  },
};

export const listAccountsTool: ToolDefinition = {
  name: "list_accounts",
  description:
    "List all accounts in the ledger with their open/close dates and currencies.",
  inputSchema: {
    type: "object",
    properties: {
      source: {
        type: "string",
        description: "The Beancount ledger source text",
      },
    },
    required: ["source"],
  },
};

export const listCommoditiesTool: ToolDefinition = {
  name: "list_commodities",
  description: "List all currencies/commodities used in the ledger.",
  inputSchema: {
    type: "object",
    properties: {
      source: {
        type: "string",
        description: "The Beancount ledger source text",
      },
    },
    required: ["source"],
  },
};

export const accountActivityTool: ToolDefinition = {
  name: "account_activity",
  description:
    "Get activity summary for a specific account: first/last transaction, total transactions, currencies used.",
  inputSchema: {
    type: "object",
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
};

// === Utility Tools ===

export const formatCheckTool: ToolDefinition = {
  name: "format_check",
  description:
    "Check if a ledger is properly formatted. Returns differences if formatting would change the file.",
  inputSchema: {
    type: "object",
    properties: {
      source: {
        type: "string",
        description: "The Beancount ledger source text",
      },
    },
    required: ["source"],
  },
};

export const bqlTablesTool: ToolDefinition = {
  name: "bql_tables",
  description:
    "Get documentation for BQL tables (entries, postings, etc.) and their available columns.",
  inputSchema: {
    type: "object",
    properties: {},
    required: [],
  },
};

export const directiveAtLineTool: ToolDefinition = {
  name: "directive_at_line",
  description: "Get the directive at a specific line number in the source.",
  inputSchema: {
    type: "object",
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
};

export const findTransactionsTool: ToolDefinition = {
  name: "find_transactions",
  description:
    "Find transactions matching criteria: payee, narration, tags, or date range.",
  inputSchema: {
    type: "object",
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
};

// === Report Tool ===

export const reportTool: ToolDefinition = {
  name: "report",
  description:
    "Generate financial reports: balance sheet, income statement, holdings, or net worth.",
  inputSchema: {
    type: "object",
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
};

// === File Operation Tools ===

export const validateFileTool: ToolDefinition = {
  name: "validate_file",
  description:
    "Validate a Beancount file from the filesystem. Handles includes automatically.",
  inputSchema: {
    type: "object",
    properties: {
      file_path: {
        type: "string",
        description: "Path to the Beancount file to validate",
      },
    },
    required: ["file_path"],
  },
};

export const queryFileTool: ToolDefinition = {
  name: "query_file",
  description: "Run a BQL query on a Beancount file from the filesystem.",
  inputSchema: {
    type: "object",
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
};

export const formatFileTool: ToolDefinition = {
  name: "format_file",
  description:
    "Format a Beancount file. Can optionally write the formatted output back to the file.",
  inputSchema: {
    type: "object",
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
};

// All tools combined
export const TOOLS: ToolDefinition[] = [
  // Original Tools
  validateTool,
  queryTool,
  balancesTool,
  formatTool,
  parseTool,
  completionsTool,
  listPluginsTool,
  runPluginTool,
  // Editor Tools
  editorCompletionsTool,
  editorHoverTool,
  editorDefinitionTool,
  editorDocumentSymbolsTool,
  editorReferencesTool,
  // Analysis Tools
  ledgerStatsTool,
  listAccountsTool,
  listCommoditiesTool,
  accountActivityTool,
  // Utility Tools
  formatCheckTool,
  bqlTablesTool,
  directiveAtLineTool,
  findTransactionsTool,
  // Report Tool
  reportTool,
  // File Operation Tools
  validateFileTool,
  queryFileTool,
  formatFileTool,
];

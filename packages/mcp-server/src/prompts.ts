// Prompts for the MCP server

export interface PromptArgument {
  name: string;
  description: string;
  required: boolean;
}

export interface PromptDefinition {
  name: string;
  description: string;
  arguments: PromptArgument[];
}

export interface PromptMessage {
  role: "user" | "assistant";
  content: {
    type: "text";
    text: string;
  };
}

export interface PromptResult {
  messages: PromptMessage[];
  [key: string]: unknown;
}

export const PROMPTS: PromptDefinition[] = [
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

export function getPrompt(
  name: string,
  args?: Record<string, string>
): PromptResult {
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
}

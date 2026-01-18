// Helper functions for the MCP server

import type {
  BeancountError,
  QueryResult,
  ToolResponse,
  ToolArguments,
} from "./types.js";

/**
 * Validate that required arguments are present.
 * Returns a ToolResponse with error if validation fails, null otherwise.
 */
export function validateArgs(
  args: ToolArguments | undefined,
  required: (keyof ToolArguments)[]
): ToolResponse | null {
  const missing: string[] = [];

  for (const key of required) {
    const value = args?.[key];
    // Check for undefined, null, or empty string for string types
    if (value === undefined || value === null) {
      missing.push(key);
    }
  }

  if (missing.length > 0) {
    const argList = missing.join(", ");
    return {
      isError: true,
      content: [
        {
          type: "text",
          text: `Missing required argument${missing.length > 1 ? "s" : ""}: ${argList}`,
        },
      ],
    };
  }

  return null;
}

/**
 * Create an error response.
 */
export function errorResponse(message: string): ToolResponse {
  return {
    isError: true,
    content: [{ type: "text", text: message }],
  };
}

/**
 * Create a success response with text content.
 */
export function textResponse(text: string): ToolResponse {
  return {
    content: [{ type: "text", text }],
  };
}

/**
 * Create a success response with JSON content.
 */
export function jsonResponse(data: unknown): ToolResponse {
  return {
    content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
  };
}

/**
 * Format validation/parse errors for display.
 */
export function formatErrors(errors: BeancountError[]): string {
  return errors
    .map((e) => {
      const loc = e.line ? `:${e.line}${e.column ? `:${e.column}` : ""}` : "";
      return `[${e.severity}]${loc} ${e.message}`;
    })
    .join("\n");
}

/**
 * Format a query result as a table.
 */
export function formatQueryResult(result: QueryResult): string {
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

/**
 * Format a single cell value for display.
 */
export function formatCell(value: unknown): string {
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
      const inv = value as {
        positions: Array<{ units: { number: string; currency: string } }>;
      };
      return inv.positions
        .map((p) => `${p.units.number} ${p.units.currency}`)
        .join(", ");
    }
    return JSON.stringify(value);
  }
  return String(value);
}

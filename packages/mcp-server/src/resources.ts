// Resources for the MCP server

import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load documentation from markdown files
function loadDoc(filename: string): string {
  // When running from dist/, docs are in dist/docs/
  const docPath = path.join(__dirname, "docs", filename);
  try {
    return fs.readFileSync(docPath, "utf-8");
  } catch {
    return `Documentation not found: ${filename}`;
  }
}

// Lazy-load documentation
let _bqlDocs: string | null = null;
let _bqlTablesDocs: string | null = null;
let _validationErrorsDocs: string | null = null;
let _bqlFunctionsDocs: string | null = null;
let _directivesDocs: string | null = null;

export function getBqlDocs(): string {
  if (_bqlDocs === null) {
    _bqlDocs = loadDoc("bql.md");
  }
  return _bqlDocs;
}

export function getBqlTablesDocs(): string {
  if (_bqlTablesDocs === null) {
    _bqlTablesDocs = loadDoc("bql-tables.md");
  }
  return _bqlTablesDocs;
}

export function getValidationErrorsDocs(): string {
  if (_validationErrorsDocs === null) {
    _validationErrorsDocs = loadDoc("validation-errors.md");
  }
  return _validationErrorsDocs;
}

export function getBqlFunctionsDocs(): string {
  if (_bqlFunctionsDocs === null) {
    _bqlFunctionsDocs = loadDoc("bql-functions.md");
  }
  return _bqlFunctionsDocs;
}

export function getDirectivesDocs(): string {
  if (_directivesDocs === null) {
    _directivesDocs = loadDoc("directives.md");
  }
  return _directivesDocs;
}

export interface ResourceDefinition {
  uri: string;
  name: string;
  description: string;
  mimeType: string;
}

export const RESOURCES: ResourceDefinition[] = [
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

export interface ResourceContents {
  uri: string;
  mimeType: string;
  text: string;
}

export function getResourceContents(uri: string): ResourceContents | null {
  switch (uri) {
    case "rustledger://docs/bql":
      return { uri, mimeType: "text/markdown", text: getBqlDocs() };
    case "rustledger://docs/validation-errors":
      return { uri, mimeType: "text/markdown", text: getValidationErrorsDocs() };
    case "rustledger://docs/bql-functions":
      return { uri, mimeType: "text/markdown", text: getBqlFunctionsDocs() };
    case "rustledger://docs/directives":
      return { uri, mimeType: "text/markdown", text: getDirectivesDocs() };
    default:
      return null;
  }
}

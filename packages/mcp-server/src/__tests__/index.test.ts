import { describe, it, expect, beforeAll } from 'vitest';
import * as rustledger from '@rustledger/wasm';

// Initialize WASM before tests
beforeAll(() => {
  rustledger.init();
});

// Sample ledger for testing
const SAMPLE_LEDGER = `
2024-01-01 open Assets:Checking USD
2024-01-01 open Expenses:Food USD
2024-01-01 open Income:Salary USD

2024-01-15 * "Grocery Store" "Weekly groceries"
  Expenses:Food     50.00 USD
  Assets:Checking  -50.00 USD

2024-01-31 * "Employer" "January salary"
  Assets:Checking  5000.00 USD
  Income:Salary   -5000.00 USD

2024-01-31 balance Assets:Checking 4950.00 USD
`;

describe('rustledger WASM bindings', () => {
  describe('validateSource', () => {
    it('should validate a correct ledger', () => {
      const result = rustledger.validateSource(SAMPLE_LEDGER);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should report errors for invalid ledger', () => {
      const invalidLedger = `
2024-01-15 * "Test"
  Expenses:Food  100 USD
  Assets:Checking
`;
      const result = rustledger.validateSource(invalidLedger);
      expect(result.valid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe('query', () => {
    it('should execute BALANCES query', () => {
      const result = rustledger.query(SAMPLE_LEDGER, 'BALANCES');
      expect(result.errors).toHaveLength(0);
      expect(result.columns).toContain('account');
    });

    it('should filter by account', () => {
      const result = rustledger.query(
        SAMPLE_LEDGER,
        'SELECT account, sum(position) WHERE account ~ "Expenses" GROUP BY account'
      );
      expect(result.errors).toHaveLength(0);
      expect(result.rows.length).toBeGreaterThan(0);
    });

    it('should report query errors', () => {
      const result = rustledger.query(SAMPLE_LEDGER, 'INVALID QUERY');
      expect(result.errors.length).toBeGreaterThan(0);
    });
  });

  describe('format', () => {
    it('should format a ledger', () => {
      const result = rustledger.format(SAMPLE_LEDGER);
      expect(result.errors).toHaveLength(0);
      expect(result.formatted).toBeDefined();
      expect(result.formatted!.length).toBeGreaterThan(0);
    });
  });

  describe('parse', () => {
    it('should parse a ledger into directives', () => {
      const result = rustledger.parse(SAMPLE_LEDGER);
      expect(result.errors).toHaveLength(0);
      expect(result.ledger).toBeDefined();
      expect(result.ledger!.directives.length).toBeGreaterThan(0);
    });

    it('should parse different directive types', () => {
      const result = rustledger.parse(SAMPLE_LEDGER);
      const directives = result.ledger!.directives;

      const types = directives.map((d: { type: string }) => d.type);
      expect(types).toContain('open');
      expect(types).toContain('transaction');
      expect(types).toContain('balance');
    });
  });

  describe('listPlugins', () => {
    it('should return available plugins', () => {
      const plugins = rustledger.listPlugins();
      expect(Array.isArray(plugins)).toBe(true);
    });
  });

  describe('bqlCompletions', () => {
    it('should return completions for partial query', () => {
      const result = rustledger.bqlCompletions('SEL', 3);
      expect(result.completions).toBeDefined();
      expect(Array.isArray(result.completions)).toBe(true);
    });
  });
});

describe('ParsedLedger class', () => {
  it('should parse and validate a ledger', () => {
    const ledger = new rustledger.ParsedLedger(SAMPLE_LEDGER);
    expect(ledger.isValid()).toBe(true);
    expect(ledger.getErrors()).toHaveLength(0);
    ledger.free();
  });

  it('should get directives', () => {
    const ledger = new rustledger.ParsedLedger(SAMPLE_LEDGER);
    const directives = ledger.getDirectives();
    expect(directives.length).toBeGreaterThan(0);
    ledger.free();
  });

  it('should run queries', () => {
    const ledger = new rustledger.ParsedLedger(SAMPLE_LEDGER);
    const result = ledger.query('BALANCES');
    expect(result.errors).toHaveLength(0);
    expect(result.columns).toBeDefined();
    ledger.free();
  });

  it('should get document symbols', () => {
    const ledger = new rustledger.ParsedLedger(SAMPLE_LEDGER);
    const symbols = ledger.getDocumentSymbols();
    expect(Array.isArray(symbols)).toBe(true);
    expect(symbols.length).toBeGreaterThan(0);
    ledger.free();
  });

  it('should get completions at position', () => {
    const ledger = new rustledger.ParsedLedger(SAMPLE_LEDGER);
    // Line 5 (0-indexed: 4), character 2 - at the start of a line
    const result = ledger.getCompletions(4, 2);
    expect(result).toBeDefined();
    expect(result.completions).toBeDefined();
    ledger.free();
  });

  it('should get hover info for account', () => {
    const ledger = new rustledger.ParsedLedger(SAMPLE_LEDGER);
    // Try to get hover on an account name
    const result = ledger.getHoverInfo(5, 10);
    // Hover may or may not return info depending on position
    expect(result === null || typeof result === 'object').toBe(true);
    ledger.free();
  });

  it('should format the ledger', () => {
    const ledger = new rustledger.ParsedLedger(SAMPLE_LEDGER);
    const result = ledger.format();
    expect(result.formatted).toBeDefined();
    ledger.free();
  });
});

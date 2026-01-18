# BQL - Beancount Query Language

BQL is a SQL-like query language for querying Beancount ledgers.

## Basic Syntax

```sql
SELECT [DISTINCT] <target-spec>, ...
[FROM <from-spec>]
[WHERE <where-expression>]
[GROUP BY <group-spec>, ...]
[ORDER BY <order-spec>, ...]
[LIMIT <limit>]
```

## Common Queries

### Account Balances
```sql
BALANCES
-- or equivalently:
SELECT account, sum(position) GROUP BY account
```

### Filter by Account
```sql
SELECT date, narration, position
WHERE account ~ "Expenses:Food"
```

### Filter by Date Range
```sql
SELECT date, account, position
WHERE date >= 2024-01-01 AND date < 2024-02-01
```

### Monthly Summary
```sql
SELECT year(date), month(date), sum(position)
WHERE account ~ "Expenses"
GROUP BY year(date), month(date)
ORDER BY year(date), month(date)
```

### Journal Entries
```sql
JOURNAL "Assets:Checking"
```

## Available Functions

- `year(date)`, `month(date)`, `day(date)` - Extract date parts
- `sum(position)` - Sum positions
- `count()` - Count entries
- `first(x)`, `last(x)` - First/last values
- `min(x)`, `max(x)` - Min/max values

## Operators

- `~` - Regex match (e.g., `account ~ "Expenses:.*"`)
- `=`, `!=`, `<`, `>`, `<=`, `>=` - Comparisons
- `AND`, `OR`, `NOT` - Boolean operators

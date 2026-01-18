# rustledger-query

Beancount Query Language (BQL) engine with SQL-like syntax.

## Supported Syntax

```sql
SELECT account, SUM(position)
WHERE account ~ 'Expenses:'
GROUP BY account
ORDER BY SUM(position) DESC
LIMIT 10
```

## Features

- Full BQL support (SELECT, FROM, WHERE, GROUP BY, ORDER BY, LIMIT)
- Regex pattern matching (`~` operator)
- Aggregate functions (SUM, COUNT, FIRST, LAST, MIN, MAX)
- Date functions (YEAR, MONTH, DAY, QUARTER)
- String functions (LENGTH, UPPER, LOWER)
- Subqueries and PIVOT tables

## Example

```rust
use rustledger_query::{parse_query, execute};

let query = parse_query("SELECT account, SUM(position) GROUP BY account")?;
let results = execute(&query, &entries)?;

for row in results {
    println!("{:?}", row);
}
```

## License

GPL-3.0

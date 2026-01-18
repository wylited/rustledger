# Beancount Directives Reference

## Transaction
```beancount
2024-01-15 * "Payee" "Narration"
  Assets:Checking  -50.00 USD
  Expenses:Food     50.00 USD
```

## Open Account
```beancount
2024-01-01 open Assets:Checking USD,EUR
```

## Close Account
```beancount
2024-12-31 close Assets:OldAccount
```

## Balance Assertion
```beancount
2024-01-31 balance Assets:Checking 1000.00 USD
```

## Pad
```beancount
2024-01-01 pad Assets:Checking Equity:Opening-Balances
```

## Commodity
```beancount
2024-01-01 commodity USD
  name: "US Dollar"
```

## Price
```beancount
2024-01-15 price AAPL 185.50 USD
```

## Event
```beancount
2024-01-01 event "location" "New York"
```

## Note
```beancount
2024-01-15 note Assets:Checking "Called bank about fees"
```

## Document
```beancount
2024-01-15 document Assets:Checking "/path/to/statement.pdf"
```

## Query
```beancount
2024-01-01 query "monthly-expenses" "
  SELECT month, sum(position) WHERE account ~ 'Expenses'
  GROUP BY month
"
```

## Custom
```beancount
2024-01-01 custom "budget" "Expenses:Food" 500 USD
```

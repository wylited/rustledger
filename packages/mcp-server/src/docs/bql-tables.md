# BQL Tables

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

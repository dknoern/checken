# Checken mechanics sheet

## Import (File > Import > Quicken CSV File… / Mint.com File (CSV)…)

### Where the rules come from

The Quicken app's executable is a small launcher; the code lives in
`Quicken.app/Contents/Frameworks/QFramework.framework/QFramework` (a universal binary).
Ghidra's decompiler can't run on this Mac (its macOS download leaves out the decompiler
program), so functions were read with the built-in decompiler (arm64 addresses) and, because
that one shows Objective-C calls only as numbers, with the system disassembler on the Intel
copy of the same code, which names every selector and string (x86_64 addresses).

| Function | arm64 | x86_64 | What it tells us |
|---|---|---|---|
| `-[CSVParser initWithString:separator:hasHeader:fieldNames:]` | — | 0xe8179e | Separator may not be empty, `"` or a line break |
| `-[CSVParser parseRecord]` / `parseField` / `parseEscaped` / `parseTextData` / `parseLineSeparator` | — | 0xe81ebb / 0xe820df / 0xe82201 / 0xe824cd / 0xe82450 | Standard quoted CSV (below) |
| `-[CSVParser parseHeader]` | — | 0xe81dcc | First line holds the column names |
| `BaseImportController.defaultParseCSV(stringContents:)` | — | 0x18289e0 | Transaction import uses separator "," and hasHeader = YES |
| `CSVRowProcessor.Result.resultTrimmedAndCoalescedWhitespace()` | — | 0x1828c30 | Values trimmed, runs of whitespace made one space |
| `-[SecurityPriceHistoryCSVImportWindowController parseFileURL:withError:]` | 0x4f3fa0 (ev_6b583a99…) | 0x673d98 | Quicken's readable CSV import: header names compared ignoring case; date formats and their order; grouping separator removed before reading amounts; error messages |
| sample-CSV writer (Swift, name stripped) | — | 0x187efc0 | Transaction CSV columns: Date, Payee, FI Payee, Amount, Transaction Type, Category, Account, Tags, Notes |
| `-[FlexReportSnapshotRenderer_Transaction renderToCSVorClipboardUsingSeparator:]` | 0x6104d8 (ev_b6d07b3c…) | 0x7ec17e | Writes the register export the person uploaded |
| `-[CSVExporter_OLD writeFooter:]` etc. | 0x88d1d4 | 0xb299a6 | Export footer "Total Inflows:/Total Outflows:/Net Total:%@%10.2lf" |

Strings (`__cstring`, 0x17d7000 and nearby, ev_164b358e…): "MM/dd/yy (Required)", "Amount
(Required)", "\"debit\" or \"credit\". \"debit\" will negate the amount and \"credit\" will
leave the amount positive. Importer will use \"credit\" if left blank", "Tags (separate with
spaces)", "FI Payee … not imported", "Unable to interpret the Date format used: \"%@\" (Row
1).", "Edit the CSV file to use a format like 12/31/2021.", "Edit the CSV file to use a format
like 123.45.", "The provided CSV header was invalid.", "Edit the header strings in the CSV
file.", "No data rows were found in the provided file."

### Rules (Quicken's, as Checken implements them)

- **CSV reading** (CSVParser): fields are separated by ","; a field in double quotes may hold
  commas and line breaks, and `""` inside it is one quote; any run of line-break characters
  ends a record, so blank lines are skipped. A record with more fields than the header gets
  extra columns (`FIELD_%ld`), which are ignored.
- **Header**: column names are matched ignoring case (`caseInsensitiveCompare:`). Quicken's
  transaction importer wants the header on the first line. Quicken's own register export
  starts with a title and filter lines, so Checken searches for the first line holding both
  Date and Amount. *(Checken extension)* No such line → "The provided CSV header was invalid…
  Edit the header strings in the CSV file."
- **Columns**: Date, Payee, FI Payee (not imported), Amount, Transaction Type, Category,
  Account, Tags, Notes (0x187efc0). Checken also accepts Mint's Description/Account Name/Labels
  and an export's Scheduled, Split, Check #, Status, Memo. *(Checken extension)*
- **Text**: every value is trimmed and runs of whitespace are made one space (0x1828c30).
- **Dates** (0x673d98): try `M/d/yy`, `M/d/yyyy`, `d-MMM-yy`, `yyyy-MM-dd`, `yyyy/MM/dd` in
  that order on the first data row, use the first that works for every row, in UTC. A row the
  chosen format can't read stops the import: "Unable to interpret the Date format used: "…"
  (line n). Edit the CSV file to use a format like 12/31/2021." A `yy` year of two digits lands
  in the 80 years before / 20 years after today (the date formatter's default; Quicken sets no
  `twoDigitStartDate`); four digits are read as written, so `M/d/yy` also reads 11/19/2026.
- **Amounts** (0x673d98): the grouping separator (`NSLocaleGroupingSeparator`, "," in the US)
  is removed and the rest read as a decimal number. An unreadable amount stops the import:
  "… Edit the CSV file to use a format like 123.45." Checken also allows a leading "$" and
  "(45.00)" for negatives. *(Checken extension)*
- **Transaction Type**: "debit" negates the amount, "credit" or blank leaves it (string above).
- **Lines with no date** (the export's "Total Inflows:", "Total Outflows:", "Net Total:" lines
  from 0x88d1d4/0xb299a6, and its title lines) are not transactions and are skipped.
- **No transactions** → "No data rows were found in the provided file."
- **Tags** go into the memo as "Tags: …" (Checken has no tags).

### Rules of Checken's own (Quicken's transaction-import code past the parser is stripped Swift; not recovered)

These were looked for and not found readable in the code, so they are Checken's choices:
- Lines marked Scheduled are left out (haven't happened). *estimated*
- Without a Status column, imported lines are Cleared. *estimated*
- Each file account goes into a same-named account or a new one; kind guessed from the name,
  then the report title ("Checking Report" → Checking). *estimated*
- "Live Opening Balance"/"Adjustment" lines become a new account's starting balance. *from the sample*
- Transfer lines (Transfer, Transfer:…, Credit Card Payment, [Account]) pair across accounts on
  the same day, then within 3 days, into one linked transfer; they stay out of spending,
  income, budgets and reports. *estimated window*
- Repeats are skipped: same account, date and amount (payees aren't compared: an "All
  Transactions" export names the column "Payee/Security", now read as Payee). Each existing
  transaction stands in for one line, so true repeats inside a file still import. *estimated*
- Importing into a hidden (closed) account shows it again; hidden accounts are offered in the
  preview's "Import into" list.
- File > Import > Remove Duplicate Imports… removes copies with no payee that match a
  transaction with a payee on the same day, account, amount and category (or the same
  transfer), at most one per named copy, after a safety copy.
- New categories are Income when their lines add up to money in, else Expense. *estimated*
- A safety copy ("ledger before import <time>.json") is written first.

Check against the sample: after import the five accounts total about $19,408.79, the same as
the export's own "Net Total: 19408.79".

Unknown: how the export marks split and scheduled transactions (the sample has none).

## Plaid bank connections (File > Connect a Bank Account / Update All Bank Accounts ⌘R; sidebar ⟳)

Sources: the person's Plaid quickstart server (`~/workspace/quickstart/node/index.js`, upgraded
2026-11-22 so access tokens stay on the server, as Plaid's Quickstart advises) and Plaid's API
docs. Quicken's own bank download (its sidebar ⟳ button) is the model for the behaviour.

Server (quickstart node/index.js):
- Access tokens never leave the server. `set_access_token` stores the token in
  `node/.data/plaid-items.json` (folder 0700, file 0600, git-ignored) and answers `{item_id}`
  only; `/api/info` answers `{item_id, products}`. After a restart the newest stored item is the
  demo's current one.
- Item endpoints, for this computer only (loopback check): `GET /api/items` (ids and bank
  names), `GET /api/items/:id/accounts`, `POST /api/items/:id/transactions/sync {cursor}` (all
  pages of Plaid `/transactions/sync`, count 500; waits 2 s up to 15 times for a new item's
  history; starts over on TRANSACTIONS_SYNC_MUTATION_DURING_PAGINATION up to 3 times; answers
  added/modified/removed/next_cursor/accounts), `DELETE /api/items/:id` (Plaid `/item/remove`,
  token forgotten) and `POST /api/items/import {access_token}` (one-time hand-over).
- Listens on 127.0.0.1 only (APP_HOST to change); the demo reaches it through its proxy.

Checken:
- Knows each connection by item id only; never holds the Plaid keys or an access token.
  Keeps each connection's sync cursor, so an update fetches only what changed.
- Connecting: create_link_token → Plaid Link in the browser from a page Checken serves on
  127.0.0.1 → public token to `set_access_token` → item id → accounts. "Add a bank already
  connected on the Plaid server" offers an item the server holds that Checken doesn't use yet.
- Tokens an older Checken kept in `plaid-tokens.json` are handed to the server on the first
  update, after which Checken's copy is cleared away.
- Each bank account goes to a new Checken account (kind from Plaid type/subtype: credit →
  Credit Card, loan → Loan, investment → Brokerage, depository savings/money market/cd/hsa →
  Savings, other depository → Checking), an existing account (default: same name, same last
  four digits, or the account "Connect to Bank" was started from), or nowhere.
- Amounts: Plaid's positive = money out, so Checken amount = −Plaid amount.
- Pending transactions wait until they post. Posted ones are added as Cleared.
- Already there (same account, date and amount, not from Plaid) → matched, not added twice.
- `modified` updates date and amount (the person's category stays); `removed` deletes only
  transactions Plaid created; matched ones of the person's own are just unlinked.
- Category: the person's own most recent category for that payee when the money goes the same
  way; else Plaid's personal_finance_category as "Primary:Detailed" in title case
  ("FOOD_AND_DRINK_GROCERIES" → "Food & Drink:Groceries"); TRANSFER_IN/TRANSFER_OUT/
  LOAN_PAYMENTS → "Transfer". New categories under "Income" are income, the rest expense.
- New accounts start at the bank's current balance: opening = bank balance − downloaded total
  (cards and loans: balance owed is negative).
- Stop Updating from Bank keeps the account and its transactions; when none of a connection's
  accounts are linked any more, Checken asks the server to disconnect it at Plaid. Deleting an
  account does the same.

## QFX / OFX import (File > Import > Bank or Brokerage File (QFX, OFX)…)

Quicken lists "Bank or Brokerage File (OFX, QFX)…" first under File > Import (the person's
picture of Quicken's menu). Checken reads the OFX standard (1.x SGML and 2.x XML; QFX is OFX
with Quicken's Web Connect header):
- Account per statement: BANKACCTFROM (ACCTID, ACCTTYPE: SAVINGS/MONEYMRKT/CD → Savings,
  CREDITLINE → Credit Card, else Checking) or CCACCTFROM (Credit Card). Named
  "<FI ORG> <Checking|Savings|Card> ••<last 4 of ACCTID>".
- STMTTRN: DTPOSTED (first 8 digits, YYYYMMDD), TRNAMT (signed; money out negative, the same
  as Checken), NAME as payee (MEMO if no NAME), MEMO kept when different, CHECKNUM.
  Entities &amp; &lt; &gt; &apos; &quot; are decoded. Added as Cleared.
- LEDGERBAL/BALAMT: a new account's starting balance = bank balance − the file's total, dated
  at its first transaction; ignored for accounts already in Checken.
- Matching an existing account: same name, else the same last four digits in its name or on
  its Plaid connection.
- No categories in the file: a payee the person categorised before gets that category when
  the money goes the same way; otherwise Uncategorized.
- Then the same preview, duplicate check (account, date, amount) and safety copy as CSV import.

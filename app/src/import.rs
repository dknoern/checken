//! Reading transactions from a CSV file exported by Quicken (or a Mint.com
//! style CSV) and adding them to the ledger. See MECHANICS.md, "Import".
//!
//! The work is split in three: `parse` turns the file's text into rows,
//! `Plan::build` works out what an import would do (which accounts to make,
//! which rows are already there, which transfers pair up), and `apply` does it.

use crate::model::*;
use chrono::NaiveDate;
use std::collections::{BTreeMap, HashMap};

/// One transaction line read from the file.
#[derive(Clone, Debug)]
pub struct CsvRow {
    pub date: NaiveDate,
    pub payee: String,
    pub category: String,
    pub amount: f64,
    /// The account named on the line (empty when the file has no account column).
    pub account: String,
    pub memo: String,
    pub number: String,
    pub cleared: Cleared,
}

/// What was read from a file, before anything is added.
#[derive(Clone, Debug)]
pub struct Parsed {
    pub rows: Vec<CsvRow>,
    /// Lines that weren't transactions (titles, filters, totals) or couldn't be read.
    #[cfg_attr(not(test), allow(dead_code))]
    pub skipped: usize,
    /// Scheduled (not yet happened) transactions, left out.
    pub scheduled: usize,
    /// The kind of account the export says it came from ("Checking Report...").
    pub kind_hint: Option<AccountKind>,
    /// The account names in the file, in order of first appearance.
    pub accounts: Vec<String>,
    /// The date format the first data row was read with.
    #[cfg_attr(not(test), allow(dead_code))]
    pub date_format: DateFormat,
    /// What kind each account is, when the file says (bank files do).
    pub account_kinds: BTreeMap<String, AccountKind>,
    /// Each account's last four digits, when the file gives its number.
    pub masks: BTreeMap<String, String>,
    /// Rows Checken made up from a bank file's balance, to start a new account
    /// at the bank's balance; ignored for accounts already in Checken.
    pub synthetic_openings: std::collections::BTreeSet<usize>,
}

/// Splits CSV text into records: commas between fields, double quotes around
/// fields that hold commas, quotes ("") or line breaks. A byte-order mark is skipped.
pub fn csv_records(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Vec::new();
    let mut rec: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            ',' => rec.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                rec.push(std::mem::take(&mut field));
                out.push(std::mem::take(&mut rec));
                any = false;
            }
            _ => field.push(c),
        }
    }
    if any {
        rec.push(field);
        out.push(rec);
    }
    out
}

/// Which column holds what, found from the header line's names.
#[derive(Default, Debug)]
struct Columns {
    date: Option<usize>,
    payee: Option<usize>,
    category: Option<usize>,
    amount: Option<usize>,
    account: Option<usize>,
    memo: Option<usize>,
    number: Option<usize>,
    status: Option<usize>,
    kind: Option<usize>,
    tags: Option<usize>,
    scheduled: Option<usize>,
}

impl Columns {
    fn from_header(rec: &[String]) -> Columns {
        let mut c = Columns::default();
        for (i, name) in rec.iter().enumerate() {
            let n = name.trim().to_lowercase();
            let slot = match n.as_str() {
                "date" => &mut c.date,
                "payee" | "payee/security" | "payee / security" | "description" | "name" => &mut c.payee,
                "category" => &mut c.category,
                "amount" => &mut c.amount,
                "account" | "account name" => &mut c.account,
                "memo" | "notes" | "memo/notes" => &mut c.memo,
                "check #" | "check number" | "num" | "number" | "check" | "chk #" => &mut c.number,
                "status" | "clr" | "cleared" => &mut c.status,
                "transaction type" | "type" => &mut c.kind,
                "tags" | "labels" | "tag" => &mut c.tags,
                "scheduled" => &mut c.scheduled,
                _ => continue,
            };
            if slot.is_none() {
                *slot = Some(i);
            }
        }
        c
    }
}

/// "Checking Report Created: ..." → Checking, and so on.
fn kind_from_title(line: &str) -> Option<AccountKind> {
    let l = line.to_lowercase();
    let head = l.split(" report").next()?;
    kind_from_words(head)
}

fn kind_from_words(s: &str) -> Option<AccountKind> {
    let s = s.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| s.contains(x));
    // Quicken lists lines of credit with the credit cards.
    if has(&["credit card", "visa", "mastercard", "amex", "american express", "discover", "card", "line of credit", " loc ", "heloc"]) {
        Some(AccountKind::CreditCard)
    } else if has(&["saving", "money market"]) {
        Some(AccountKind::Savings)
    } else if has(&["checking"]) {
        Some(AccountKind::Checking)
    } else if has(&["brokerage", "invest", "401k", "401(k)", "ira", "retirement"]) {
        Some(AccountKind::Investment)
    } else if has(&["loan", "mortgage"]) {
        Some(AccountKind::Loan)
    } else if has(&["cash", "wallet"]) {
        Some(AccountKind::Cash)
    } else if has(&["property", "asset", "house", "vehicle"]) {
        Some(AccountKind::Asset)
    } else {
        None
    }
}

/// A sensible kind for an account the import will create.
pub fn guess_kind(name: &str, hint: Option<AccountKind>) -> AccountKind {
    kind_from_words(name).or(hint).unwrap_or(AccountKind::Checking)
}

/// Quicken reads an amount by removing the grouping separator ("," here) and
/// reading what's left as a decimal number (-[SecurityPriceHistoryCSVImportWindowController
/// parseFileURL:withError:], 0x4f3fa0: NSLocaleGroupingSeparator, then
/// safeDecimalNumberWithString:). Checken also lets a leading "$" and
/// accounting-style "(45.00)" through.
fn read_amount(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (neg, inner) = match s.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        Some(x) => (true, x),
        None => (false, s),
    };
    let clean: String = inner.chars().filter(|c| !matches!(c, ',' | '$' | ' ')).collect();
    if clean.is_empty() || !clean.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+')) {
        return None;
    }
    let v = (clean.parse::<f64>().ok()? * 100.0).round() / 100.0;
    Some(if neg { -v.abs() } else { v })
}

/// Quicken's CSV date formats, tried in this order on the first data row; the
/// first that reads it is used for every row (0x4f3fa0: setDateFormat: "M/d/yy",
/// "M/d/yyyy", "d-MMM-yy", "yyyy-MM-dd", "yyyy/MM/dd", time zone UTC).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DateFormat {
    MonthDayYear2,
    MonthDayYear4,
    DayMonYear,
    IsoDash,
    IsoSlash,
}

pub const DATE_FORMATS: [DateFormat; 5] =
    [DateFormat::MonthDayYear2, DateFormat::MonthDayYear4, DateFormat::DayMonYear, DateFormat::IsoDash, DateFormat::IsoSlash];

/// A "yy" year as a date formatter reads it: two digits land within the 80 years
/// before and 20 after today (the formatter's default; Quicken sets no other);
/// more digits are the year itself.
fn year_yy(s: &str) -> Option<i32> {
    let y: i32 = s.parse().ok()?;
    if s.len() != 2 {
        return Some(y);
    }
    let now = chrono::Datelike::year(&today());
    let mut full = (now / 100) * 100 + y;
    if full >= now + 20 {
        full -= 100;
    } else if full < now - 80 {
        full += 100;
    }
    Some(full)
}

fn num(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 4 || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

impl DateFormat {
    pub fn read(self, s: &str) -> Option<NaiveDate> {
        let s = s.trim();
        match self {
            DateFormat::MonthDayYear2 | DateFormat::MonthDayYear4 => {
                let p: Vec<&str> = s.split('/').collect();
                let [m, d, y] = p.as_slice() else { return None };
                let year = if self == DateFormat::MonthDayYear2 { year_yy(y)? } else { num(y)? as i32 };
                NaiveDate::from_ymd_opt(year, num(m)?, num(d)?)
            }
            DateFormat::DayMonYear => {
                let p: Vec<&str> = s.split('-').collect();
                let [d, mon, y] = p.as_slice() else { return None };
                const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
                let m = MONTHS.iter().position(|x| x.eq_ignore_ascii_case(mon))? as u32 + 1;
                NaiveDate::from_ymd_opt(year_yy(y)?, m, num(d)?)
            }
            DateFormat::IsoDash | DateFormat::IsoSlash => {
                let sep = if self == DateFormat::IsoDash { '-' } else { '/' };
                let p: Vec<&str> = s.split(sep).collect();
                let [y, m, d] = p.as_slice() else { return None };
                if y.len() != 4 {
                    return None;
                }
                NaiveDate::from_ymd_opt(num(y)? as i32, num(m)?, num(d)?)
            }
        }
    }
}

/// Text as Quicken keeps it: trimmed, with runs of spaces, tabs and line breaks
/// made one space (CSVRowProcessor.Result.resultTrimmedAndCoalescedWhitespace,
/// q_stringByTrimmingAndCoalescingWhitespace).
fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Reads the whole file. `fallback_account` names the account when the file
/// has no account column.
pub fn parse(text: &str, fallback_account: &str) -> Result<Parsed, String> {
    let recs = csv_records(text);
    let kind_hint = recs.first().and_then(|r| r.first()).and_then(|t| kind_from_title(t));
    // Quicken's importer wants the header on the first line; its own register
    // exports put a title and filter lines first, so the header is searched for.
    let header = recs.iter().position(|r| {
        let names: Vec<String> = r.iter().map(|x| x.trim().to_lowercase()).collect();
        names.iter().any(|n| n == "date") && names.iter().any(|n| n == "amount")
    });
    let Some(h) = header else {
        return Err("The provided CSV header was invalid: it needs Date and Amount columns. Edit the header strings in the CSV file.".into());
    };
    let cols = Columns::from_header(&recs[h]);
    let (Some(dc), Some(ac)) = (cols.date, cols.amount) else { unreachable!() };
    let get = |r: &Vec<String>, c: Option<usize>| c.and_then(|i| r.get(i)).map(|s| tidy(s)).unwrap_or_default();

    let mut rows = Vec::new();
    let mut skipped = h; // the title and filter lines above the header
    let mut scheduled = 0;
    let mut accounts: Vec<String> = Vec::new();
    let mut format: Option<DateFormat> = None;
    for (i, r) in recs.iter().enumerate().skip(h + 1) {
        let line = i + 1;
        let date_text = r.get(dc).map(|d| d.trim()).unwrap_or("");
        // Lines without a date are the export's totals ("Total Inflows:" ...), not transactions.
        if date_text.is_empty() {
            skipped += 1;
            continue;
        }
        let fmt = match format {
            Some(f) => f,
            None => {
                let Some(f) = DATE_FORMATS.iter().copied().find(|f| f.read(date_text).is_some()) else {
                    return Err(format!(
                        "Unable to interpret the Date format used: \"{date_text}\" (line {line}). Edit the CSV file to use a format like 12/31/2021."
                    ));
                };
                format = Some(f);
                f
            }
        };
        let Some(date) = fmt.read(date_text) else {
            return Err(format!("Unable to interpret the Date format used: \"{date_text}\" (line {line}). Edit the CSV file to use a format like 12/31/2021."));
        };
        let amount_text = r.get(ac).map(|a| a.trim()).unwrap_or("");
        let Some(mut amount) = read_amount(amount_text) else {
            return Err(format!("Unable to interpret the Amount \"{amount_text}\" (line {line}). Edit the CSV file to use a format like 123.45."));
        };
        if !get(r, cols.scheduled).is_empty() {
            scheduled += 1;
            continue;
        }
        // Quicken: "debit" will negate the amount and "credit" will leave it;
        // blank counts as "credit".
        if get(r, cols.kind).eq_ignore_ascii_case("debit") {
            amount = -amount;
        }
        let mut account = get(r, cols.account);
        if account.is_empty() {
            account = fallback_account.to_string();
        }
        if !accounts.contains(&account) {
            accounts.push(account.clone());
        }
        let mut memo = get(r, cols.memo);
        let tags = get(r, cols.tags);
        if !tags.is_empty() {
            if !memo.is_empty() {
                memo.push_str("  ");
            }
            memo.push_str(&format!("Tags: {tags}"));
        }
        let status = get(r, cols.status).to_lowercase();
        let cleared = if cols.status.is_none() {
            // An export of past transactions: they've already happened at the bank.
            Cleared::Cleared
        } else if status.starts_with('r') {
            Cleared::Reconciled
        } else if status.starts_with('c') || status == "*" || status == "x" {
            Cleared::Cleared
        } else {
            Cleared::No
        };
        let mut category = get(r, cols.category);
        if category.eq_ignore_ascii_case("uncategorized") {
            category.clear();
        }
        rows.push(CsvRow { date, payee: get(r, cols.payee), category, amount, account, memo, number: get(r, cols.number), cleared });
    }
    if rows.is_empty() {
        return Err("No data rows were found in the provided file.".into());
    }
    Ok(Parsed {
        rows,
        skipped,
        scheduled,
        kind_hint,
        accounts,
        date_format: format.unwrap_or(DateFormat::MonthDayYear2),
        account_kinds: BTreeMap::new(),
        masks: BTreeMap::new(),
        synthetic_openings: Default::default(),
    })
}

/// Where one account in the file goes.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Make a new account of this kind.
    New(AccountKind),
    /// Add to an account already in the ledger.
    Existing(u32),
}

fn is_opening_balance(r: &CsvRow) -> bool {
    r.payee.to_lowercase().contains("opening balance")
        && (r.category.is_empty() || r.category.eq_ignore_ascii_case("adjustment") || r.category.to_lowercase().contains("opening"))
}

/// What an import would do.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    /// Rows to add as transactions (indexes into `Parsed::rows`), with the
    /// account each lands in and, for a matched transfer, the other account.
    pub adds: Vec<(usize, AcctRef, Option<AcctRef>)>,
    /// Opening balances for new accounts: file account name → (amount, date).
    pub openings: BTreeMap<String, (f64, NaiveDate)>,
    /// Lines already in the ledger (imported before), left out.
    pub duplicates: usize,
    pub transfers_matched: usize,
    pub new_categories: Vec<(String, CatKind)>,
    pub first: Option<NaiveDate>,
    pub last: Option<NaiveDate>,
    /// Rows per file account (all of them, before duplicates are taken out).
    pub per_account: BTreeMap<String, usize>,
    /// Rows per file account that are already in the ledger.
    pub dup_per_account: BTreeMap<String, usize>,
}

/// An account in the plan: one in the ledger, or one the import will make
/// (named by its name in the file).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AcctRef {
    Ledger(u32),
    New(String),
}

fn cents(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

impl Plan {
    pub fn build(ledger: &Ledger, parsed: &Parsed, targets: &BTreeMap<String, Target>) -> Plan {
        let mut plan = Plan::default();
        let acct_of = |name: &str| match targets.get(name) {
            Some(Target::Existing(id)) => AcctRef::Ledger(*id),
            _ => AcctRef::New(name.to_string()),
        };

        // What's already in the ledger, to leave out lines imported before:
        // the same account, date and amount. Payees aren't compared, because
        // another export of the same register can name or fill them differently
        // (one names the column "Payee/Security"). Each transaction counts once
        // in its account and once in a transfer's other side, and each one only
        // stands in for one line, so real repeats inside a file still import.
        let mut have: HashMap<(u32, NaiveDate, i64), usize> = HashMap::new();
        for t in &ledger.transactions {
            *have.entry((t.account, t.date, cents(t.amount))).or_default() += 1;
            if let Some(x) = t.transfer {
                *have.entry((x, t.date, cents(-t.amount))).or_default() += 1;
            }
        }

        let mut keep: Vec<usize> = Vec::new();
        for (i, r) in parsed.rows.iter().enumerate() {
            if !parsed.synthetic_openings.contains(&i) {
                *plan.per_account.entry(r.account.clone()).or_default() += 1;
            }
            let acct = acct_of(&r.account);
            // A starting balance made up from a bank file only starts a new account.
            if parsed.synthetic_openings.contains(&i) && matches!(acct, AcctRef::Ledger(_)) {
                continue;
            }
            if let AcctRef::Ledger(id) = acct {
                // An opening-balance line is already there when the account starts with that amount.
                let opening_like = is_opening_balance(r) || r.category.eq_ignore_ascii_case("adjustment");
                let seen = if opening_like && ledger.account(id).is_some_and(|a| cents(a.opening_balance) == cents(r.amount)) {
                    true
                } else {
                    match have.get_mut(&(id, r.date, cents(r.amount))).filter(|n| **n > 0) {
                        Some(n) => {
                            *n -= 1;
                            true
                        }
                        None => false,
                    }
                };
                if seen {
                    plan.duplicates += 1;
                    *plan.dup_per_account.entry(r.account.clone()).or_default() += 1;
                    continue;
                }
            }
            if is_opening_balance(r) && matches!(acct, AcctRef::New(_)) {
                let e = plan.openings.entry(r.account.clone()).or_insert((0.0, r.date));
                e.0 += r.amount;
                e.1 = e.1.min(r.date);
                continue;
            }
            keep.push(i);
        }

        // Pair transfers: an amount leaving one imported account and the same
        // amount arriving in another on the same day (then within 3 days).
        let rows = &parsed.rows;
        let mut partner: HashMap<usize, usize> = HashMap::new();
        let xfers: Vec<usize> = keep.iter().copied().filter(|&i| is_transfer_category(&rows[i].category)).collect();
        for window in [0i64, 3] {
            let mut by_amount: HashMap<i64, Vec<usize>> = HashMap::new();
            for &i in &xfers {
                if rows[i].amount > 0.0 && !partner.contains_key(&i) {
                    by_amount.entry(cents(rows[i].amount)).or_default().push(i);
                }
            }
            for &i in &xfers {
                let r = &rows[i];
                if r.amount >= 0.0 || partner.contains_key(&i) {
                    continue;
                }
                let Some(cands) = by_amount.get_mut(&cents(-r.amount)) else { continue };
                let found = cands.iter().position(|&j| {
                    let o = &rows[j];
                    acct_of(&o.account) != acct_of(&r.account) && (o.date - r.date).num_days().abs() <= window && !partner.contains_key(&j)
                });
                if let Some(p) = found {
                    let j = cands.remove(p);
                    partner.insert(i, j);
                    partner.insert(j, i);
                }
            }
        }
        // A bracketed category naming another account is a transfer to it.
        let account_named = |cat: &str| -> Option<AcctRef> {
            let inner = cat.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
            if let Some(a) = ledger.accounts.iter().find(|a| a.name.eq_ignore_ascii_case(inner)) {
                return Some(AcctRef::Ledger(a.id));
            }
            parsed.accounts.iter().find(|n| n.eq_ignore_ascii_case(inner)).map(|n| acct_of(n))
        };

        for &i in &keep {
            let r = &rows[i];
            let acct = acct_of(&r.account);
            let other = match partner.get(&i) {
                // The paired line is the same money: only the outgoing side is added.
                Some(_) if r.amount > 0.0 => continue,
                Some(&j) => {
                    plan.transfers_matched += 1;
                    Some(acct_of(&rows[j].account))
                }
                None => account_named(&r.category).filter(|o| *o != acct),
            };
            plan.first = Some(plan.first.map_or(r.date, |d| d.min(r.date)));
            plan.last = Some(plan.last.map_or(r.date, |d| d.max(r.date)));
            plan.adds.push((i, acct, other));
        }

        // Categories the ledger doesn't have yet. "Parent:Child" is a
        // subcategory (Quicken's own notation), and a subcategory has its top
        // category's type, so the type is decided once per top category: the
        // one already in the ledger, else income when the money in its whole
        // branch mostly came in.
        let mut new_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut top_net: BTreeMap<String, f64> = BTreeMap::new();
        for (i, _, other) in &plan.adds {
            let r = &rows[*i];
            if other.is_some() || r.category.is_empty() {
                continue;
            }
            *top_net.entry(cat_top(&r.category).to_string()).or_default() += r.amount;
            let mut name = Some(r.category.as_str());
            while let Some(n) = name {
                if ledger.category_kind(n).is_none() {
                    new_names.insert(n.to_string());
                }
                name = cat_parent(n);
            }
        }
        plan.new_categories = new_names
            .into_iter()
            .map(|name| {
                let top = cat_top(&name);
                let kind = ledger.category_kind(top).unwrap_or_else(|| {
                    let v = top_net.get(top).copied().unwrap_or(0.0);
                    let income = v > 0.0 && !is_transfer_category(top) || (v == 0.0 && top.to_lowercase().contains("income"));
                    if income { CatKind::Income } else { CatKind::Expense }
                });
                (name, kind)
            })
            .collect();
        plan
    }
}

/// What an import did.
#[derive(Debug, Default)]
pub struct Outcome {
    pub added: usize,
    /// Hidden (closed) accounts the import went into, shown again.
    pub reopened: usize,
    pub accounts_made: usize,
    pub categories_made: usize,
}

/// Adds the plan's accounts, categories and transactions to the ledger.
pub fn apply(ledger: &mut Ledger, parsed: &Parsed, targets: &BTreeMap<String, Target>, plan: &Plan) -> Outcome {
    let mut out = Outcome::default();
    let rows = &parsed.rows;
    // Importing into a hidden (closed) account shows it again.
    for name in &parsed.accounts {
        if let Some(Target::Existing(id)) = targets.get(name) {
            if let Some(a) = ledger.accounts.iter_mut().find(|a| a.id == *id && a.closed) {
                a.closed = false;
                out.reopened += 1;
            }
        }
    }
    // Make the new accounts, opened on their opening balance's date (or their first line's).
    let mut made: HashMap<String, u32> = HashMap::new();
    for name in &parsed.accounts {
        let Some(Target::New(kind)) = targets.get(name) else { continue };
        let first = plan.adds.iter().filter(|(i, _, _)| rows[*i].account == *name).map(|(i, _, _)| rows[*i].date).min();
        let (opening, open_date) = match plan.openings.get(name) {
            Some((v, d)) => (*v, first.map_or(*d, |f| f.min(*d))),
            None => (0.0, first.unwrap_or_else(today)),
        };
        if first.is_none() && !plan.openings.contains_key(name) {
            continue;
        }
        let id = ledger.next_account_id();
        let mut unique = name.clone();
        let mut n = 2;
        while ledger.accounts.iter().any(|a| a.name.eq_ignore_ascii_case(&unique)) {
            unique = format!("{name} {n}");
            n += 1;
        }
        ledger.accounts.push(Account { id, name: unique, kind: *kind, opening_balance: opening, opening_date: open_date, closed: false });
        made.insert(name.clone(), id);
        out.accounts_made += 1;
    }
    let resolve = |a: &AcctRef| -> Option<u32> {
        match a {
            AcctRef::Ledger(id) => Some(*id),
            AcctRef::New(n) => made.get(n).copied(),
        }
    };
    for (name, kind) in &plan.new_categories {
        if ledger.category_kind(name).is_none() {
            ledger.categories.push(Category { name: name.clone(), kind: *kind });
            out.categories_made += 1;
        }
    }
    for (i, acct, other) in &plan.adds {
        let r = &rows[*i];
        let Some(account) = resolve(acct) else { continue };
        let transfer = other.as_ref().and_then(resolve).filter(|&o| o != account);
        let id = ledger.new_id();
        ledger.transactions.push(Transaction {
            id,
            account,
            date: r.date,
            number: r.number.clone(),
            payee: r.payee.clone(),
            category: if transfer.is_some() { String::new() } else { r.category.clone() },
            memo: r.memo.clone(),
            amount: r.amount,
            cleared: r.cleared,
            transfer,
            splits: Vec::new(),
        });
        out.added += 1;
    }
    ledger.normalize_categories();
    out
}

/// Hidden (closed) accounts this import would go into, by their name in the file.
pub fn hidden_targets(ledger: &Ledger, parsed: &Parsed, targets: &BTreeMap<String, Target>) -> Vec<String> {
    parsed
        .accounts
        .iter()
        .filter(|n| matches!(targets.get(*n), Some(Target::Existing(id)) if ledger.account(*id).is_some_and(|a| a.closed)))
        .cloned()
        .collect()
}

/// Transactions an earlier import added twice: same account, day, amount and
/// category (or same transfer), where one copy has no payee and another has
/// one. That's what importing a second export whose payee column wasn't
/// recognised left behind. Returns the ids of the payee-less copies to remove
/// (at most as many as there are copies with a payee).
pub fn import_duplicates(ledger: &Ledger) -> Vec<u64> {
    let mut groups: HashMap<(u32, u32, NaiveDate, i64, String), (Vec<u64>, usize)> = HashMap::new();
    for t in &ledger.transactions {
        if !t.splits.is_empty() {
            continue;
        }
        let key = match t.transfer {
            // A transfer is the same money whichever side it was entered from.
            Some(x) if x < t.account => (x, t.account, t.date, cents(-t.amount), String::new()),
            Some(x) => (t.account, x, t.date, cents(t.amount), String::new()),
            None => (t.account, 0, t.date, cents(t.amount), t.category.to_lowercase()),
        };
        let g = groups.entry(key).or_default();
        if t.payee.trim().is_empty() {
            g.0.push(t.id);
        } else {
            g.1 += 1;
        }
    }
    let mut out = Vec::new();
    for (_, (mut blank, named)) in groups {
        blank.sort_unstable_by(|a, b| b.cmp(a)); // newest first
        out.extend(blank.into_iter().take(named));
    }
    out.sort_unstable();
    out
}

/// The default target for each account in the file: an account with the
/// same name if there is one, otherwise a new account of a guessed kind.
pub fn default_targets(ledger: &Ledger, parsed: &Parsed) -> BTreeMap<String, Target> {
    parsed
        .accounts
        .iter()
        .map(|name| {
            // Same name, or (for bank files) the same last four digits, either in
            // the account's name or on its bank connection.
            let mask = parsed.masks.get(name).filter(|m| !m.is_empty());
            let by_mask = |a: &&Account| {
                mask.is_some_and(|m| !a.closed && (a.name.contains(m.as_str()) || ledger.plaid.links.get(&a.id).is_some_and(|l| l.mask == *m)))
            };
            let found = ledger.accounts.iter().find(|a| a.name.eq_ignore_ascii_case(name)).or_else(|| ledger.accounts.iter().find(by_mask));
            let t = match found {
                Some(a) => Target::Existing(a.id),
                None => Target::New(parsed.account_kinds.get(name).copied().unwrap_or_else(|| guess_kind(name, parsed.kind_hint))),
            };
            (name.clone(), t)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_formats_follow_quicken() {
        let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd);
        assert_eq!(DateFormat::MonthDayYear2.read("11/19/2026"), d(2026, 11, 19));
        assert_eq!(DateFormat::MonthDayYear2.read("3/4/26"), d(2026, 3, 4));
        assert_eq!(DateFormat::MonthDayYear2.read("3/4/95"), d(1995, 3, 4));
        assert_eq!(DateFormat::DayMonYear.read("5-Jan-26"), d(2026, 1, 5));
        assert_eq!(DateFormat::IsoDash.read("2026-02-28"), d(2026, 2, 28));
        assert_eq!(DateFormat::IsoSlash.read("2026/02/28"), d(2026, 2, 28));
        assert_eq!(DateFormat::IsoDash.read("02-28-2026"), None);
        // The first data row picks the format; a later row in another format is refused.
        let bad = "Date,Payee,Amount\n1/2/2026,A,1.00\n2026-01-03,B,2.00\n";
        assert!(parse(bad, "X").unwrap_err().contains("line 3"));
        let bad_amount = "Date,Payee,Amount\n1/2/2026,A,abc\n";
        assert!(parse(bad_amount, "X").unwrap_err().contains("123.45"));
        assert!(parse("Payee,Amount\nA,1\n", "X").unwrap_err().contains("header"));
    }

    const SAMPLE: &str = "\u{feff}Checking Report Created: 2026-11-21 09:45:54 -0800\n,\nFilter Criteria:,All Dates\n,All Checking Accounts\n,\n\
,\"Scheduled\",\"Split\",\"Date\",\"Payee\",\"Category\",\"Amount\",\"Account\"\n\
,,,\"11/19/2026\",\"State Farm\",\"Auto & Transport:Auto Insurance\",\"-290.89\",\"Inbound\"\n\
,,,\"11/18/2026\",\"Venmo - Pat, Lee\",\"Business Expenses:Salary, Wages Paid\",\"-150.00\",\"Main Checking\"\n\
,,,\"11/16/2026\",\"External Deposit\",\"Business Income\",\"3,349.65\",\"Main Checking\"\n\
,,,\"11/15/2026\",\"Withdrawal - Transfer\",\"Transfer\",\"-98.94\",\"Main Checking\"\n\
,,,\"11/15/2026\",\"Deposit - Transfer\",\"Transfer\",\"98.94\",\"Inbound\"\n\
,,,\"11/14/2026\",\"Venmo\",\"Transfer\",\"-20.00\",\"Main Checking\"\n\
,,,\"11/19/2026\",,\"Uncategorized\",\"0.00\",\"Main Checking\"\n\
,,,\"10/16/2021\",\"Live Opening Balance\",\"Adjustment\",\"22,993.06\",\"Main Checking\"\n\
Total Inflows:,,,,,,2611958.04\nTotal Outflows:,,,,,,-2592549.25\nNet Total:,  19408.79\n";

    #[test]
    fn reads_a_quicken_mac_export() {
        let p = parse(SAMPLE, "Imported").unwrap();
        assert_eq!(p.rows.len(), 8);
        assert_eq!(p.kind_hint, Some(AccountKind::Checking));
        assert_eq!(p.accounts, vec!["Inbound".to_string(), "Main Checking".to_string()]);
        assert_eq!(p.rows[1].payee, "Venmo - Pat, Lee");
        assert_eq!(p.rows[1].category, "Business Expenses:Salary, Wages Paid");
        assert_eq!(p.rows[2].amount, 3349.65);
        assert_eq!(p.rows[6].category, "");
        assert_eq!(p.skipped, 5 + 3);
    }

    #[test]
    fn imports_pairs_transfers_and_skips_repeats() {
        let mut l = Ledger::default();
        let p = parse(SAMPLE, "Imported").unwrap();
        let targets = default_targets(&l, &p);
        assert_eq!(targets.get("Main Checking"), Some(&Target::New(AccountKind::Checking)));
        let plan = Plan::build(&l, &p, &targets);
        assert_eq!(plan.transfers_matched, 1);
        assert_eq!(plan.openings.get("Main Checking").map(|o| o.0), Some(22993.06));
        assert!(plan.new_categories.contains(&("Business Income".to_string(), CatKind::Income)));
        assert!(plan.new_categories.contains(&("Auto & Transport:Auto Insurance".to_string(), CatKind::Expense)));
        let out = apply(&mut l, &p, &targets, &plan);
        assert_eq!(out.accounts_made, 2);
        assert_eq!(out.added, 6);
        let main = l.accounts.iter().find(|a| a.name == "Main Checking").unwrap().id;
        let inbound = l.accounts.iter().find(|a| a.name == "Inbound").unwrap().id;
        assert!((l.balance(main) - (22993.06 - 150.0 + 3349.65 - 98.94 - 20.0)).abs() < 0.001);
        assert!((l.balance(inbound) - (-290.89 + 98.94)).abs() < 0.001);
        // Transfers stay out of spending; the unpaired one keeps its category.
        let d0 = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let a = l.actuals(d0, last_of_month(d0));
        assert!(!a.contains_key("Transfer"));
        assert_eq!(a.get("Business Income").copied(), Some(3349.65));

        // The same file again adds nothing.
        let targets = default_targets(&l, &p);
        let plan = Plan::build(&l, &p, &targets);
        assert_eq!(plan.adds.len(), 0, "{:?}", plan.adds);
        assert_eq!(plan.duplicates, 8);
    }

    #[test]
    fn a_second_export_with_payee_security_adds_nothing() {
        let mut l = Ledger::default();
        let p = parse(SAMPLE, "Imported").unwrap();
        let t = default_targets(&l, &p);
        let plan = Plan::build(&l, &p, &t);
        apply(&mut l, &p, &t, &plan);
        let n = l.transactions.len();
        // The same lines with a Check # column and "Payee/Security", payees blank.
        let b = SAMPLE
            .replace("\"Payee\",", "\"Check #\",\"Payee/Security\",")
            .replace(",,,\"11/", ",,,\"11/")
            .lines()
            .map(|line| if line.starts_with(",,,\"") { line.replacen("\",\"", "\",,\"", 1) } else { line.to_string() })
            .collect::<Vec<_>>()
            .join("\n");
        let p2 = parse(&b, "Imported").unwrap();
        assert_eq!(p2.rows[0].payee, "State Farm");
        let blanked = Parsed { rows: p2.rows.iter().cloned().map(|mut r| { r.payee.clear(); r }).collect(), ..p2.clone() };
        let t2 = default_targets(&l, &blanked);
        let plan2 = Plan::build(&l, &blanked, &t2);
        assert_eq!(plan2.adds.len(), 0);
        apply(&mut l, &blanked, &t2, &plan2);
        assert_eq!(l.transactions.len(), n);
        // Hidden accounts are offered and shown again by an import.
        let inbound = l.accounts.iter().find(|a| a.name == "Inbound").unwrap().id;
        l.accounts.iter_mut().find(|a| a.id == inbound).unwrap().closed = true;
        let t3 = default_targets(&l, &p);
        assert_eq!(hidden_targets(&l, &p, &t3), vec!["Inbound".to_string()]);
        let plan3 = Plan::build(&l, &p, &t3);
        let out = apply(&mut l, &p, &t3, &plan3);
        assert_eq!(out.reopened, 1);
        assert!(!l.account(inbound).unwrap().closed);
    }

    #[test]
    fn finds_payee_less_copies_from_an_earlier_import() {
        let mut l = Ledger::default();
        let d = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let mut add = |id: u64, payee: &str, cat: &str, amt: f64, xfer: Option<u32>| {
            l.transactions.push(Transaction {
                id, account: 1, date: d, number: String::new(), payee: payee.into(), category: cat.into(),
                memo: String::new(), amount: amt, cleared: Cleared::Cleared, transfer: xfer, splits: Vec::new(),
            });
        };
        add(1, "Zelle Storage", "Storage", -640.0, None);
        add(2, "", "Storage", -640.0, None);
        add(3, "Deposit", "", 400.0, Some(2));
        add(4, "", "", 400.0, Some(2));
        add(5, "", "Coffee", -5.0, None); // no named twin: kept
        assert_eq!(import_duplicates(&l), vec![2, 4]);
    }

    #[test]
    fn reads_mint_style_debit_and_credit() {
        let text = "Date,Description,Original Description,Amount,Transaction Type,Category,Account Name,Labels,Notes\n\
1/05/26,Coffee  Shop ,COFFEE 123,4.50,debit,Coffee Shops,Rewards Visa,,morning\n\
1/06/26,Employer,PAYROLL,\"2,000.00\",credit,Paycheck,Everyday Checking,work,\n";
        let p = parse(text, "Imported").unwrap();
        assert_eq!(p.rows[0].amount, -4.5);
        assert_eq!(p.rows[0].payee, "Coffee Shop");
        assert_eq!(p.rows[0].date, NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
        assert_eq!(p.date_format, DateFormat::MonthDayYear2);
        assert_eq!(p.rows[1].amount, 2000.0);
        assert_eq!(p.rows[1].memo, "Tags: work");
        assert_eq!(guess_kind("Rewards Visa", None), AccountKind::CreditCard);
    }
}

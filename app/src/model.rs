//! The ledger data: accounts, categories, transactions and monthly budgets,
//! plus the sums the screens and reports need.

use chrono::{Datelike, Local, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AccountKind {
    Checking,
    Savings,
    CreditCard,
    Cash,
    Investment,
    Asset,
    Loan,
}

impl AccountKind {
    pub const ALL: [AccountKind; 7] = [
        AccountKind::Checking,
        AccountKind::Savings,
        AccountKind::CreditCard,
        AccountKind::Cash,
        AccountKind::Investment,
        AccountKind::Asset,
        AccountKind::Loan,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AccountKind::Checking => "Checking",
            AccountKind::Savings => "Savings",
            AccountKind::CreditCard => "Credit Card",
            AccountKind::Cash => "Cash",
            AccountKind::Investment => "Brokerage",
            AccountKind::Asset => "Asset",
            AccountKind::Loan => "Loan",
        }
    }

    pub fn group(self) -> &'static str {
        match self {
            AccountKind::Checking | AccountKind::Savings | AccountKind::Cash => "Banking",
            AccountKind::CreditCard => "Credit Cards",
            AccountKind::Investment => "Investments",
            AccountKind::Asset | AccountKind::Loan => "Property & Debt",
        }
    }
}

/// The account list's top headings, as Quicken's sidebar has them: Banking
/// holds checking, savings, credit cards and cash; then Investing; then
/// Property & Debt.
pub const LIST_GROUPS: [&str; 3] = ["Banking", "Investing", "Property & Debt"];

impl AccountKind {
    /// The sidebar heading this kind of account sits under.
    pub fn list_group(self) -> &'static str {
        match self {
            AccountKind::Checking | AccountKind::Savings | AccountKind::CreditCard | AccountKind::Cash => "Banking",
            AccountKind::Investment => "Investing",
            AccountKind::Asset | AccountKind::Loan => "Property & Debt",
        }
    }

    /// The sub-heading kinds in list order: Checking, Savings, Credit Card, Cash, ...
    pub const LIST_ORDER: [AccountKind; 7] = [
        AccountKind::Checking,
        AccountKind::Savings,
        AccountKind::CreditCard,
        AccountKind::Cash,
        AccountKind::Investment,
        AccountKind::Asset,
        AccountKind::Loan,
    ];
}

/// A heading in the account list that can be selected to see all its accounts
/// together: a top group ("Banking") or a kind ("Savings").
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Scope {
    Group(&'static str),
    Kind(AccountKind),
}

impl Scope {
    pub fn includes(self, a: &Account) -> bool {
        !a.closed
            && match self {
                Scope::Group(g) => a.kind.list_group() == g,
                Scope::Kind(k) => a.kind == k,
            }
    }

    pub fn title(self) -> &'static str {
        match self {
            Scope::Group(g) => g,
            Scope::Kind(k) => k.label(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub id: u32,
    pub name: String,
    pub kind: AccountKind,
    pub opening_balance: f64,
    pub opening_date: NaiveDate,
    #[serde(default)]
    pub closed: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CatKind {
    Income,
    Expense,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Category {
    pub name: String,
    pub kind: CatKind,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cleared {
    No,
    Cleared,
    Reconciled,
}

impl Cleared {
    pub fn next(self) -> Cleared {
        match self {
            Cleared::No => Cleared::Cleared,
            Cleared::Cleared => Cleared::Reconciled,
            Cleared::Reconciled => Cleared::No,
        }
    }
}

/// One transaction. `amount` is signed from the point of view of `account`:
/// money out is negative, money in is positive. A transfer also shows in
/// the `transfer` account with the opposite sign.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Transaction {
    pub id: u64,
    pub account: u32,
    pub date: NaiveDate,
    #[serde(default)]
    pub number: String,
    pub payee: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub memo: String,
    pub amount: f64,
    pub cleared: Cleared,
    #[serde(default)]
    pub transfer: Option<u32>,
    /// When not empty, the transaction is split across these categories and
    /// `amount` is their total.
    #[serde(default)]
    pub splits: Vec<Split>,
}

/// One part of a split transaction: its own category, amount and note.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Split {
    #[serde(default)]
    pub category: String,
    pub amount: f64,
    #[serde(default)]
    pub memo: String,
}

impl Transaction {
    /// The (category, amount) parts of a transaction: its splits, or itself.
    pub fn parts(&self) -> Vec<(&str, f64)> {
        if self.splits.is_empty() {
            vec![(self.category.as_str(), self.amount)]
        } else {
            self.splits.iter().map(|s| (s.category.as_str(), s.amount)).collect()
        }
    }

    /// Whether any part of the transaction uses this category.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn uses_category(&self, name: &str) -> bool {
        self.category == name || self.splits.iter().any(|s| s.category == name)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReportKind {
    BudgetVsActual,
    Spending,
    IncomeExpense,
    NetWorth,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Period {
    ThisMonth,
    LastMonth,
    Last3,
    YearToDate,
    Last12,
}

/// A report the person saved under My Reports.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedReport {
    pub id: u64,
    pub name: String,
    pub kind: ReportKind,
    pub period: Period,
    #[serde(default)]
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Ledger {
    pub accounts: Vec<Account>,
    pub categories: Vec<Category>,
    pub transactions: Vec<Transaction>,
    /// "YYYY-MM" -> category -> planned amount (always positive).
    pub budgets: BTreeMap<String, BTreeMap<String, f64>>,
    pub next_id: u64,
    #[serde(default)]
    pub reports: Vec<SavedReport>,
    /// Bank connections through Plaid and what they've downloaded (no secrets).
    #[serde(default)]
    pub plaid: crate::plaid::PlaidData,
}

/// A row of an account register: the transaction, its signed amount in this
/// account, and the running balance after it.
pub struct RegisterRow {
    pub txn: usize,
    pub amount: f64,
    pub balance: f64,
}

pub fn month_key(d: NaiveDate) -> String {
    format!("{:04}-{:02}", d.year(), d.month())
}

pub fn first_of_month(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap()
}

pub fn last_of_month(d: NaiveDate) -> NaiveDate {
    first_of_month(d) + Months::new(1) - chrono::Days::new(1)
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_path() -> PathBuf {
    let home = home_dir();
    let dir = if cfg!(target_os = "macos") { home.join("Library/Application Support/Checken") } else { home.join(".checken") };
    dir.join("ledger.json")
}

impl Ledger {
    /// The person's data from data_path(). With no data file, Checken starts
    /// with sample accounts, transactions and budgets to explore (File > Purge
    /// All Data can start it empty instead). It no longer looks in the old
    /// "Pocket Ledger" folder: that data was carried over when the app was
    /// renamed.
    ///
    /// A data file that can't be read is set aside as "ledger (couldn't be
    /// read) <time>.json" before starting fresh, so a later save can never
    /// overwrite it. `may_write` is false for screenshot runs, which change
    /// nothing on disk.
    pub fn load_with(may_write: bool) -> Ledger {
        let path = data_path();
        let mut l = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Ledger>(&text) {
                Ok(l) => l,
                Err(_) => {
                    if may_write {
                        let stamp = Local::now().format("%Y-%m-%d %H%M%S");
                        let _ = std::fs::rename(&path, path.with_file_name(format!("ledger (couldn't be read) {stamp}.json")));
                    }
                    Ledger::sample()
                }
            },
            Err(_) => Ledger::sample(),
        };
        l.normalize_categories();
        l
    }

    pub fn load() -> Ledger {
        Ledger::load_with(true)
    }

    /// A new, empty ledger: categories and saved reports to start from, no
    /// accounts or transactions.
    pub fn starter() -> Ledger {
        let s = Ledger::sample();
        Ledger { categories: s.categories, reports: s.reports, next_id: s.next_id, ..Default::default() }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&data_path())
    }

    /// Writes the whole ledger to a file, via a temporary file so a failed
    /// write never leaves half a file behind.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn account(&self, id: u32) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn account_name(&self, id: u32) -> String {
        self.account(id).map(|a| a.name.clone()).unwrap_or_else(|| "(deleted)".into())
    }

    pub fn next_account_id(&self) -> u32 {
        self.accounts.iter().map(|a| a.id).max().unwrap_or(0) + 1
    }

    pub fn category_kind(&self, name: &str) -> Option<CatKind> {
        self.categories.iter().find(|c| c.name == name).map(|c| c.kind)
    }

    /// Signed effect of a transaction on an account (0 if unrelated).
    pub fn effect_on(t: &Transaction, account: u32) -> f64 {
        if t.account == account {
            t.amount
        } else if t.transfer == Some(account) {
            -t.amount
        } else {
            0.0
        }
    }

    pub fn balance_at(&self, account: u32, until: Option<NaiveDate>) -> f64 {
        let Some(a) = self.account(account) else { return 0.0 };
        let mut bal = if until.is_none_or(|d| a.opening_date <= d) { a.opening_balance } else { 0.0 };
        for t in &self.transactions {
            if until.is_none_or(|d| t.date <= d) {
                bal += Self::effect_on(t, account);
            }
        }
        bal
    }

    pub fn balance(&self, account: u32) -> f64 {
        self.balance_at(account, None)
    }

    pub fn cleared_balance(&self, account: u32) -> f64 {
        let open = self.account(account).map(|a| a.opening_balance).unwrap_or(0.0);
        open + self
            .transactions
            .iter()
            .filter(|t| t.cleared != Cleared::No)
            .map(|t| Self::effect_on(t, account))
            .sum::<f64>()
    }

    pub fn net_worth_at(&self, until: Option<NaiveDate>) -> f64 {
        self.accounts.iter().map(|a| self.balance_at(a.id, until)).sum()
    }

    /// Register rows for an account, oldest first, with running balances.
    pub fn register(&self, account: u32) -> Vec<RegisterRow> {
        let mut idx: Vec<usize> = (0..self.transactions.len())
            .filter(|&i| {
                let t = &self.transactions[i];
                t.account == account || t.transfer == Some(account)
            })
            .collect();
        idx.sort_by_key(|&i| (self.transactions[i].date, self.transactions[i].id));
        let mut bal = self.account(account).map(|a| a.opening_balance).unwrap_or(0.0);
        idx.into_iter()
            .map(|i| {
                let amount = Self::effect_on(&self.transactions[i], account);
                bal += amount;
                RegisterRow { txn: i, amount, balance: bal }
            })
            .collect()
    }

    /// Actual totals per category between two dates (inclusive). Expenses are
    /// returned as positive spending, income as positive earnings.
    pub fn actuals(&self, from: NaiveDate, to: NaiveDate) -> BTreeMap<String, f64> {
        let mut out = BTreeMap::new();
        let kinds: std::collections::HashMap<&str, CatKind> = self.categories.iter().map(|c| (c.name.as_str(), c.kind)).collect();
        for t in &self.transactions {
            if t.transfer.is_some() || t.date < from || t.date > to {
                continue;
            }
            // Each part of a split counts toward its own category.
            for (cat, amount) in t.parts() {
                // Money moving between the person's own accounts isn't spending or income.
                if cat.is_empty() || is_transfer_category(cat) {
                    continue;
                }
                let signed = match kinds.get(cat).copied() {
                    Some(CatKind::Income) => amount,
                    _ => -amount,
                };
                *out.entry(cat.to_string()).or_insert(0.0) += signed;
            }
        }
        out
    }

    /// Budgeted amount for a category summed over every month in the range.
    pub fn budgeted(&self, category: &str, from: NaiveDate, to: NaiveDate) -> f64 {
        let mut m = first_of_month(from);
        let mut total = 0.0;
        while m <= to {
            if let Some(b) = self.budgets.get(&month_key(m)) {
                total += b.get(category).copied().unwrap_or(0.0);
            }
            m = m + Months::new(1);
        }
        total
    }

    pub fn set_budget(&mut self, month: NaiveDate, category: &str, amount: f64) {
        let entry = self.budgets.entry(month_key(month)).or_default();
        if amount.abs() < 0.005 {
            entry.remove(category);
        } else {
            entry.insert(category.to_string(), amount);
        }
    }

    pub fn known_payees(&self) -> Vec<String> {
        let mut v: Vec<String> = self.transactions.iter().map(|t| t.payee.clone()).collect();
        v.sort_by_key(|p| p.to_lowercase());
        v.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        v
    }

    /// The most recent transaction with this payee, for filling in a new one.
    pub fn last_for_payee(&self, payee: &str) -> Option<&Transaction> {
        self.transactions
            .iter()
            .filter(|t| t.payee.eq_ignore_ascii_case(payee))
            .max_by_key(|t| (t.date, t.id))
    }

    /// Renames a category, and moves its subcategories along with it
    /// ("Auto" → "Car" also makes "Auto:Fuel" → "Car:Fuel").
    pub fn rename_category(&mut self, old: &str, new: &str) {
        let moved = |name: &str| -> Option<String> {
            if name == old {
                Some(new.to_string())
            } else {
                name.strip_prefix(old).and_then(|rest| rest.strip_prefix(':')).map(|rest| format!("{new}:{rest}"))
            }
        };
        for c in &mut self.categories {
            if let Some(n) = moved(&c.name) {
                c.name = n;
            }
        }
        for t in &mut self.transactions {
            if let Some(n) = moved(&t.category) {
                t.category = n;
            }
            for s in &mut t.splits {
                if let Some(n) = moved(&s.category) {
                    s.category = n;
                }
            }
        }
        for m in self.budgets.values_mut() {
            let keys: Vec<String> = m.keys().filter(|k| moved(k).is_some()).cloned().collect();
            for k in keys {
                if let (Some(v), Some(n)) = (m.remove(&k), moved(&k)) {
                    m.insert(n, v);
                }
            }
        }
        self.normalize_categories();
    }

    /// Keeps the category list a proper tree, as Quicken's is: every
    /// subcategory's parent exists, a subcategory has its top category's type
    /// (income or expense), and the list is in tree order (each category
    /// followed by its subcategories, by name).
    pub fn normalize_categories(&mut self) {
        let mut have: std::collections::HashSet<String> = self.categories.iter().map(|c| c.name.clone()).collect();
        let mut missing: Vec<Category> = Vec::new();
        for c in &self.categories {
            let mut p = cat_parent(&c.name);
            while let Some(name) = p {
                if have.insert(name.to_string()) {
                    missing.push(Category { name: name.to_string(), kind: c.kind });
                }
                p = cat_parent(name);
            }
        }
        self.categories.extend(missing);
        let top_kind: std::collections::HashMap<String, CatKind> =
            self.categories.iter().filter(|c| cat_parent(&c.name).is_none()).map(|c| (c.name.clone(), c.kind)).collect();
        for c in &mut self.categories {
            if let Some(k) = top_kind.get(cat_top(&c.name)) {
                c.kind = *k;
            }
        }
        self.categories.sort_by_cached_key(|c| cat_sort_key(&c.name));
        self.categories.dedup_by(|a, b| a.name == b.name);
    }

    pub fn has_subcategories(&self, name: &str) -> bool {
        self.categories.iter().any(|c| cat_parent(&c.name) == Some(name))
    }

    /// Budgeted amount for a category and all its subcategories.
    pub fn budgeted_rolled(&self, category: &str, from: NaiveDate, to: NaiveDate) -> f64 {
        self.categories
            .iter()
            .filter(|c| cat_within(&c.name, category))
            .map(|c| self.budgeted(&c.name, from, to))
            .sum()
    }
}

/// Category names are paths, as Quicken writes them: "Auto & Transport" is a
/// category and "Auto & Transport:Auto Insurance" one of its subcategories.
pub fn cat_parent(name: &str) -> Option<&str> {
    name.rsplit_once(':').map(|(p, _)| p)
}

/// The category's own name, without its parents ("Auto Insurance").
pub fn cat_leaf(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l.trim_start())
}

/// The top category a category sits under ("Auto & Transport").
pub fn cat_top(name: &str) -> &str {
    name.split(':').next().unwrap_or(name)
}

/// How deep a category sits: 0 for a top category, 1 for a subcategory...
pub fn cat_depth(name: &str) -> usize {
    name.matches(':').count()
}

/// Whether `name` is `ancestor` or one of its subcategories (at any depth).
pub fn cat_within(name: &str, ancestor: &str) -> bool {
    name == ancestor || name.strip_prefix(ancestor).is_some_and(|r| r.starts_with(':'))
}

fn cat_sort_key(name: &str) -> Vec<String> {
    name.split(':').map(|s| s.trim().to_lowercase()).collect()
}

/// Adds every category's amount into each of its parents as well, so a
/// parent's figure includes its subcategories (Quicken's roll-up).
pub fn roll_up(own: &BTreeMap<String, f64>) -> BTreeMap<String, f64> {
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for (name, v) in own {
        *out.entry(name.clone()).or_default() += v;
        let mut p = cat_parent(name);
        while let Some(parent) = p {
            *out.entry(parent.to_string()).or_default() += v;
            p = cat_parent(parent);
        }
    }
    out
}

impl Ledger {

    pub fn sample() -> Ledger {
        let mut l = Ledger::default();
        let today = today();
        let start = first_of_month(today) - Months::new(3);
        let acct = |id, name: &str, kind, opening| Account {
            id,
            name: name.into(),
            kind,
            opening_balance: opening,
            opening_date: start,
            closed: false,
        };
        l.accounts = vec![
            acct(1, "Everyday Checking", AccountKind::Checking, 2450.00),
            acct(2, "Rainy Day Savings", AccountKind::Savings, 8000.00),
            acct(3, "Rewards Card", AccountKind::CreditCard, -312.40),
            acct(4, "Wallet", AccountKind::Cash, 120.00),
            acct(5, "Retirement Fund", AccountKind::Investment, 41250.00),
            acct(6, "Car Loan", AccountKind::Loan, -9800.00),
        ];
        let cats: [(&str, CatKind); 17] = [
            ("Salary", CatKind::Income),
            ("Interest", CatKind::Income),
            ("Other Income", CatKind::Income),
            ("Rent", CatKind::Expense),
            ("Groceries", CatKind::Expense),
            ("Dining Out", CatKind::Expense),
            ("Utilities", CatKind::Expense),
            ("Phone & Internet", CatKind::Expense),
            ("Gas & Fuel", CatKind::Expense),
            ("Auto Loan", CatKind::Expense),
            ("Insurance", CatKind::Expense),
            ("Entertainment", CatKind::Expense),
            ("Shopping", CatKind::Expense),
            ("Health", CatKind::Expense),
            ("Gifts", CatKind::Expense),
            ("Travel", CatKind::Expense),
            ("Personal Care", CatKind::Expense),
        ];
        l.categories = cats.iter().map(|(n, k)| Category { name: n.to_string(), kind: *k }).collect();

        // A small repeatable pseudo-random sequence so amounts look natural.
        let mut seed: u32 = 12345;
        let mut rnd = move |lo: f64, hi: f64| {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let f = ((seed >> 8) % 10000) as f64 / 10000.0;
            ((lo + (hi - lo) * f) * 100.0).round() / 100.0
        };
        let add = |l: &mut Ledger, acc: u32, d: NaiveDate, payee: &str, cat: &str, amt: f64, xfer: Option<u32>| {
            if d > today {
                return;
            }
            let id = l.new_id();
            let cleared = if (today - d).num_days() > 6 { Cleared::Cleared } else { Cleared::No };
            l.transactions.push(Transaction {
                id,
                account: acc,
                date: d,
                number: String::new(),
                payee: payee.into(),
                category: if xfer.is_some() { String::new() } else { cat.into() },
                memo: String::new(),
                amount: amt,
                cleared,
                transfer: xfer,
                splits: Vec::new(),
            });
        };
        let day = |m: NaiveDate, d: u32| m.with_day(d.min(28)).unwrap();
        for k in 0..4 {
            let m = start + Months::new(k);
            add(&mut l, 1, day(m, 1), "Maple Street Apartments", "Rent", -1450.00, None);
            add(&mut l, 1, day(m, 1), "Acme Corp Payroll", "Salary", 2185.40, None);
            add(&mut l, 1, day(m, 15), "Acme Corp Payroll", "Salary", 2185.40, None);
            add(&mut l, 1, day(m, 4), "City Power & Light", "Utilities", -rnd(68.0, 112.0), None);
            add(&mut l, 1, day(m, 9), "Brightline Wireless", "Phone & Internet", -95.00, None);
            add(&mut l, 1, day(m, 12), "Lakeside Auto Finance", "", -325.00, Some(6));
            add(&mut l, 1, day(m, 16), "Transfer to savings", "", -400.00, Some(2));
            add(&mut l, 1, day(m, 20), "Rewards Card payment", "", -rnd(300.0, 400.0), Some(3));
            add(&mut l, 1, day(m, 22), "Harbor Insurance", "Insurance", -128.75, None);
            add(&mut l, 2, day(m, 28), "Interest earned", "Interest", rnd(6.0, 9.5), None);
            add(&mut l, 1, day(m, 6), "ATM withdrawal", "", -100.00, Some(4));
            for w in 0..4u32 {
                add(&mut l, 1, day(m, 3 + w * 7), "Green Basket Market", "Groceries", -rnd(62.0, 148.0), None);
                add(&mut l, 3, day(m, 5 + w * 7), "Corner Bistro", "Dining Out", -rnd(18.0, 64.0), None);
                add(&mut l, 4, day(m, 2 + w * 7), "Daily Grind Coffee", "Dining Out", -rnd(4.5, 9.0), None);
            }
            add(&mut l, 3, day(m, 7), "QuickFuel Station", "Gas & Fuel", -rnd(38.0, 55.0), None);
            add(&mut l, 3, day(m, 21), "QuickFuel Station", "Gas & Fuel", -rnd(38.0, 55.0), None);
            add(&mut l, 3, day(m, 11), "Starlight Cinema", "Entertainment", -rnd(24.0, 42.0), None);
            add(&mut l, 3, day(m, 18), "Online Marketplace", "Shopping", -rnd(25.0, 140.0), None);
            add(&mut l, 3, day(m, 25), "Streamly", "Entertainment", -15.99, None);
            add(&mut l, 1, day(m, 14), "Neighborhood Pharmacy", "Health", -rnd(12.0, 45.0), None);
        }
        let plan: [(&str, f64); 15] = [
            ("Salary", 4370.80),
            ("Interest", 8.00),
            ("Rent", 1450.00),
            ("Groceries", 450.00),
            ("Dining Out", 200.00),
            ("Utilities", 100.00),
            ("Phone & Internet", 95.00),
            ("Gas & Fuel", 100.00),
            ("Insurance", 130.00),
            ("Entertainment", 60.00),
            ("Shopping", 100.00),
            ("Health", 40.00),
            ("Gifts", 50.00),
            ("Travel", 150.00),
            ("Personal Care", 30.00),
        ];
        for k in 0..4 {
            let m = start + Months::new(k);
            for (c, v) in plan {
                l.set_budget(m, c, v);
            }
        }
        for (name, kind, period, description) in [
            ("Monthly Spending", ReportKind::Spending, Period::ThisMonth, "Where the money went this month, by category"),
            ("Budget Check-in", ReportKind::BudgetVsActual, Period::ThisMonth, "This month's spending against the budget"),
            ("Year at a Glance", ReportKind::IncomeExpense, Period::YearToDate, "Income and expenses for each month this year"),
        ] {
            let id = l.new_id();
            l.reports.push(SavedReport { id, name: name.into(), kind, period, description: description.into() });
        }
        l
    }
}

/// A transfer category (as Quicken and Mint exports name them): money moving
/// between the person's own accounts, which reports leave out of spending and income.
pub fn is_transfer_category(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    n == "transfer" || n.starts_with("transfer:") || n == "credit card payment" || (n.starts_with('[') && n.ends_with(']'))
}

pub fn money(v: f64) -> String {
    let neg = v < -0.004;
    let cents = (v.abs() * 100.0).round() as u64;
    let whole = (cents / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}${}.{:02}", if neg { "-" } else { "" }, grouped, cents % 100)
}

pub fn parse_money(s: &str) -> Option<f64> {
    let clean: String = s.chars().filter(|c| !matches!(c, '$' | ',' | ' ')).collect();
    if clean.is_empty() {
        return Some(0.0);
    }
    clean.parse::<f64>().ok().map(|v| (v * 100.0).round() / 100.0)
}

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%m/%d/%Y").to_string()
}

/// Accepts 3/14/2026, 3/14 (this year), 2026-03-14 or "today".
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("today") || s.eq_ignore_ascii_case("t") {
        return Some(today());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    let parts: Vec<&str> = s.split(['/', '-', '.']).collect();
    let num = |p: &str| p.trim().parse::<i32>().ok();
    match parts.as_slice() {
        [m, d] => NaiveDate::from_ymd_opt(today().year(), num(m)? as u32, num(d)? as u32),
        [m, d, y] => {
            let mut y = num(y)?;
            if y < 100 {
                y += 2000;
            }
            NaiveDate::from_ymd_opt(y, num(m)? as u32, num(d)? as u32)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_form_a_tree_like_quickens() {
        let mut l = Ledger::default();
        l.categories = vec![
            Category { name: "Auto & Transport:Auto Payment".into(), kind: CatKind::Expense },
            Category { name: "Auto & Transport:Auto Insurance".into(), kind: CatKind::Income },
            Category { name: "Salary".into(), kind: CatKind::Income },
        ];
        l.normalize_categories();
        let names: Vec<&str> = l.categories.iter().map(|c| c.name.as_str()).collect();
        // The missing parent is added, and the list is in tree order.
        assert_eq!(names, vec!["Auto & Transport", "Auto & Transport:Auto Insurance", "Auto & Transport:Auto Payment", "Salary"]);
        // Subcategories take their top category's type.
        assert!(l.categories[..3].iter().all(|c| c.kind == CatKind::Expense));
        assert!(l.has_subcategories("Auto & Transport"));
        assert_eq!(cat_leaf("Auto & Transport:Auto Insurance"), "Auto Insurance");
        assert_eq!(cat_top("Auto & Transport:Auto Insurance"), "Auto & Transport");
        // Roll-up: a parent's figure includes its subcategories.
        let own: BTreeMap<String, f64> =
            [("Auto & Transport".to_string(), 10.0), ("Auto & Transport:Auto Payment".to_string(), 300.0)].into_iter().collect();
        assert_eq!(roll_up(&own).get("Auto & Transport").copied(), Some(310.0));
        // Renaming a parent carries its subcategories along.
        let d = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        l.set_budget(d, "Auto & Transport:Auto Payment", 300.0);
        l.rename_category("Auto & Transport", "Car");
        assert!(l.category_kind("Car:Auto Payment").is_some());
        assert_eq!(l.budgeted_rolled("Car", d, last_of_month(d)), 300.0);
    }

    #[test]
    fn split_parts_count_toward_their_own_categories() {
        let mut l = Ledger::default();
        l.categories = vec![
            Category { name: "Groceries".into(), kind: CatKind::Expense },
            Category { name: "Household".into(), kind: CatKind::Expense },
        ];
        let d = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        l.transactions.push(Transaction {
            id: 1,
            account: 1,
            date: d,
            number: String::new(),
            payee: "Market".into(),
            category: String::new(),
            memo: String::new(),
            amount: -100.0,
            cleared: Cleared::No,
            transfer: None,
            splits: vec![
                Split { category: "Groceries".into(), amount: -70.0, memo: String::new() },
                Split { category: "Household".into(), amount: -30.0, memo: "soap".into() },
            ],
        });
        let a = l.actuals(first_of_month(d), last_of_month(d));
        assert_eq!(a.get("Groceries").copied(), Some(70.0));
        assert_eq!(a.get("Household").copied(), Some(30.0));
        l.rename_category("Household", "Home");
        assert_eq!(l.transactions[0].splits[1].category, "Home");
        assert!(l.transactions[0].uses_category("Home"));
    }
}

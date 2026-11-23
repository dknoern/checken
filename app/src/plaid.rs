//! Connecting bank accounts through Plaid and downloading their transactions.
//!
//! How it fits together (see MECHANICS.md, "Plaid"), following Plaid's
//! Quickstart advice that access tokens stay on the server:
//! - Checken talks only to the person's Plaid server (default
//!   http://localhost:8000). It never holds an access token or the Plaid keys;
//!   it knows each bank connection by its item id.
//! - Connecting: `POST /api/create_link_token` gives a link token, Plaid Link
//!   runs in the browser (a small page Checken serves on this computer only),
//!   and `POST /api/set_access_token` swaps Link's public token on the server,
//!   which keeps the access token and answers with the item id.
//! - Data: `GET /api/items`, `GET /api/items/{id}/accounts`,
//!   `POST /api/items/{id}/transactions/sync` (Checken keeps the cursor, so a
//!   refresh fetches only what changed) and `DELETE /api/items/{id}`.
//! - Checken versions before this kept access tokens in `plaid-tokens.json`;
//!   those are handed to the server once (`POST /api/items/import`) and the
//!   file is deleted.
//!
//! Requests go through the Mac's own `curl`.

use crate::model::*;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;

/// One bank connection (a Plaid "item").
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PlaidItem {
    pub item_id: String,
    pub institution: String,
    /// Where the last sync left off ("" = from the start).
    #[serde(default)]
    pub cursor: String,
    #[serde(default)]
    pub last_sync: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

/// A Checken account fed by one Plaid account.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PlaidLink {
    pub item_id: String,
    pub account_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mask: String,
}

/// Everything Plaid-related the ledger keeps (no secrets).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PlaidData {
    #[serde(default)]
    pub items: Vec<PlaidItem>,
    /// Checken account id → the Plaid account that feeds it.
    #[serde(default)]
    pub links: BTreeMap<u32, PlaidLink>,
    /// Plaid transaction id → the Checken transaction made from it (or matched to it).
    #[serde(default)]
    pub txns: BTreeMap<String, u64>,
    /// Checken transactions Plaid created (as opposed to ones it was matched to,
    /// which a removal at the bank only unlinks, never deletes).
    #[serde(default)]
    pub created: std::collections::BTreeSet<u64>,
    /// The Plaid server (blank = http://localhost:8000).
    #[serde(default)]
    pub server_url: String,
}

impl PlaidData {
    pub fn server(&self) -> String {
        let s = self.server_url.trim().trim_end_matches('/');
        if s.is_empty() { "http://localhost:8000".into() } else { s.to_string() }
    }

    pub fn item(&self, item_id: &str) -> Option<&PlaidItem> {
        self.items.iter().find(|i| i.item_id == item_id)
    }
}

// ------------------------------------------------- old keys hand-over ---

/// Where Checken versions before the server kept tokens stored them.
fn legacy_tokens_path() -> PathBuf {
    data_path().with_file_name("plaid-tokens.json")
}

/// Access tokens an older Checken kept (item id → token), if any.
pub fn legacy_tokens() -> BTreeMap<String, String> {
    std::fs::read_to_string(legacy_tokens_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Practice runs (screenshots) change nothing on disk or on the server's store.
static PRACTICE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn practice_run() {
    PRACTICE.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn practicing() -> bool {
    PRACTICE.load(std::sync::atomic::Ordering::SeqCst)
}

/// Hands each token an older Checken kept to the server, which stores it;
/// then deletes Checken's copy. Returns how many were handed over.
///
/// `in_use` are the connections Checken still uses. A key for any other
/// connection isn't needed and is dropped; so is a key Plaid no longer
/// accepts (the connection was removed or its access withdrawn). Only a key
/// the server couldn't take for another reason (say, it isn't running) is
/// kept to try again. A problem here never stops an update: it's reported,
/// and syncing carries on through the server.
pub fn hand_over_legacy_tokens(server: &str, in_use: &[String]) -> Result<usize, String> {
    let tokens = legacy_tokens();
    if tokens.is_empty() || practicing() {
        return Ok(0);
    }
    let mut kept: BTreeMap<String, String> = BTreeMap::new();
    let mut done = 0;
    let mut last_err = None;
    for (item, token) in tokens {
        if !in_use.contains(&item) {
            continue;
        }
        match request("POST", &format!("{server}/api/items/import"), Some(&json!({ "access_token": token }))) {
            Ok(_) => done += 1,
            Err(e) if gone(&e) => {}
            Err(e) => {
                last_err = Some(e);
                kept.insert(item, token);
            }
        }
    }
    let path = legacy_tokens_path();
    if kept.is_empty() {
        // Overwrite before removing, so the old token isn't left in the file's place.
        let _ = std::fs::write(&path, "{}");
        let _ = std::fs::remove_file(&path);
    } else if let Ok(text) = serde_json::to_string(&kept) {
        let _ = std::fs::write(&path, text);
    }
    match last_err {
        Some(e) => Err(e),
        None => Ok(done),
    }
}

/// Whether Plaid says a connection no longer exists or its key no longer works.
fn gone(err: &str) -> bool {
    ["ITEM_NOT_FOUND", "INVALID_ACCESS_TOKEN", "ACCESS_NOT_GRANTED", "USER_PERMISSION_REVOKED"].iter().any(|c| err.contains(c))
}

// ------------------------------------------------------------- talking ---

/// Sends a request to the Plaid server and returns its JSON answer, through curl.
fn request(method: &str, url: &str, body: Option<&Value>) -> Result<Value, String> {
    let mut args: Vec<String> = vec!["-sS".into(), "--max-time".into(), "120".into(), "-X".into(), method.into(), "-w".into(), "\n%{http_code}".into()];
    if body.is_some() {
        args.extend(["-H".into(), "Content-Type: application/json".into(), "--data-binary".into(), "@-".into()]);
    }
    args.push(url.into());
    // curl ships with macOS, Windows 10+ and most Linux systems.
    let curl = if cfg!(target_os = "macos") { "/usr/bin/curl" } else if cfg!(windows) { "curl.exe" } else { "curl" };
    let mut child = std::process::Command::new(curl)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start curl ({e})."))?;
    {
        let mut stdin = child.stdin.take().ok_or("curl has no input")?;
        if let Some(b) = body {
            stdin.write_all(b.to_string().as_bytes()).map_err(|e| e.to_string())?;
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("Couldn't reach the Plaid server ({err}). Is it running?"));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, code) = text.rsplit_once('\n').unwrap_or((&text, "0"));
    let code: u16 = code.trim().parse().unwrap_or(0);
    let v: Value = serde_json::from_str(body).unwrap_or(Value::String(body.chars().take(300).collect()));
    if !(200..300).contains(&code) {
        if code == 404 && v.is_string() {
            return Err("The Plaid server needs the update that keeps bank keys on the server; restart it after updating.".into());
        }
        // Plaid's errors carry a message meant for people (the server wraps them in "error").
        let e = v.get("error").filter(|e| e.is_object()).unwrap_or(&v);
        let msg = e
            .get("display_message")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| e.get("error_message").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| format!("The Plaid server answered {code}: {v}"));
        let code_name = e.get("error_code").and_then(Value::as_str).unwrap_or("");
        return Err(format!("{msg}{}", if code_name.is_empty() { String::new() } else { format!(" ({code_name})") }));
    }
    Ok(v)
}

/// A link token for Plaid Link, from the server.
pub fn create_link_token(server: &str) -> Result<String, String> {
    let v = request("POST", &format!("{server}/api/create_link_token"), Some(&json!({})))?;
    v.get("link_token").and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("The server didn't return a link token: {v}"))
}

/// Has the server swap Link's public token for an access token, which it
/// keeps; Checken gets the connection's item id.
pub fn exchange(server: &str, public_token: &str) -> Result<String, String> {
    let v = request("POST", &format!("{server}/api/set_access_token"), Some(&json!({ "public_token": public_token })))?;
    v.get("item_id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string).ok_or_else(|| "The server didn't return the connection's id.".into())
}

/// The bank connections the server holds: (item id, bank name).
pub fn server_items(server: &str) -> Result<Vec<(String, String)>, String> {
    let v = request("GET", &format!("{server}/api/items"), None)?;
    let list = v.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(list
        .iter()
        .filter_map(|i| {
            let id = i.get("item_id").and_then(Value::as_str)?.to_string();
            let name = i.get("institution_name").and_then(Value::as_str).unwrap_or_default().to_string();
            Some((id, name))
        })
        .collect())
}

/// Disconnects a bank at Plaid; the server forgets its key.
pub fn remove_item(server: &str, item_id: &str) -> Result<(), String> {
    if practicing() {
        return Ok(());
    }
    request("DELETE", &format!("{server}/api/items/{item_id}"), None).map(|_| ())
}

fn parse_accounts(v: &Value) -> Vec<BankAccount> {
    let list = v.get("accounts").and_then(Value::as_array).cloned().unwrap_or_default();
    let s = |a: &Value, k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    list.iter()
        .map(|a| BankAccount {
            account_id: s(a, "account_id"),
            name: s(a, "name"),
            official_name: s(a, "official_name"),
            mask: s(a, "mask"),
            kind: s(a, "type"),
            subtype: s(a, "subtype"),
            current: a.get("balances").and_then(|b| b.get("current")).and_then(Value::as_f64),
        })
        .collect()
}

/// One account at the bank, as Plaid describes it.
#[derive(Clone, Debug, Default)]
pub struct BankAccount {
    pub account_id: String,
    pub name: String,
    pub official_name: String,
    pub mask: String,
    pub kind: String,
    pub subtype: String,
    /// The bank's current balance (for cards and loans, what's owed).
    pub current: Option<f64>,
}

impl BankAccount {
    pub fn label(&self) -> String {
        if self.mask.is_empty() { self.name.clone() } else { format!("{} ••{}", self.name, self.mask) }
    }

    /// The Checken kind for this Plaid type and subtype.
    pub fn checken_kind(&self) -> AccountKind {
        match (self.kind.as_str(), self.subtype.as_str()) {
            ("credit", _) => AccountKind::CreditCard,
            ("loan", _) => AccountKind::Loan,
            ("investment", _) | ("brokerage", _) => AccountKind::Investment,
            ("depository", "savings" | "money market" | "cd" | "hsa") => AccountKind::Savings,
            ("depository", _) => AccountKind::Checking,
            _ => AccountKind::Checking,
        }
    }

    /// The balance as Checken counts it: money owed is negative.
    pub fn signed_balance(&self) -> Option<f64> {
        let c = self.current?;
        Some(if matches!(self.kind.as_str(), "credit" | "loan") { -c } else { c })
    }
}

/// The accounts of a connection and the bank's name, from the server.
pub fn accounts(server: &str, item_id: &str) -> Result<(Vec<BankAccount>, String), String> {
    let v = request("GET", &format!("{server}/api/items/{item_id}/accounts"), None)?;
    let name = v.get("institution_name").and_then(Value::as_str).unwrap_or_default().to_string();
    Ok((parse_accounts(&v), name))
}

/// One downloaded transaction, in Checken's terms.
#[derive(Clone, Debug, Default)]
pub struct BankTxn {
    pub id: String,
    pub account_id: String,
    pub date: Option<NaiveDate>,
    pub payee: String,
    /// Checken's sign: money in is positive.
    pub amount: f64,
    pub pending: bool,
    pub category: String,
    pub number: String,
}

/// What one `/transactions/sync` run brought.
#[derive(Clone, Debug, Default)]
pub struct SyncResult {
    pub added: Vec<BankTxn>,
    pub modified: Vec<BankTxn>,
    pub removed: Vec<String>,
    pub cursor: String,
    pub balances: Vec<BankAccount>,
}

fn title_case(s: &str) -> String {
    s.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let w = w.to_lowercase();
            if w == "and" {
                return "&".to_string();
            }
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Plaid's category as a Checken category: "FOOD_AND_DRINK" /
/// "FOOD_AND_DRINK_GROCERIES" → "Food & Drink:Groceries". Money moving between
/// the person's accounts becomes "Transfer" (left out of spending and income).
pub fn plaid_category(primary: &str, detailed: &str) -> String {
    if primary.is_empty() {
        return String::new();
    }
    if matches!(primary, "TRANSFER_IN" | "TRANSFER_OUT" | "LOAN_PAYMENTS") {
        return "Transfer".into();
    }
    let top = title_case(primary);
    let sub = detailed.strip_prefix(primary).map(|s| s.trim_start_matches('_')).unwrap_or(detailed);
    if sub.is_empty() || sub.eq_ignore_ascii_case("other") { top } else { format!("{top}:{}", title_case(sub)) }
}

fn parse_txn(t: &Value) -> BankTxn {
    let s = |k: &str| t.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let pfc = t.get("personal_finance_category");
    let primary = pfc.and_then(|p| p.get("primary")).and_then(Value::as_str).unwrap_or_default();
    let detailed = pfc.and_then(|p| p.get("detailed")).and_then(Value::as_str).unwrap_or_default();
    let payee = t.get("merchant_name").and_then(Value::as_str).filter(|m| !m.is_empty()).map(str::to_string).unwrap_or_else(|| s("name"));
    BankTxn {
        id: s("transaction_id"),
        account_id: s("account_id"),
        date: NaiveDate::parse_from_str(&s("date"), "%Y-%m-%d").ok(),
        payee,
        // Plaid: positive = money out of the account. Checken: money out is negative.
        amount: -(t.get("amount").and_then(Value::as_f64).unwrap_or(0.0) * 100.0).round() / 100.0,
        pending: t.get("pending").and_then(Value::as_bool).unwrap_or(false),
        category: plaid_category(primary, detailed),
        number: s("check_number"),
    }
}

/// Everything new for a connection since `cursor` (the server pages through
/// Plaid's `/transactions/sync` and waits for a new connection's history),
/// with current balances.
pub fn sync(server: &str, item_id: &str, cursor: &str) -> Result<SyncResult, String> {
    let v = request("POST", &format!("{server}/api/items/{item_id}/transactions/sync"), Some(&json!({ "cursor": cursor })))?;
    let list = |k: &str| v.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(SyncResult {
        added: list("added").iter().map(parse_txn).collect(),
        modified: list("modified").iter().map(parse_txn).collect(),
        removed: list("removed").iter().filter_map(|r| r.get("transaction_id").and_then(Value::as_str).map(str::to_string)).collect(),
        cursor: v.get("next_cursor").and_then(Value::as_str).unwrap_or(cursor).to_string(),
        balances: parse_accounts(&v),
    })
}

// ----------------------------------------------------------- applying ----

/// What a sync changed in the ledger.
#[derive(Debug, Default, Clone)]
pub struct Applied {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub matched: usize,
    pub pending_skipped: usize,
}

/// Puts a sync's results into the ledger: new posted transactions are added
/// to the linked accounts (unless the same one is already there, from an
/// import or typed in: same account, date and amount, then it's just matched),
/// changed ones are updated and removed ones deleted. Pending transactions
/// wait until they post. A payee seen before keeps its category, as Quicken
/// remembers payees; otherwise Plaid's category is used.
pub fn apply(ledger: &mut Ledger, item_id: &str, res: &SyncResult) -> Applied {
    let mut out = Applied::default();
    let account_for: BTreeMap<String, u32> =
        ledger.plaid.links.iter().filter(|(_, l)| l.item_id == item_id).map(|(a, l)| (l.account_id.clone(), *a)).collect();

    // Removed: delete what came from them.
    for rid in &res.removed {
        if let Some(tid) = ledger.plaid.txns.remove(rid) {
            if ledger.plaid.created.remove(&tid) {
                let before = ledger.transactions.len();
                ledger.transactions.retain(|t| t.id != tid);
                out.removed += before - ledger.transactions.len();
            }
        }
    }
    // Modified: update date, payee and amount (the category the person chose stays).
    for m in &res.modified {
        let Some(&tid) = ledger.plaid.txns.get(&m.id) else { continue };
        if let Some(t) = ledger.transactions.iter_mut().find(|t| t.id == tid) {
            if let Some(d) = m.date {
                t.date = d;
            }
            if t.splits.is_empty() {
                t.amount = m.amount;
            }
            out.updated += 1;
        }
    }
    // What's already in each linked account, by date and amount, to match rather than duplicate.
    let cents = |v: f64| (v * 100.0).round() as i64;
    let mut have: std::collections::HashMap<(u32, NaiveDate, i64), Vec<u64>> = std::collections::HashMap::new();
    let from_plaid: std::collections::HashSet<u64> = ledger.plaid.txns.values().copied().collect();
    for t in &ledger.transactions {
        if from_plaid.contains(&t.id) {
            continue;
        }
        have.entry((t.account, t.date, cents(t.amount))).or_default().push(t.id);
        if let Some(x) = t.transfer {
            have.entry((x, t.date, cents(-t.amount))).or_default().push(t.id);
        }
    }
    // Payees the person has categorised before (their own entries and imports,
    // not earlier downloads), newest first, with which way the money went.
    let mut remembered: std::collections::HashMap<String, (NaiveDate, u64, String, bool)> = std::collections::HashMap::new();
    for t in &ledger.transactions {
        if ledger.plaid.created.contains(&t.id) || t.transfer.is_some() || !t.splits.is_empty() || t.category.is_empty() || t.payee.is_empty() {
            continue;
        }
        let key = t.payee.to_lowercase();
        let newer = remembered.get(&key).is_none_or(|(d, id, _, _)| (t.date, t.id) > (*d, *id));
        if newer {
            remembered.insert(key, (t.date, t.id, t.category.clone(), t.amount >= 0.0));
        }
    }
    let mut new_cats: Vec<String> = Vec::new();
    for a in &res.added {
        if ledger.plaid.txns.contains_key(&a.id) {
            continue;
        }
        if a.pending {
            out.pending_skipped += 1;
            continue;
        }
        let (Some(&account), Some(date)) = (account_for.get(&a.account_id), a.date) else { continue };
        // Already there (imported from Quicken, or typed in)? Then just remember the match.
        if let Some(ids) = have.get_mut(&(account, date, cents(a.amount))) {
            if let Some(tid) = ids.pop() {
                ledger.plaid.txns.insert(a.id.clone(), tid);
                out.matched += 1;
                continue;
            }
        }
        // A remembered payee keeps its category when the money goes the same way
        // (a refund from a shop isn't spending); otherwise Plaid's category.
        let category = remembered
            .get(&a.payee.to_lowercase())
            .filter(|(_, _, _, inflow)| *inflow == (a.amount >= 0.0))
            .map(|(_, _, c, _)| c.clone())
            .unwrap_or_else(|| a.category.clone());
        if !category.is_empty() && ledger.category_kind(&category).is_none() && !new_cats.contains(&category) {
            new_cats.push(category.clone());
        }
        let id = ledger.new_id();
        ledger.transactions.push(Transaction {
            id,
            account,
            date,
            number: a.number.clone(),
            payee: a.payee.clone(),
            category,
            memo: String::new(),
            amount: a.amount,
            cleared: Cleared::Cleared,
            transfer: None,
            splits: Vec::new(),
        });
        ledger.plaid.txns.insert(a.id.clone(), id);
        ledger.plaid.created.insert(id);
        out.added += 1;
    }
    for name in new_cats {
        // Plaid's income categories start with "Income"; everything else is spending.
        let kind = if cat_top(&name).eq_ignore_ascii_case("income") { CatKind::Income } else { CatKind::Expense };
        ledger.categories.push(Category { name, kind });
    }
    ledger.normalize_categories();
    if let Some(item) = ledger.plaid.items.iter_mut().find(|i| i.item_id == item_id) {
        item.cursor = res.cursor.clone();
        item.last_sync = Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string());
        item.last_error = None;
    }
    out
}

// --------------------------------------------------------- Plaid Link ----

/// What the Link page reported back.
#[derive(Debug, Clone)]
pub enum LinkOutcome {
    Connected { public_token: String, institution: String },
    Cancelled(String),
}

/// Serves a one-page Plaid Link on this computer only (127.0.0.1), has
/// `open` show it (Checken's own Link window), and waits up to 20 minutes for
/// the person to finish, or until `cancel` is set (they closed the window).
pub fn run_link(link_token: &str, open: &dyn Fn(&str), cancel: &std::sync::atomic::AtomicBool) -> Result<LinkOutcome, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let url = format!("http://127.0.0.1:{port}/");
    open(&url);
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let page = LINK_PAGE.replace("__LINK_TOKEN__", &link_token.replace('"', ""));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20 * 60);
    loop {
        if std::time::Instant::now() > deadline {
            return Ok(LinkOutcome::Cancelled("The bank connection timed out.".into()));
        }
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(LinkOutcome::Cancelled("The bank connection window was closed.".into()));
        }
        let (mut stream, _) = match listener.accept() {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(150));
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
        let (method, path, body) = read_request(&mut stream);
        let reply = |stream: &mut std::net::TcpStream, ctype: &str, text: &str| {
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{text}", text.len());
        };
        match (method.as_str(), path.as_str()) {
            ("GET", "/") => reply(&mut stream, "text/html; charset=utf-8", &page),
            ("POST", "/done") => {
                reply(&mut stream, "application/json", "{}");
                let v: Value = serde_json::from_str(&body).unwrap_or_default();
                let public_token = v.get("public_token").and_then(Value::as_str).unwrap_or_default().to_string();
                let institution = v
                    .get("metadata")
                    .and_then(|m| m.get("institution"))
                    .and_then(|i| i.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if public_token.is_empty() {
                    return Ok(LinkOutcome::Cancelled("Plaid didn't hand back a connection.".into()));
                }
                return Ok(LinkOutcome::Connected { public_token, institution });
            }
            ("POST", "/exit") => {
                reply(&mut stream, "application/json", "{}");
                let v: Value = serde_json::from_str(&body).unwrap_or_default();
                let why = v
                    .get("err")
                    .and_then(|e| e.get("display_message").and_then(Value::as_str).or_else(|| e.get("error_message").and_then(Value::as_str)))
                    .unwrap_or("The bank connection was closed before it finished.")
                    .to_string();
                return Ok(LinkOutcome::Cancelled(why));
            }
            _ => {
                let _ = write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        }
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> (String, String, String) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut header_end = None;
    let mut content_len = 0usize;
    while let Ok(n) = stream.read(&mut chunk) {
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if header_end.is_none() {
            if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = Some(p + 4);
                let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                content_len = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
                    .unwrap_or(0);
            }
        }
        if let Some(h) = header_end {
            if buf.len() >= h + content_len {
                break;
            }
        }
        if buf.len() > 1_000_000 {
            break;
        }
    }
    let h = header_end.unwrap_or(buf.len());
    let head = String::from_utf8_lossy(&buf[..h]).to_string();
    let mut first = head.lines().next().unwrap_or_default().split_whitespace();
    let method = first.next().unwrap_or_default().to_string();
    let path = first.next().unwrap_or_default().to_string();
    let body = String::from_utf8_lossy(&buf[h.min(buf.len())..]).to_string();
    (method, path, body)
}

const LINK_PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Checken – Connect a bank</title>
<style>
body{font:16px -apple-system,BlinkMacSystemFont,sans-serif;color:#1d1d1f;background:#fff;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}
.card{background:#fff;border-radius:12px;padding:32px 40px;box-shadow:0 8px 28px rgba(0,0,0,.12);max-width:460px;text-align:center}
h1{font-size:22px;margin:0 0 10px} p{color:#707074;margin:8px 0} button{margin-top:14px;background:#2b62c4;color:#fff;border:0;border-radius:6px;padding:9px 18px;font-size:15px}
</style></head>
<body><div class="card"><h1>Connect a bank to Checken</h1><p id="msg">Opening Plaid…</p><button id="again" style="display:none">Open Plaid again</button></div>
<script src="https://cdn.plaid.com/link/v2/stable/link-initialize.js"></script>
<script>
const msg = document.getElementById('msg'), again = document.getElementById('again');
const done = (text) => { msg.textContent = text; again.style.display = 'none'; };
const handler = Plaid.create({
  token: "__LINK_TOKEN__",
  onSuccess: (public_token, metadata) => {
    fetch('/done', {method: 'POST', body: JSON.stringify({public_token, metadata})})
      .then(() => done('Connected. Reading your accounts…'))
      .catch(() => done('Connected, but Checken stopped listening. Try again from Checken.'));
  },
  onExit: (err, metadata) => {
    msg.textContent = err ? (err.display_message || err.error_message || 'Plaid closed.') : 'Plaid was closed.';
    again.style.display = 'inline-block';
    again.onclick = () => { again.style.display = 'none'; handler.open(); };
    const cancel = document.createElement('button');
    cancel.textContent = 'Cancel'; cancel.style.marginLeft = '10px'; cancel.style.background = '#999';
    cancel.onclick = () => fetch('/exit', {method: 'POST', body: JSON.stringify({err, metadata})}).then(() => done('Cancelled.'));
    if (!document.getElementById('cancelbtn')) { cancel.id = 'cancelbtn'; again.after(cancel); }
  },
});
handler.open();
</script></body></html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaid_categories_become_checken_subcategories() {
        assert_eq!(plaid_category("FOOD_AND_DRINK", "FOOD_AND_DRINK_GROCERIES"), "Food & Drink:Groceries");
        assert_eq!(plaid_category("TRANSFER_OUT", "TRANSFER_OUT_ACCOUNT_TRANSFER"), "Transfer");
        assert_eq!(plaid_category("INCOME", "INCOME_WAGES"), "Income:Wages");
        assert_eq!(plaid_category("", ""), "");
    }

    #[test]
    fn sync_adds_matches_updates_and_removes() {
        let mut l = Ledger::default();
        let d = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        l.accounts.push(Account { id: 1, name: "Main".into(), kind: AccountKind::Checking, opening_balance: 0.0, opening_date: d, closed: false });
        l.plaid.items.push(PlaidItem { item_id: "item".into(), institution: "Bank".into(), ..Default::default() });
        l.plaid.links.insert(1, PlaidLink { item_id: "item".into(), account_id: "acc".into(), ..Default::default() });
        // Already imported from Quicken: matched, not duplicated.
        l.transactions.push(Transaction {
            id: 50, account: 1, date: d, number: String::new(), payee: "Coffee".into(), category: "Dining".into(),
            memo: String::new(), amount: -4.5, cleared: Cleared::Cleared, transfer: None, splits: Vec::new(),
        });
        l.next_id = 50;
        let t = |id: &str, amt: f64, pending: bool| BankTxn {
            id: id.into(), account_id: "acc".into(), date: Some(d), payee: "Shop".into(), amount: amt, pending, category: "Shops:Groceries".into(), number: String::new(),
        };
        let res = SyncResult {
            added: vec![t("a1", -4.5, false), t("a2", -20.0, false), t("a3", -7.0, true)],
            cursor: "c1".into(),
            ..Default::default()
        };
        let out = apply(&mut l, "item", &res);
        assert_eq!((out.added, out.matched, out.pending_skipped), (1, 1, 1));
        assert_eq!(l.transactions.len(), 2);
        assert_eq!(l.plaid.item("item").unwrap().cursor, "c1");
        assert!(l.category_kind("Shops").is_some() && l.category_kind("Shops:Groceries").is_some());
        // A payee the person categorised before keeps that category; a payee
        // only seen in downloads (or money going the other way) gets Plaid's.
        l.transactions.push(Transaction {
            id: 60, account: 1, date: d, number: String::new(), payee: "Corner Shop".into(), category: "Groceries".into(),
            memo: String::new(), amount: -12.0, cleared: Cleared::Cleared, transfer: None, splits: Vec::new(),
        });
        l.categories.push(Category { name: "Groceries".into(), kind: CatKind::Expense });
        let mk = |id: &str, payee: &str, amt: f64, cat: &str| BankTxn {
            id: id.into(), account_id: "acc".into(), date: Some(d), payee: payee.into(), amount: amt, pending: false, category: cat.into(), number: String::new(),
        };
        let res_p = SyncResult {
            added: vec![mk("p1", "Corner Shop", -9.0, "Shops"), mk("p2", "Corner Shop", 5.0, "Income:Refunds"), mk("p3", "Store", 900.0, "Income:Wages"), mk("p4", "Store", -30.0, "Shops")],
            cursor: "c1b".into(),
            ..Default::default()
        };
        apply(&mut l, "item", &res_p);
        let cat_of = |l: &Ledger, amt: f64| l.transactions.iter().find(|t| t.amount == amt).map(|t| t.category.clone()).unwrap();
        assert_eq!(cat_of(&l, -9.0), "Groceries");
        assert_eq!(cat_of(&l, 5.0), "Income:Refunds");
        assert_eq!(cat_of(&l, -30.0), "Shops");
        // The same sync again adds nothing.
        let again = apply(&mut l, "item", &res);
        assert_eq!(again.added, 0);
        // Modified and removed.
        let res2 = SyncResult { modified: vec![t("a2", -25.0, false)], removed: vec!["a1".into()], cursor: "c2".into(), ..Default::default() };
        let out2 = apply(&mut l, "item", &res2);
        assert_eq!((out2.updated, out2.removed), (1, 0));
        assert!(l.transactions.iter().any(|t| t.amount == -25.0));
        assert!(l.transactions.iter().any(|t| t.id == 50), "a matched line of the person's own is only unlinked");
        assert!(!l.plaid.txns.contains_key("a1"));
        // A line Plaid created goes when the bank removes it.
        let out3 = apply(&mut l, "item", &SyncResult { removed: vec!["a2".into()], cursor: "c3".into(), ..Default::default() });
        assert_eq!(out3.removed, 1);
        assert!(!l.transactions.iter().any(|t| t.amount == -25.0));
    }
}

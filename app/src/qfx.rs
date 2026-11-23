//! Reading a bank download file: Quicken's QFX (Web Connect) or plain OFX,
//! the format Quicken lists first under File > Import ("Bank or Brokerage
//! File (OFX, QFX)"). Both the older SGML form (tags without closing tags
//! for single values) and the XML form are read.
//!
//! What's taken (see MECHANICS.md, "QFX / OFX import"):
//! - Each statement's account: BANKACCTFROM (ACCTID, ACCTTYPE) or
//!   CCACCTFROM (a credit card), named "<bank> <kind> ••<last 4>".
//! - Each STMTTRN: DTPOSTED (YYYYMMDD…), TRNAMT (already signed: money out is
//!   negative), NAME (or PAYEE/NAME) as the payee, MEMO, CHECKNUM.
//! - LEDGERBAL/BALAMT, the bank's balance: a new account starts so that it
//!   ends at that balance.
//! The result goes through the same preview, matching and duplicate checks
//! as a CSV import.

use crate::import::Parsed;
use crate::model::*;
use chrono::NaiveDate;
use std::collections::BTreeMap;

/// Whether some text looks like an OFX/QFX file.
pub fn looks_like_ofx(text: &str) -> bool {
    let head: String = text.chars().take(4000).collect::<String>().to_uppercase();
    head.contains("OFXHEADER") || head.contains("<OFX>")
}

#[derive(Default)]
struct Txn {
    date: Option<NaiveDate>,
    amount: Option<f64>,
    name: String,
    memo: String,
    number: String,
    kind: String,
}

#[derive(Default)]
struct Statement {
    acct_id: String,
    acct_type: String,
    credit_card: bool,
    balance: Option<f64>,
    txns: Vec<Txn>,
}

fn ofx_date(v: &str) -> Option<NaiveDate> {
    let d: String = v.chars().take(8).collect();
    NaiveDate::parse_from_str(&d, "%Y%m%d").ok()
}

fn ofx_amount(v: &str) -> Option<f64> {
    // Some banks write a decimal comma.
    let v = v.trim().replace(',', ".");
    v.parse::<f64>().ok().map(|x| (x * 100.0).round() / 100.0)
}

/// Undoes the few character entities OFX uses.
fn unescape(v: &str) -> String {
    v.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&apos;", "'").replace("&quot;", "\"").replace("&nbsp;", " ")
}

/// Reads a QFX/OFX file into rows for the import preview.
pub fn parse(text: &str) -> Result<Parsed, String> {
    if !looks_like_ofx(text) {
        return Err("It doesn't look like a QFX or OFX bank file.".into());
    }
    let start = text.find("<OFX>").or_else(|| text.to_uppercase().find("<OFX>")).unwrap_or(0);
    let body = &text[start..];
    // Walk the tags: "<TAG>value" (a value runs to the next "<"), "<TAG>" and "</TAG>".
    let mut org = String::new();
    let mut statements: Vec<Statement> = Vec::new();
    let mut cur: Option<Statement> = None;
    let mut txn: Option<Txn> = None;
    let mut path: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        let Some(gt) = rest.find('>') else { break };
        let tag = rest[..gt].trim().to_uppercase();
        rest = &rest[gt + 1..];
        let value_end = rest.find('<').unwrap_or(rest.len());
        let value = unescape(rest[..value_end].trim());
        if let Some(name) = tag.strip_prefix('/') {
            // Closing an aggregate (or an XML leaf).
            if let Some(pos) = path.iter().rposition(|p| p == name) {
                path.truncate(pos);
            }
            match name {
                "STMTTRN" => {
                    if let (Some(t), Some(s)) = (txn.take(), cur.as_mut()) {
                        s.txns.push(t);
                    }
                }
                "STMTRS" | "CCSTMTRS" => {
                    if let Some(s) = cur.take() {
                        statements.push(s);
                    }
                }
                _ => {}
            }
            continue;
        }
        if value.is_empty() {
            // Opening an aggregate.
            match tag.as_str() {
                "STMTRS" => cur = Some(Statement::default()),
                "CCSTMTRS" => cur = Some(Statement { credit_card: true, ..Default::default() }),
                "STMTTRN" => txn = Some(Txn::default()),
                _ => {}
            }
            path.push(tag);
            continue;
        }
        // A value.
        let inside = |name: &str| path.iter().any(|p| p == name);
        match tag.as_str() {
            "ORG" if inside("FI") && org.is_empty() => org = value,
            "ACCTID" if inside("BANKACCTFROM") || inside("CCACCTFROM") => {
                if let Some(s) = cur.as_mut() {
                    s.acct_id = value;
                }
            }
            "ACCTTYPE" if inside("BANKACCTFROM") => {
                if let Some(s) = cur.as_mut() {
                    s.acct_type = value.to_uppercase();
                }
            }
            "BALAMT" if inside("LEDGERBAL") => {
                if let Some(s) = cur.as_mut() {
                    s.balance = ofx_amount(&value);
                }
            }
            _ => {
                if let Some(t) = txn.as_mut() {
                    match tag.as_str() {
                        "DTPOSTED" => t.date = ofx_date(&value),
                        "TRNAMT" => t.amount = ofx_amount(&value),
                        "NAME" => {
                            if t.name.is_empty() {
                                t.name = value;
                            }
                        }
                        "MEMO" => t.memo = value,
                        "CHECKNUM" => t.number = value,
                        "TRNTYPE" => t.kind = value.to_uppercase(),
                        _ => {}
                    }
                }
            }
        }
        // An XML leaf closes itself with </TAG>; an SGML leaf doesn't, so don't track it.
    }
    if let Some(s) = cur.take() {
        statements.push(s);
    }

    let bank = if org.is_empty() { "Bank".to_string() } else { org };
    let mut out = Parsed {
        rows: Vec::new(),
        skipped: 0,
        scheduled: 0,
        kind_hint: None,
        accounts: Vec::new(),
        date_format: crate::import::DateFormat::IsoDash,
        account_kinds: BTreeMap::new(),
        masks: BTreeMap::new(),
        synthetic_openings: Default::default(),
    };
    for s in statements {
        let kind = if s.credit_card {
            AccountKind::CreditCard
        } else {
            match s.acct_type.as_str() {
                "SAVINGS" | "MONEYMRKT" | "CD" => AccountKind::Savings,
                "CREDITLINE" => AccountKind::CreditCard,
                _ => AccountKind::Checking,
            }
        };
        let digits: String = s.acct_id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let mask: String = digits.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        let kind_word = match kind {
            AccountKind::CreditCard => "Card",
            AccountKind::Savings => "Savings",
            _ => "Checking",
        };
        let name = if mask.is_empty() { format!("{bank} {kind_word}") } else { format!("{bank} {kind_word} ••{mask}") };
        if !out.accounts.contains(&name) {
            out.accounts.push(name.clone());
        }
        out.account_kinds.insert(name.clone(), kind);
        out.masks.insert(name.clone(), mask);
        let mut total = 0.0;
        let mut first: Option<NaiveDate> = None;
        for t in s.txns {
            let (Some(date), Some(amount)) = (t.date, t.amount) else {
                out.skipped += 1;
                continue;
            };
            total += amount;
            first = Some(first.map_or(date, |f| f.min(date)));
            let payee = if t.name.is_empty() { t.memo.clone() } else { t.name.clone() };
            let memo = if t.memo == payee { String::new() } else { t.memo.clone() };
            out.rows.push(crate::import::CsvRow {
                date,
                payee: payee.split_whitespace().collect::<Vec<_>>().join(" "),
                category: String::new(),
                amount,
                account: name.clone(),
                memo,
                number: t.number,
                cleared: Cleared::Cleared,
            });
        }
        // Start a new account so that it ends at the bank's balance.
        if let (Some(bal), Some(first)) = (s.balance, first) {
            let opening = ((bal - total) * 100.0).round() / 100.0;
            if opening.abs() > 0.004 {
                out.synthetic_openings.insert(out.rows.len());
                out.rows.push(crate::import::CsvRow {
                    date: first,
                    payee: "Opening Balance".into(),
                    category: "Adjustment".into(),
                    amount: opening,
                    account: name.clone(),
                    memo: String::new(),
                    number: String::new(),
                    cleared: Cleared::Cleared,
                });
            }
        }
    }
    if out.rows.is_empty() {
        return Err("No transactions were found in this bank file.".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{Plan, Target, apply, default_targets};

    const SGML: &str = "OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\n\n<OFX>\n<SIGNONMSGSRSV1><SONRS><FI><ORG>BECU<FID>1234</FI></SONRS></SIGNONMSGSRSV1>\n\
<BANKMSGSRSV1><STMTTRNRS><STMTRS><CURDEF>USD\n<BANKACCTFROM><BANKID>325081403<ACCTID>000123456789<ACCTTYPE>CHECKING</BANKACCTFROM>\n\
<BANKTRANLIST><DTSTART>20261101<DTEND>20261121\n\
<STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20261119120000.000[-8:PST]<TRNAMT>-87.33<FITID>A1<NAME>Amazon<MEMO>Household &amp; more</STMTTRN>\n\
<STMTTRN><TRNTYPE>CREDIT<DTPOSTED>20261120<TRNAMT>1500.00<FITID>A2<NAME>Payroll</STMTTRN>\n\
<STMTTRN><TRNTYPE>CHECK<DTPOSTED>20261121<TRNAMT>-40.00<FITID>A3<CHECKNUM>1042<NAME>Check 1042</STMTTRN>\n\
</BANKTRANLIST><LEDGERBAL><BALAMT>2372.67<DTASOF>20261121</LEDGERBAL></STMTRS></STMTTRNRS></BANKMSGSRSV1>\n\
<CREDITCARDMSGSRSV1><CCSTMTTRNRS><CCSTMTRS><CCACCTFROM><ACCTID>4111111111119999</CCACCTFROM><BANKTRANLIST>\n\
<STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20261118<TRNAMT>-12.50<FITID>C1<NAME>Coffee</STMTTRN>\n\
</BANKTRANLIST><LEDGERBAL><BALAMT>-512.50<DTASOF>20261121</LEDGERBAL></CCSTMTRS></CCSTMTTRNRS></CREDITCARDMSGSRSV1></OFX>\n";

    #[test]
    fn reads_a_qfx_file() {
        let p = parse(SGML).unwrap();
        assert_eq!(p.accounts, vec!["BECU Checking ••6789".to_string(), "BECU Card ••9999".to_string()]);
        assert_eq!(p.account_kinds.get("BECU Card ••9999"), Some(&AccountKind::CreditCard));
        assert_eq!(p.rows[0].payee, "Amazon");
        assert_eq!(p.rows[0].memo, "Household & more");
        assert_eq!(p.rows[0].amount, -87.33);
        assert_eq!(p.rows[0].date, NaiveDate::from_ymd_opt(2026, 11, 19).unwrap());
        assert_eq!(p.rows[2].number, "1042");
        // A new account ends at the bank's balance: 2372.67 = opening + 1372.67.
        let mut l = Ledger::default();
        let t = default_targets(&l, &p);
        let plan = Plan::build(&l, &p, &t);
        apply(&mut l, &p, &t, &plan);
        let chk = l.accounts.iter().find(|a| a.name == "BECU Checking ••6789").unwrap().id;
        assert!((l.balance(chk) - 2372.67).abs() < 0.001);
        let card = l.accounts.iter().find(|a| a.name == "BECU Card ••9999").unwrap();
        assert_eq!(card.kind, AccountKind::CreditCard);
        assert!((l.balance(card.id) + 512.50).abs() < 0.001);
        // Importing it again adds nothing, and the made-up starting balance
        // isn't added to accounts that already exist.
        let t2 = default_targets(&l, &p);
        assert!(matches!(t2.get("BECU Checking ••6789"), Some(Target::Existing(_))));
        let plan2 = Plan::build(&l, &p, &t2);
        assert_eq!(plan2.adds.len(), 0);
    }

    #[test]
    fn reads_xml_ofx_and_matches_by_last_four() {
        let xml = "<?xml version=\"1.0\"?><?OFX OFXHEADER=\"200\"?><OFX><SIGNONMSGSRSV1><SONRS><FI><ORG>Chase</ORG></FI></SONRS></SIGNONMSGSRSV1>\
<BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKACCTFROM><ACCTID>99991111</ACCTID><ACCTTYPE>SAVINGS</ACCTTYPE></BANKACCTFROM><BANKTRANLIST>\
<STMTTRN><TRNTYPE>INT</TRNTYPE><DTPOSTED>20261101</DTPOSTED><TRNAMT>0.32</TRNAMT><FITID>X</FITID><NAME>Interest</NAME></STMTTRN>\
</BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";
        let p = parse(xml).unwrap();
        assert_eq!(p.accounts, vec!["Chase Savings ••1111".to_string()]);
        assert_eq!(p.rows.len(), 1);
        assert_eq!(p.rows[0].amount, 0.32);
        let mut l = Ledger::default();
        let d = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        l.accounts.push(Account { id: 7, name: "Rainy Day 1111".into(), kind: AccountKind::Savings, opening_balance: 0.0, opening_date: d, closed: false });
        assert_eq!(default_targets(&l, &p).get("Chase Savings ••1111"), Some(&Target::Existing(7)));
        assert!(parse("Date,Amount\n").is_err());
    }
}

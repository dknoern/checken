//! Printing a report: the report is laid out as a print-ready page (title,
//! date range, a bar chart and the table) and opened, where the system's
//! Print window comes up straight away.

use super::*;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Bars for the chart: label and up to two values with their colours.
struct Bar {
    label: String,
    values: Vec<(f64, &'static str)>,
}

/// Picks the chart from the table's columns for each kind of report.
fn bars(kind: ReportKind, header: &[String], rows: &[Vec<String>], bold: &[usize]) -> Vec<Bar> {
    let num = |row: &Vec<String>, i: usize| row.get(i).and_then(|c| parse_money(c)).unwrap_or(0.0);
    let _ = header;
    rows.iter()
        .enumerate()
        .filter(|(i, _)| !bold.contains(i))
        .map(|(_, r)| {
            let label = r.first().cloned().unwrap_or_default();
            let values = match kind {
                ReportKind::Spending => vec![(num(r, 1), "#2b62c4")],
                ReportKind::BudgetVsActual => {
                    let (b, a) = (num(r, 1), num(r, 2));
                    vec![(b, "#bdbdbd"), (a, if a > b + 0.004 { "#d02c2c" } else { "#2f9a3a" })]
                }
                ReportKind::IncomeExpense => vec![(num(r, 1), "#2f9a3a"), (num(r, 2), "#d02c2c")],
                ReportKind::NetWorth => vec![(num(r, 3), "#2b62c4")],
            };
            Bar { label, values }
        })
        .collect()
}

/// A horizontal bar chart as SVG.
fn chart_svg(bars: &[Bar]) -> String {
    let max = bars.iter().flat_map(|b| b.values.iter().map(|v| v.0.abs())).fold(1.0, f64::max);
    let (label_w, bar_w, value_w) = (170.0, 380.0, 150.0);
    let per = |b: &Bar| if b.values.len() > 1 { 26.0 } else { 20.0 };
    let height: f64 = bars.iter().map(per).sum::<f64>() + 6.0;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{height}" viewBox="0 0 {w} {height}" font-family="-apple-system, Helvetica, Arial, sans-serif" font-size="11">"#,
        w = label_w + bar_w + value_w
    );
    let mut y = 3.0;
    for b in bars {
        let h = per(b);
        svg += &format!(r##"<text x="0" y="{}" dominant-baseline="middle" fill="#222">{}</text>"##, y + h / 2.0, esc(&b.label));
        let thick = if b.values.len() > 1 { 9.0 } else { 12.0 };
        for (i, (v, col)) in b.values.iter().enumerate() {
            let w = (v.abs() / max * bar_w).max(1.0);
            let by = y + (h - thick * b.values.len() as f64 - 2.0 * (b.values.len() as f64 - 1.0)) / 2.0 + i as f64 * (thick + 2.0);
            svg += &format!(r#"<rect x="{label_w}" y="{by}" width="{w}" height="{thick}" rx="2" fill="{col}"/>"#);
        }
        let text = b.values.iter().map(|(v, _)| money(*v)).collect::<Vec<_>>().join(" / ");
        svg += &format!(
            r##"<text x="{}" y="{}" dominant-baseline="middle" text-anchor="end" fill="#333">{}</text>"##,
            label_w + bar_w + value_w,
            y + h / 2.0,
            esc(&text)
        );
        y += h;
    }
    svg + "</svg>"
}

fn legend(kind: ReportKind) -> &'static str {
    match kind {
        ReportKind::BudgetVsActual => {
            r#"<span class="key" style="background:#bdbdbd"></span>Budget <span class="key" style="background:#2f9a3a"></span>Actual, within budget <span class="key" style="background:#d02c2c"></span>Actual, over budget"#
        }
        ReportKind::IncomeExpense => r#"<span class="key" style="background:#2f9a3a"></span>Income <span class="key" style="background:#d02c2c"></span>Expenses"#,
        _ => "",
    }
}

impl App {
    /// Lays the report out for printing and opens it with the Print window.
    pub(super) fn print_report(&mut self, name: &str, description: &str, kind: ReportKind, period: Period) {
        let (from, to) = period.range();
        let table = self.report_table(kind, from, to);
        let chart = chart_svg(&bars(kind, &table.header, &table.rows, &table.bold));
        let mut rows_html = String::new();
        for (i, row) in table.rows.iter().enumerate() {
            let class = if table.bold.contains(&i) { " class=\"total\"" } else { "" };
            rows_html += &format!("<tr{class}>");
            for (j, cell) in row.iter().enumerate() {
                let num = if j > 0 { " class=\"num\"" } else { "" };
                let neg = if j > 0 && cell.starts_with('-') { " style=\"color:#c0262b\"" } else { "" };
                rows_html += &format!("<td{num}{neg}>{}</td>", esc(cell));
            }
            rows_html += "</tr>";
        }
        let head_html: String = table
            .header
            .iter()
            .enumerate()
            .map(|(j, h)| format!("<th{}>{}</th>", if j > 0 { " class=\"num\"" } else { "" }, esc(h)))
            .collect();
        let html = format!(
            r#"<!doctype html>
<html><head><meta charset="utf-8"><title>{title}</title>
<style>
  @page {{ margin: 0.6in; }}
  body {{ font-family: -apple-system, "Helvetica Neue", Helvetica, Arial, sans-serif; color: #1d1d1f; font-size: 11pt; margin: 24px; }}
  .top {{ display: flex; justify-content: space-between; align-items: baseline; border-bottom: 1px solid #ccc; padding-bottom: 8px; }}
  h1 {{ font-size: 20pt; font-weight: 500; margin: 0; }}
  .app {{ color: #888; font-size: 9pt; }}
  .sub {{ color: #555; margin: 6px 0 2px; }}
  .range {{ color: #555; font-size: 10pt; margin-bottom: 14px; }}
  .chart {{ margin: 10px 0 6px; }}
  .legend {{ font-size: 9pt; color: #555; margin-bottom: 14px; }}
  .key {{ display: inline-block; width: 10px; height: 10px; border-radius: 2px; margin: 0 4px 0 10px; vertical-align: -1px; }}
  table {{ border-collapse: collapse; width: 100%; font-size: 10pt; }}
  th {{ text-align: left; font-weight: 500; color: #666; text-transform: uppercase; letter-spacing: 0.06em; font-size: 8pt; border-bottom: 1px solid #c9cfa8; padding: 6px 8px; }}
  td {{ padding: 5px 8px; border-bottom: 1px solid #eee; }}
  tr:nth-child(even) td {{ background: #f6f6f6; }}
  .num {{ text-align: right; font-variant-numeric: tabular-nums; }}
  tr.total td {{ font-weight: 600; border-top: 1px solid #999; background: #fff; }}
  .foot {{ margin-top: 16px; color: #999; font-size: 8pt; }}
  tr {{ page-break-inside: avoid; }}
</style></head>
<body onload="window.print()">
  <div class="top"><h1>{title}</h1><span class="app">Checken</span></div>
  <div class="sub">{desc}</div>
  <div class="range">{range_label}: {from} – {to}</div>
  <div class="chart">{chart}</div>
  <div class="legend">{legend}</div>
  <table><thead><tr>{head}</tr></thead><tbody>{rows}</tbody></table>
  <div class="foot">Printed {printed}</div>
</body></html>"#,
            title = esc(name),
            desc = esc(description),
            range_label = esc(period.label()),
            from = fmt_date(from),
            to = fmt_date(to),
            chart = chart,
            legend = legend(kind),
            head = head_html,
            rows = rows_html,
            printed = chrono::Local::now().format("%B %-d, %Y at %-I:%M %p"),
        );
        let dir = std::env::temp_dir().join("Checken Print");
        let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let path = dir.join(format!("{}.html", safe.trim()));
        let written = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, html));
        if let Err(e) = written {
            self.status = format!("Couldn't prepare the report for printing: {e}");
            return;
        }
        if self.shot.is_some() {
            return;
        }
        self.status = match open_file(&path) {
            Ok(()) => format!("Opened “{name}” for printing. Choose your printer in the Print window."),
            Err(e) => format!("Couldn't open the report for printing: {e}"),
        };
    }

    /// File > Print Report: prints the open report, or the one selected in the list.
    pub(super) fn print_current(&mut self) {
        if self.view != View::Reports {
            self.status = "Open a report, then choose Print.".into();
            return;
        }
        if let Some(e) = self.report_open.and_then(|k| self.report_entry(k)) {
            let (kind, period) = (self.report, self.period);
            self.print_report(&e.name, &e.description, kind, period);
        } else if let Some(e) = self.report_sel.and_then(|k| self.report_entry(k)) {
            self.print_report(&e.name, &e.description, e.kind, e.period);
        } else {
            self.status = "Select a report to print.".into();
        }
    }
}

/// Opens a file with the app the system uses for it.
fn open_file(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(path).spawn().map(|_| ())
}

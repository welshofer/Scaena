//! SEC EDGAR as a source (Vzestup's `edgar/`, PLAN G.4): a ticker or a company's name resolved
//! to its CIK, its latest filings of the forms asked chosen, each filing's document as text,
//! its tables as tab-separated rows, under a header that says which filings it holds and where
//! each is. Made here as plain functions over what the client fetches: SEC asks every request
//! to carry a contact in its User-Agent, which is the user's own (a setting, never a name baked
//! in), and serves no browser.

use serde_json::Value;

/// The list of every company's ticker and CIK.
pub const TICKERS_URL: &str = "https://www.sec.gov/files/company_tickers.json";
/// The most a filing's text takes (Vzestup's `PER_FILING_TEXT_CAP`).
pub const PER_FILING_CAP: usize = 350_000;
/// The most the source takes (Vzestup's `COMPOSITE_TEXT_CAP`).
pub const COMPOSITE_CAP: usize = 800_000;
/// The forms asked for where the user names none: the latest annual and quarterly reports.
pub const DEFAULT_FORMS: [&str; 2] = ["10-K", "10-Q"];

/// The hosts a request for SEC's data may go to: no address a user typed reaches the client.
pub fn allowed(url: &str) -> bool {
    let host = url.strip_prefix("https://").and_then(|r| r.split(['/', '?']).next()).unwrap_or_default();
    matches!(host, "data.sec.gov" | "www.sec.gov")
}

/// The headers every request to SEC carries: its User-Agent the user's contact.
pub fn headers(contact: &str) -> Vec<(String, String)> {
    vec![("user-agent".into(), format!("Scaena {contact}")), ("accept".into(), "application/json, text/html".into())]
}

/// A company SEC lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Company {
    /// Ten digits, zero-padded.
    pub cik: String,
    pub ticker: String,
    pub name: String,
}

/// A name as matched: lower case, its punctuation and corporate words gone.
fn normalized(name: &str) -> String {
    let lower = name.to_lowercase().replace(['.', ','], "");
    let words: Vec<&str> = lower
        .split_whitespace()
        .filter(|w| {
            !matches!(
                *w,
                "inc"
                    | "corp"
                    | "corporation"
                    | "incorporated"
                    | "company"
                    | "co"
                    | "ltd"
                    | "llc"
                    | "plc"
                    | "holdings"
                    | "holding"
                    | "group"
                    | "the"
            )
        })
        .collect();
    words.join(" ")
}

/// The companies the ticker list holds.
pub fn companies(tickers: &Value) -> Vec<Company> {
    tickers
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
        .filter_map(|e| {
            let cik = match &e["cik_str"] {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => return None,
            };
            Some(Company {
                cik: format!("{cik:0>10}"),
                ticker: e["ticker"].as_str()?.to_uppercase(),
                name: e["title"].as_str()?.to_string(),
            })
        })
        .collect()
}

/// The one company `query` names (Vzestup's `resolveTickerToCompany`): its ticker (`$` or not),
/// its name as written, or the one name that holds it; none where it names none, or many.
pub fn resolve(companies: &[Company], query: &str) -> Option<Company> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let ticker = q.trim_start_matches('$').to_uppercase();
    if let Some(hit) = companies.iter().find(|c| c.ticker == ticker) {
        return Some(hit.clone());
    }
    let name = normalized(q);
    if name.is_empty() {
        return None;
    }
    if let Some(hit) = companies.iter().find(|c| normalized(&c.name) == name) {
        return Some(hit.clone());
    }
    let mut holding = companies.iter().filter(|c| normalized(&c.name).contains(&name));
    match (holding.next(), holding.next()) {
        (Some(one), None) => Some(one.clone()),
        _ => None,
    }
}

/// The URL of a company's filings index.
pub fn submissions_url(cik: &str) -> String {
    format!("https://data.sec.gov/submissions/CIK{cik:0>10}.json")
}

/// A filing: its form, when it was filed and the period it covers, and its document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filing {
    pub form: String,
    pub accession: String,
    pub filed: String,
    pub period: Option<String>,
    pub url: String,
}

/// The recent filings a company's index lists, newest first, as SEC lists them.
pub fn filings(submissions: &Value) -> Vec<Filing> {
    let recent = &submissions["filings"]["recent"];
    let list = |k: &str| recent[k].as_array().cloned().unwrap_or_default();
    let (accessions, forms, filed, periods, documents) =
        (list("accessionNumber"), list("form"), list("filingDate"), list("reportDate"), list("primaryDocument"));
    let cik = match &submissions["cik"] {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.trim_start_matches('0').to_string(),
        _ => String::new(),
    };
    (0..accessions.len())
        .filter_map(|i| {
            let accession = accessions[i].as_str()?;
            let document = documents.get(i)?.as_str().filter(|d| !d.is_empty())?;
            Some(Filing {
                form: forms.get(i)?.as_str()?.to_string(),
                accession: accession.to_string(),
                filed: filed.get(i)?.as_str()?.to_string(),
                period: periods.get(i).and_then(Value::as_str).filter(|p| !p.is_empty()).map(str::to_string),
                url: format!("https://www.sec.gov/Archives/edgar/data/{cik}/{}/{document}", accession.replace('-', "")),
            })
        })
        .collect()
}

/// The latest filing of each form in `forms`, in that order (Vzestup's `pickLatestByForm`).
pub fn latest(filings: &[Filing], forms: &[&str]) -> Vec<Filing> {
    forms.iter().filter_map(|form| filings.iter().find(|f| f.form == *form).cloned()).collect()
}

/// The URL of a company's XBRL facts, every figure it has tagged in its filings.
pub fn facts_url(cik: &str) -> String {
    format!("https://data.sec.gov/api/xbrl/companyfacts/CIK{cik:0>10}.json")
}

/// The yearly figures read from a company's XBRL facts: each a label, its unit, its scale,
/// and the concepts that may carry it, the first that has a year giving it.
const FIGURES: [(&str, &str, f64, &[&str]); 6] = [
    (
        "Revenue ($M)",
        "USD",
        1e-6,
        &[
            "RevenueFromContractWithCustomerExcludingAssessedTax",
            "Revenues",
            "SalesRevenueNet",
            "RevenueFromContractWithCustomerIncludingAssessedTax",
        ],
    ),
    ("Gross profit ($M)", "USD", 1e-6, &["GrossProfit"]),
    ("Operating income ($M)", "USD", 1e-6, &["OperatingIncomeLoss"]),
    ("Net income ($M)", "USD", 1e-6, &["NetIncomeLoss", "ProfitLoss"]),
    ("R&D ($M)", "USD", 1e-6, &["ResearchAndDevelopmentExpense"]),
    ("Diluted EPS ($)", "USD/shares", 1.0, &["EarningsPerShareDiluted"]),
];

/// How many years of figures are kept: the latest.
const YEARS: usize = 6;

/// A company's yearly figures from its XBRL facts (PLAN G.4): revenue, profit, income, R&D,
/// and diluted earnings per share, a row for each of its latest years, money in millions of
/// dollars. A year is the calendar year SEC frames a fiscal year in (`CY2024`), so a value
/// restated later is counted once. None where fewer than two years are found.
pub fn financials(facts: &Value, ticker: &str) -> Option<crate::Sheet> {
    let gaap = &facts["facts"]["us-gaap"];
    let mut columns: Vec<std::collections::BTreeMap<i64, f64>> = Vec::new();
    for (_, unit, scale, concepts) in FIGURES {
        let mut by_year = std::collections::BTreeMap::new();
        for concept in concepts {
            for fact in gaap[concept]["units"][unit].as_array().into_iter().flatten() {
                let year =
                    fact["frame"].as_str().and_then(|f| f.strip_prefix("CY")).and_then(|y| y.parse::<i64>().ok());
                if let (Some(year), Some(value)) = (year, fact["val"].as_f64()) {
                    by_year.entry(year).or_insert(value * scale);
                }
            }
        }
        columns.push(by_year);
    }
    // The years revenue or net income is told for, the latest kept.
    let mut years: Vec<i64> = columns[0].keys().chain(columns[3].keys()).copied().collect();
    years.sort_unstable();
    years.dedup();
    let years = &years[years.len().saturating_sub(YEARS)..];
    if years.len() < 2 {
        return None;
    }
    let header = std::iter::once("Year".to_string()).chain(FIGURES.iter().map(|(label, ..)| label.to_string()));
    let mut rows = vec![header.collect::<Vec<_>>()];
    for year in years {
        let cells = (columns.iter().zip(FIGURES))
            .map(|(c, (_, _, scale, _))| c.get(year).map(|v| figure(*v, if scale < 1.0 { 1 } else { 2 })));
        let cells = cells.map(Option::unwrap_or_default);
        rows.push(std::iter::once(year.to_string()).chain(cells).collect());
    }
    // A figure no year tells is left out.
    let told: Vec<usize> =
        (0..rows[0].len()).filter(|&c| c == 0 || rows[1..].iter().any(|r| !r[c].is_empty())).collect();
    let rows = rows.into_iter().map(|r| told.iter().map(|&c| r[c].clone()).collect()).collect();
    Some(crate::Sheet { name: format!("{} financials", ticker.to_lowercase()), rows })
}

/// A figure as a table writes it: to `places` decimals (a tenth of a million, a cent a share),
/// its trailing zeros and a bare point left off.
fn figure(v: f64, places: usize) -> String {
    let written = format!("{v:.places$}");
    if written.contains('.') { written.trim_end_matches('0').trim_end_matches('.').to_string() } else { written }
}

/// A filing's document as text (Vzestup's `filingHtmlToText`): its headings marked by their
/// level, a list's items by a dash, each table's rows a line, cells apart by tabs; scripts,
/// styles, and its head left out.
pub fn filing_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::new();
    let mut i = 0;
    let s = html.as_bytes();
    let mut skipping: Option<&'static str> = None;
    while i < s.len() {
        if s[i] != b'<' {
            let end = html[i..].find('<').map_or(s.len(), |e| i + e);
            if skipping.is_none() {
                out.push_str(&entities(&html[i..end]));
            }
            i = end;
            continue;
        }
        if lower[i..].starts_with("<!--") {
            i = lower[i..].find("-->").map_or(s.len(), |e| i + e + 3);
            continue;
        }
        let end = lower[i..].find('>').map_or(s.len(), |e| i + e + 1);
        let tag = &lower[i + 1..end.saturating_sub(1).max(i + 1)];
        let closing = tag.starts_with('/');
        let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        if let Some(skip) = skipping {
            if closing && name == skip {
                skipping = None;
            }
            i = end;
            continue;
        }
        match name.as_str() {
            "script" | "style" | "head" if !closing => {
                skipping = Some(match name.as_str() {
                    "script" => "script",
                    "style" => "style",
                    _ => "head",
                })
            }
            "table" if !closing => {
                let close = lower[i..].find("</table").map_or(s.len(), |e| i + e);
                let rows = table(&html[end.min(close)..close]);
                if !rows.is_empty() {
                    out.push('\n');
                    out.push_str(&rows);
                    out.push('\n');
                }
                i = lower[close..].find('>').map_or(s.len(), |e| close + e + 1);
                continue;
            }
            "br" => out.push('\n'),
            "li" if !closing => out.push_str("\n- "),
            h if h.len() == 2 && h.starts_with('h') && h.as_bytes()[1].is_ascii_digit() => {
                let level = (h.as_bytes()[1] - b'0').clamp(1, 6) as usize;
                out.push_str("\n\n");
                if !closing {
                    out.push_str(&"#".repeat(level));
                    out.push(' ');
                }
            }
            "p" | "div" | "tr" | "section" | "article" if closing => out.push('\n'),
            _ => {}
        }
        i = end;
    }
    tidy(&out)
}

/// A table's rows, a line each, its cells apart by tabs; rows with nothing in them left out.
fn table(inner: &str) -> String {
    let lower = inner.to_ascii_lowercase();
    let mut rows = Vec::new();
    for row in lower.match_indices("<tr").map(|(at, _)| at) {
        let row_end = lower[row..].find("</tr").map_or(lower.len(), |e| row + e);
        let mut cells = Vec::new();
        let mut at = row;
        while let Some(start) = lower[at..row_end].find("<t").map(|e| at + e) {
            let kind = lower.as_bytes().get(start + 2).copied();
            if !matches!(kind, Some(b'd' | b'h')) {
                at = start + 2;
                continue;
            }
            let open_end = lower[start..].find('>').map_or(row_end, |e| start + e + 1);
            let close = lower[open_end..row_end].find("</t").map_or(row_end, |e| open_end + e);
            let text = strip(&inner[open_end..close]);
            cells.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
            at = close.max(start + 2);
        }
        if cells.iter().any(|c| !c.is_empty()) {
            rows.push(cells.join("\t"));
        }
    }
    rows.join("\n")
}

/// Text with its tags gone and its entities written out.
fn strip(html: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            c if !inside => out.push(c),
            _ => {}
        }
    }
    entities(&out)
}

/// The entities a filing writes, written out; the invisible marks a filing's HTML carries left
/// out.
fn entities(text: &str) -> String {
    let named = [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&apos;", "'"),
        ("&nbsp;", " "),
        ("&ndash;", "\u{2013}"),
        ("&mdash;", "\u{2014}"),
        ("&hellip;", "\u{2026}"),
        ("&trade;", "\u{2122}"),
        ("&reg;", "\u{ae}"),
        ("&copy;", "\u{a9}"),
        ("&rsquo;", "'"),
        ("&lsquo;", "'"),
        ("&rdquo;", "\""),
        ("&ldquo;", "\""),
        ("&bull;", "\u{2022}"),
        ("&middot;", "\u{b7}"),
    ];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let lower = rest.get(..10).unwrap_or(rest).to_ascii_lowercase();
        if let Some((entity, ch)) = named.iter().find(|(e, _)| lower.starts_with(e)) {
            out.push_str(ch);
            rest = &rest[entity.len()..];
            continue;
        }
        let code = rest.strip_prefix("&#").and_then(|r| {
            let end = r.find(';')?;
            let n = &r[..end];
            let v = match n.strip_prefix(['x', 'X']) {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => n.parse().ok()?,
            };
            Some((char::from_u32(v)?, end + 3))
        });
        match code {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out.chars()
        .filter(|c| !matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}'))
        .collect()
}

/// Spaces as one, lines trimmed, no more than one blank line in a row.
fn tidy(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line: String = line.split([' ', '\u{a0}']).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
        let line = line.trim().to_string();
        if line.is_empty() && lines.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_string()
}

/// The source the filings make (Vzestup's `composeEdgarSource`): its name, and its text, a
/// header saying which filings it holds and where each is, then each filing's text, cut where it
/// runs past its room.
pub fn compose(company: &Company, filed: &[(Filing, String)], figures: Option<&crate::Sheet>) -> (String, String) {
    let named: Vec<String> = filed.iter().map(|(f, _)| format!("{} ({})", f.form, f.filed)).collect();
    let name = format!("SEC: {} \u{2014} {}", company.ticker, named.join(", "));
    let mut text = vec![
        format!("# {} ({}) \u{2014} SEC filings", company.name, company.ticker),
        String::new(),
        format!("CIK: {}", company.cik),
        "Filings included:".to_string(),
    ];
    for (f, _) in filed {
        let period = f.period.as_ref().map(|p| format!(" (period {p})")).unwrap_or_default();
        text.push(format!("  \u{2022} {} filed {}{period} \u{2014} {}", f.form, f.filed, f.url));
    }
    text.push(String::new());
    text.push(
        "All facts below are sourced verbatim from these filings on SEC.gov. Cite specifically when used in a slide."
            .into(),
    );
    text.push(String::new());
    let mut composite = text.join("\n");
    // The figures before the filings, so a filing cut short never takes them.
    if let Some(figures) = figures {
        let rule = "=".repeat(64);
        let table: Vec<String> = figures.rows.iter().map(|r| r.join("\t")).collect();
        composite.push_str(&format!(
            "\n\n{rule}\nFinancial figures (XBRL), by year as SEC frames them\n{rule}\n\nEach year is the calendar year a fiscal year mostly covers. Money is in millions of US dollars.\n\n{}",
            table.join("\n")
        ));
    }
    for (f, body) in filed {
        let (body, _) = cut(body, PER_FILING_CAP);
        let period = f.period.as_ref().map(|p| format!(" (period {p})")).unwrap_or_default();
        let rule = "=".repeat(64);
        composite.push_str(&format!(
            "\n\n{rule}\n{} \u{2014} filed {}{period}\nAccession: {}\nSource: {}\n{rule}\n\n{body}",
            f.form, f.filed, f.accession, f.url
        ));
    }
    (name, cut(&composite, COMPOSITE_CAP).0)
}

/// `text` held to `most` bytes, marked as Vzestup marks a filing cut short.
fn cut(text: &str, most: usize) -> (String, bool) {
    let marker = "\n\u{2026}[filing text truncated for length]";
    if text.len() <= most {
        return (text.to_string(), false);
    }
    let mut end = most.saturating_sub(marker.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}{marker}", &text[..end]), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn listed() -> Vec<Company> {
        companies(&json!({
            "0": { "cik_str": 320193, "ticker": "AAPL", "title": "Apple Inc." },
            "1": { "cik_str": 789019, "ticker": "MSFT", "title": "MICROSOFT CORP" },
            "2": { "cik_str": 1018724, "ticker": "AMZN", "title": "AMAZON COM INC" },
            "3": { "cik_str": 1652044, "ticker": "GOOGL", "title": "Alphabet Inc." },
            "4": { "cik_str": 1652044, "ticker": "GOOG", "title": "Alphabet Inc." }
        }))
    }

    #[test]
    fn a_query_names_one_company_or_none() {
        let c = listed();
        assert_eq!(resolve(&c, "$msft").unwrap().cik, "0000789019");
        assert_eq!(resolve(&c, "Apple").unwrap().ticker, "AAPL");
        assert_eq!(resolve(&c, "microsoft corporation").unwrap().ticker, "MSFT");
        assert_eq!(resolve(&c, "amazon").unwrap().ticker, "AMZN");
        assert_eq!(resolve(&c, "a"), None, "many hold it");
        assert_eq!(resolve(&c, "Tidewater"), None);
        assert!(allowed("https://data.sec.gov/submissions/CIK0000320193.json"));
        assert!(!allowed("https://169.254.169.254/latest"));
    }

    #[test]
    fn the_latest_of_each_form_is_chosen_and_its_document_named() {
        let s = json!({ "cik": "320193", "filings": { "recent": {
            "accessionNumber": ["0000320193-24-000123", "0000320193-24-000081", "0000320193-23-000106"],
            "form": ["10-K", "10-Q", "10-K"],
            "filingDate": ["2024-11-01", "2024-08-02", "2023-11-03"],
            "reportDate": ["2024-09-28", "2024-06-29", ""],
            "primaryDocument": ["aapl-20240928.htm", "aapl-20240629.htm", "aapl-20230930.htm"]
        } } });
        let all = filings(&s);
        let picked = latest(&all, &["10-Q", "10-K", "8-K"]);
        assert_eq!(picked.len(), 2);
        assert_eq!(picked[0].form, "10-Q");
        assert_eq!(
            picked[1].url,
            "https://www.sec.gov/Archives/edgar/data/320193/000032019324000123/aapl-20240928.htm"
        );
        assert_eq!(all[2].period, None);
    }

    #[test]
    fn a_filing_reads_as_text_with_its_tables_as_rows() {
        let html = r#"<html><head><title>x</title><style>td{}</style></head><body>
<h2>Item 7. Management&#8217;s Discussion</h2><p>Net sales rose&nbsp;2%.<!-- note --></p>
<table><tr><td>Segment</td><td>2024</td></tr><tr><td><span>Americas</span></td><td>$167,045</td></tr><tr><td></td><td></td></tr></table>
<ul><li>iPhone</li><li>Mac</li></ul><script>var a="<p>";</script></body></html>"#;
        assert_eq!(
            filing_text(html),
            "## Item 7. Management\u{2019}s Discussion\n\nNet sales rose 2%.\n\nSegment\t2024\nAmericas\t$167,045\n\n- iPhone\n- Mac"
        );
    }

    #[test]
    fn a_companys_facts_are_its_yearly_figures() {
        let fact = |frame: &str, val: f64| json!({ "frame": frame, "val": val, "form": "10-K" });
        let facts = json!({ "facts": { "us-gaap": {
            "SalesRevenueNet": { "units": { "USD": [fact("CY2016", 215_639_000_000.0), fact("CY2017", 229_234_000_000.0)] } },
            "RevenueFromContractWithCustomerExcludingAssessedTax": { "units": { "USD": [
                fact("CY2018", 265_595_000_000.0), fact("CY2018Q4", 84_310_000_000.0), { "val": 1.0, "form": "10-K" }
            ] } },
            "NetIncomeLoss": { "units": { "USD": [fact("CY2017", 48_351_000_000.0), fact("CY2018", 59_531_000_000.0)] } },
            "EarningsPerShareDiluted": { "units": { "USD/shares": [fact("CY2018", 2.97)] } }
        } } });
        let sheet = financials(&facts, "AAPL").unwrap();
        assert_eq!(sheet.name, "aapl financials");
        let rows: Vec<String> = sheet.rows.iter().map(|r| r.join(",")).collect();
        assert_eq!(
            rows,
            [
                "Year,Revenue ($M),Net income ($M),Diluted EPS ($)",
                "2016,215639,,",
                "2017,229234,48351,",
                "2018,265595,59531,2.97"
            ]
        );
        assert_eq!(financials(&json!({ "facts": {} }), "X"), None);
        assert_eq!(facts_url("320193"), "https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json");
    }

    #[test]
    fn the_source_says_what_it_holds() {
        let company = Company { cik: "0000320193".into(), ticker: "AAPL".into(), name: "Apple Inc.".into() };
        let f = Filing {
            form: "10-K".into(),
            accession: "0000320193-24-000123".into(),
            filed: "2024-11-01".into(),
            period: Some("2024-09-28".into()),
            url: "https://www.sec.gov/x.htm".into(),
        };
        let (name, text) = compose(&company, &[(f, "Net sales rose 2%.".into())], None);
        assert_eq!(name, "SEC: AAPL \u{2014} 10-K (2024-11-01)");
        assert!(text.starts_with("# Apple Inc. (AAPL) \u{2014} SEC filings\n\nCIK: 0000320193\nFilings included:\n"));
        assert!(
            text.contains("  \u{2022} 10-K filed 2024-11-01 (period 2024-09-28) \u{2014} https://www.sec.gov/x.htm")
        );
        assert!(text.ends_with("Source: https://www.sec.gov/x.htm\n================================================================\n\nNet sales rose 2%."));
    }
}

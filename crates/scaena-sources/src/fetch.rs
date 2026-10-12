//! Sources fetched over the network, each a plan the client carries out (PLAN G.4): a
//! company's latest filings on SEC's EDGAR, a GitHub repository, and a web page. Each step is a
//! GET the client makes and hands the answer back to; the client chooses nothing but how to make
//! a request, so the CLI, the browser, and the Mac read a source alike.
//!
//! A browser reads only what a host lets another page read (CORS, PLAN G.5): a repository there
//! is read from its tree and its files one by one (GitHub's API and raw.githubusercontent.com
//! allow it; the host of its archive does not), and SEC, whose requests must name who makes them
//! in a header a browser cannot set, and Google, which lets no other page read its exports, are
//! refused with the way round.

use crate::{Kind, Source, SourceError, github, html, sec};

/// A source named over the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remote {
    /// `sec:TICKER`, or `sec:TICKER:10-K,8-K`: the company's latest filing of each form.
    Sec { query: String, forms: Vec<String> },
    /// `github:OWNER/REPO`, or a GitHub address.
    GitHub(github::RepoRef),
    /// A Google document, spreadsheet, or presentation shared by link, or a file in Drive.
    Google { kind: Google, id: String },
    /// A web address.
    Web(String),
}

/// What a Google address names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Google {
    Document,
    Spreadsheet,
    Presentation,
    /// A file kept in Drive: a PDF or an Office file, read by what its bytes say.
    File,
}

impl Google {
    /// Where it is fetched as a file: a document as Word's, a spreadsheet as Excel's, a
    /// presentation as PowerPoint's, which the readers here read.
    fn export(self, id: &str) -> String {
        match self {
            Google::Document => format!("https://docs.google.com/document/d/{id}/export?format=docx"),
            Google::Spreadsheet => format!("https://docs.google.com/spreadsheets/d/{id}/export?format=xlsx"),
            Google::Presentation => format!("https://docs.google.com/presentation/d/{id}/export/pptx"),
            Google::File => format!("https://drive.google.com/uc?export=download&id={id}"),
        }
    }

    fn named(self) -> &'static str {
        match self {
            Google::Document => "Google document",
            Google::Spreadsheet => "Google spreadsheet",
            Google::Presentation => "Google presentation",
            Google::File => "Drive file",
        }
    }
}

/// The Google file an address names: `docs.google.com/document/d/ID/…`, a spreadsheet's or a
/// presentation's alike, or `drive.google.com/file/d/ID/…` (or `open?id=ID`).
fn google(url: &str) -> Option<(Google, String)> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = path.split('#').next().unwrap_or(path);
    let segments: Vec<&str> = path.split(['/', '?']).collect();
    let id_after_d = || {
        let at = segments.iter().position(|s| *s == "d")?;
        segments.get(at + 1).filter(|id| valid_id(id)).map(|id| id.to_string())
    };
    match host {
        "docs.google.com" => {
            let kind = match *segments.first()? {
                "document" => Google::Document,
                "spreadsheets" => Google::Spreadsheet,
                "presentation" => Google::Presentation,
                _ => return None,
            };
            Some((kind, id_after_d()?))
        }
        "drive.google.com" => {
            let id = id_after_d().or_else(|| {
                let query = path.split_once('?')?.1;
                let id = query.split('&').find_map(|kv| kv.strip_prefix("id="))?;
                valid_id(id).then(|| id.to_string())
            })?;
            Some((Google::File, id))
        }
        _ => None,
    }
}

/// A Google file's id: letters, digits, `-`, and `_`.
fn valid_id(id: &str) -> bool {
    id.len() >= 10 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// What `spec` names over the network; none for a file.
pub fn remote(spec: &str) -> Result<Option<Remote>, SourceError> {
    let spec = spec.trim();
    if let Some(query) = spec.strip_prefix("sec:") {
        let (query, forms) = match query.rsplit_once(':') {
            Some((q, forms)) if !forms.is_empty() && forms.split(',').all(form) => {
                (q, forms.split(',').map(|f| f.trim().to_ascii_uppercase()).collect())
            }
            _ => (query, sec::DEFAULT_FORMS.iter().map(|f| f.to_string()).collect()),
        };
        if query.trim().is_empty() {
            return Err(SourceError::Unreadable("sec: names no company: sec:TICKER".into()));
        }
        return Ok(Some(Remote::Sec { query: query.trim().to_string(), forms }));
    }
    let lower = spec.to_ascii_lowercase();
    let on_github = ["https://github.com/", "http://github.com/", "https://www.github.com/", "github.com/"]
        .iter()
        .any(|p| lower.starts_with(p));
    if lower.starts_with("github:") || on_github {
        let r = github::parse(spec).ok_or_else(|| {
            SourceError::Unreadable(format!("`{spec}` names no GitHub repository: github:OWNER/REPO"))
        })?;
        return Ok(Some(Remote::GitHub(r)));
    }
    if lower.starts_with("https://") || lower.starts_with("http://") {
        if let Some((kind, id)) = google(spec) {
            return Ok(Some(Remote::Google { kind, id }));
        }
        return Ok(Some(Remote::Web(spec.to_string())));
    }
    Ok(None)
}

/// A form SEC files: `10-K`, `8-K`, `DEF 14A`, `S-1`.
fn form(f: &str) -> bool {
    let f = f.trim();
    let shaped = f.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | ' ' | '/'));
    !f.is_empty() && f.len() <= 12 && shaped && f.chars().any(|c| c.is_ascii_digit() || c == '-')
}

/// A request the client makes: a GET of `url` with `headers`, its answer held to `most` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Get {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub most: u64,
}

/// What a plan asks for next: a request, or the source it read.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Get(Get),
    Done(Source),
}

/// What reaching a source takes from the user: the contact SEC asks every request to carry, and
/// a GitHub token of their own, for a private repository or more requests an hour; and whether
/// the client is a browser, which reaches only what a host lets another page read.
#[derive(Debug, Clone, Default)]
pub struct Reach {
    pub contact: Option<String>,
    pub token: Option<String>,
    pub browser: bool,
}

/// A source being fetched.
#[derive(Debug)]
pub struct Fetch {
    state: State,
    headers: Vec<(String, String)>,
    browser: bool,
    token: bool,
}

#[derive(Debug)]
enum State {
    Tickers {
        query: String,
        forms: Vec<String>,
    },
    Submissions {
        company: sec::Company,
        forms: Vec<String>,
    },
    Facts {
        company: sec::Company,
        picked: Vec<sec::Filing>,
    },
    Filings {
        company: sec::Company,
        picked: Vec<sec::Filing>,
        filed: Vec<(sec::Filing, String)>,
        figures: Option<crate::Sheet>,
    },
    Google(Google),
    Metadata(github::RepoRef),
    Archive(github::RepoRef, Option<github::Metadata>),
    /// A browser's: the repository's tree, then each file chosen, one by one.
    Tree(github::RepoRef, Option<github::Metadata>, String),
    Files {
        repo: github::RepoRef,
        meta: Option<github::Metadata>,
        reference: String,
        files: Vec<github::File>,
        /// The files still to read, by their place in `files`, the next last.
        wanted: Vec<usize>,
    },
    Page(String),
    Done,
}

const MOST: u64 = 64 << 20;
/// The most a repository's archive may hold, compressed.
const MOST_ARCHIVE: u64 = 256 << 20;

impl Fetch {
    /// The plan that reads `remote`, and the first request it makes. SEC asks each request to
    /// say who makes it: without the user's contact, nothing is asked of it.
    pub fn new(remote: Remote, reach: &Reach) -> Result<(Fetch, Get), SourceError> {
        if reach.browser {
            match &remote {
                Remote::Sec { .. } => {
                    return Err(SourceError::Unreadable(
                        "SEC asks each request to name who makes it, in a header a browser cannot set: ground a deck in filings with the Mac app or `scaena generate --source sec:TICKER`".into(),
                    ));
                }
                Remote::Google { kind, .. } => {
                    return Err(SourceError::Unreadable(format!(
                        "Google lets no other page read a {}: download it and add the file",
                        kind.named()
                    )));
                }
                _ => {}
            }
        }
        let (state, headers, url) = match remote {
            Remote::Sec { query, forms } => {
                let contact = reach.contact.as_deref().filter(|c| !c.trim().is_empty()).ok_or_else(|| {
                    SourceError::Unreadable(
                        "SEC asks each request to say who makes it: give your email as the SEC contact".into(),
                    )
                })?;
                (State::Tickers { query, forms }, sec::headers(contact.trim()), sec::TICKERS_URL.to_string())
            }
            Remote::GitHub(r) => {
                let url = github::metadata_url(&r);
                (State::Metadata(r), github::headers(reach.token.as_deref()), url)
            }
            Remote::Google { kind, id } => {
                (State::Google(kind), vec![("user-agent".to_string(), "Scaena".to_string())], kind.export(&id))
            }
            Remote::Web(url) => (State::Page(url.clone()), vec![("user-agent".to_string(), "Scaena".to_string())], url),
        };
        // A browser sets its own User-Agent: one set by a page is a header a site must give leave
        // for first (CORS), which most do not.
        let mut headers = headers;
        if reach.browser {
            headers.retain(|(k, _)| k != "user-agent");
        }
        let most = MOST;
        let token = reach.token.as_deref().is_some_and(|t| !t.is_empty());
        let fetch = Fetch { state, headers: headers.clone(), browser: reach.browser, token };
        Ok((fetch, Get { url, headers, most }))
    }

    fn get(&self, url: String, most: u64) -> Step {
        Step::Get(Get { url, headers: self.headers.clone(), most })
    }

    /// The answer to the last request: its status, its content type, and its body. The next
    /// request, or the source read.
    pub fn answer(&mut self, status: u16, kind: &str, body: &[u8]) -> Result<Step, SourceError> {
        let state = std::mem::replace(&mut self.state, State::Done);
        let failed = |what: &str| SourceError::Unreadable(format!("{what} answered {status}"));
        let json = |what: &str| -> Result<serde_json::Value, SourceError> {
            if status != 200 {
                return Err(failed(what));
            }
            serde_json::from_slice(body).map_err(|_| SourceError::Unreadable(format!("{what} answered with no JSON")))
        };
        match state {
            State::Tickers { query, forms } => {
                let companies = sec::companies(&json("SEC's list of companies")?);
                let company = sec::resolve(&companies, &query).ok_or_else(|| {
                    SourceError::Unreadable(format!("SEC lists no one company as `{query}`: give its ticker"))
                })?;
                let url = sec::submissions_url(&company.cik);
                self.state = State::Submissions { company, forms };
                Ok(self.get(url, MOST))
            }
            State::Submissions { company, forms } => {
                let what = format!("SEC's index of {}'s filings", company.ticker);
                let wanted: Vec<&str> = forms.iter().map(String::as_str).collect();
                let mut picked = sec::latest(&sec::filings(&json(&what)?), &wanted);
                if picked.is_empty() {
                    return Err(SourceError::Unreadable(format!(
                        "{} ({}) has filed no {} that SEC lists lately",
                        company.name,
                        company.ticker,
                        forms.join(" or ")
                    )));
                }
                picked.reverse();
                let url = sec::facts_url(&company.cik);
                self.state = State::Facts { company, picked };
                Ok(self.get(url, MOST))
            }
            State::Facts { company, picked } => {
                // A company that tags no facts (a 404) is read from its filings alone.
                let facts = (status == 200).then(|| serde_json::from_slice(body).ok()).flatten();
                let figures = facts.and_then(|f: serde_json::Value| sec::financials(&f, &company.ticker));
                self.next_filing(company, picked, Vec::new(), figures)
            }
            State::Filings { company, mut picked, mut filed, figures } => {
                let filing = picked.pop().expect("a filing asked for");
                if status != 200 {
                    return Err(failed(&filing.url));
                }
                let text = sec::filing_text(&crate::decode(body));
                filed.push((filing, text));
                self.next_filing(company, picked, filed, figures)
            }
            State::Metadata(r) => {
                let named = format!("{}/{}", r.owner, r.repo);
                let meta = match status {
                    200 => serde_json::from_slice(body).ok().map(|v| github::metadata(&v)),
                    404 => {
                        return Err(SourceError::Unreadable(format!(
                            "GitHub has no repository {named} that this reaches: a private one needs a token"
                        )));
                    }
                    403 | 429 => {
                        return Err(SourceError::Unreadable(format!(
                            "GitHub turned the request for {named} away ({status}): a token allows more"
                        )));
                    }
                    _ => None,
                };
                if self.browser {
                    let reference = (r.reference.clone())
                        .or_else(|| meta.as_ref().and_then(|m| m.default_branch.clone()))
                        .unwrap_or_else(|| "HEAD".into());
                    let url = github::tree_url(&r, &reference);
                    self.state = State::Tree(r, meta, reference);
                    return Ok(self.get(url, MOST));
                }
                let url = github::archive_url(&r);
                self.state = State::Archive(r, meta);
                Ok(self.get(url, MOST_ARCHIVE))
            }
            State::Tree(repo, meta, reference) => {
                let what = format!("GitHub's tree of {}/{}", repo.owner, repo.repo);
                let files = github::tree(&json(&what)?);
                let mut wanted = github::wanted(&files, &repo);
                wanted.reverse();
                self.next_file(repo, meta, reference, files, wanted)
            }
            State::Files { repo, meta, reference, mut files, mut wanted } => {
                let at = wanted.pop().expect("a file asked for");
                // A file that does not come is left out, as one past the budget is.
                if status == 200 {
                    files[at].data = Some(body.to_vec());
                }
                self.next_file(repo, meta, reference, files, wanted)
            }
            State::Archive(r, meta) => {
                if status != 200 {
                    return Err(failed(&format!("GitHub's archive of {}/{}", r.owner, r.repo)));
                }
                let files = github::unpack(body, &github::keep).map_err(|e| SourceError::Unreadable(e.to_string()))?;
                let (name, text) =
                    github::compose(&r, meta.as_ref(), &files).map_err(|e| SourceError::Unreadable(e.to_string()))?;
                Ok(Step::Done(Source { name, kind: Kind::GitHub, text, sheets: Vec::new(), pdf: None }))
            }
            State::Google(google) => {
                // A file not shared by link answers with Google's sign-in page.
                let page = kind.contains("html") || body.trim_ascii_start().starts_with(b"<");
                if status != 200 || page {
                    return Err(SourceError::Unreadable(format!(
                        "the {} is not shared by link: share it so anyone with the link can view it, or download it and give the file",
                        google.named()
                    )));
                }
                let ext = match google {
                    Google::Document => "docx",
                    Google::Spreadsheet => "xlsx",
                    Google::Presentation => "pptx",
                    Google::File => crate::sniff(body).ok_or_else(|| {
                        SourceError::Unreadable("the Drive file is neither a PDF nor an Office file".into())
                    })?,
                };
                let title = crate::ooxml::Package::open(body).ok().and_then(|mut p| p.title());
                let name = format!("{}.{ext}", title.as_deref().unwrap_or(google.named()));
                Ok(Step::Done(crate::read(&name, body)?))
            }
            State::Page(url) => {
                if status != 200 {
                    return Err(failed(&url));
                }
                Ok(Step::Done(page(&url, kind, body)?))
            }
            State::Done => Err(SourceError::Unreadable("the source is read already".into())),
        }
    }

    /// The next file of a repository asked for, or the source its files make.
    fn next_file(
        &mut self,
        repo: github::RepoRef,
        meta: Option<github::Metadata>,
        reference: String,
        files: Vec<github::File>,
        wanted: Vec<usize>,
    ) -> Result<Step, SourceError> {
        match wanted.last() {
            Some(&next) => {
                let path = files[next].path.clone();
                let step = match self.token {
                    // A private repository's files come through the API, as they are.
                    true => {
                        let mut headers = self.headers.clone();
                        headers.retain(|(k, _)| k != "accept");
                        headers.push(("accept".into(), "application/vnd.github.raw+json".into()));
                        Step::Get(Get {
                            url: github::contents_url(&repo, &reference, &path),
                            headers,
                            most: github::MAX_FILE,
                        })
                    }
                    false => Step::Get(Get {
                        url: github::raw_url(&repo, &reference, &path),
                        headers: Vec::new(),
                        most: github::MAX_FILE,
                    }),
                };
                self.state = State::Files { repo, meta, reference, files, wanted };
                Ok(step)
            }
            None => {
                let (name, text) = github::compose(&repo, meta.as_ref(), &files)
                    .map_err(|e| SourceError::Unreadable(e.to_string()))?;
                Ok(Step::Done(Source { name, kind: Kind::GitHub, text, sheets: Vec::new(), pdf: None }))
            }
        }
    }

    /// The next filing asked for, or the source the filings make.
    fn next_filing(
        &mut self,
        company: sec::Company,
        picked: Vec<sec::Filing>,
        filed: Vec<(sec::Filing, String)>,
        figures: Option<crate::Sheet>,
    ) -> Result<Step, SourceError> {
        match picked.last() {
            Some(next) => {
                if !sec::allowed(&next.url) {
                    return Err(SourceError::Unreadable(format!("{}: not an address on SEC.gov", next.url)));
                }
                let url = next.url.clone();
                self.state = State::Filings { company, picked, filed, figures };
                Ok(self.get(url, MOST))
            }
            None => {
                let (name, text) = sec::compose(&company, &filed, figures.as_ref());
                let sheets = figures.into_iter().collect();
                Ok(Step::Done(Source { name, kind: Kind::Sec, text, sheets, pdf: None }))
            }
        }
    }
}

/// What a web address answered: a page as its words, a PDF as itself, another file as its
/// extension reads.
fn page(url: &str, kind: &str, body: &[u8]) -> Result<Source, SourceError> {
    let kind = kind.to_ascii_lowercase();
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let last = path.rsplit('/').next().filter(|l| !l.is_empty() && !l.contains(':')).unwrap_or("page");
    if kind.contains("pdf") || body.starts_with(b"%PDF") {
        let name = if last.to_ascii_lowercase().ends_with(".pdf") { last.to_string() } else { format!("{last}.pdf") };
        return crate::read(&name, body);
    }
    if kind.contains("html") || kind.is_empty() && body.trim_ascii_start().starts_with(b"<") {
        let text = html::read(&crate::decode(body));
        return Ok(Source { name: url.to_string(), kind: Kind::Html, text, sheets: Vec::new(), pdf: None });
    }
    crate::read(last, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_spec_names_what_it_fetches() {
        let sec = remote("sec:AAPL:10-K,8-k").unwrap().unwrap();
        assert_eq!(sec, Remote::Sec { query: "AAPL".into(), forms: vec!["10-K".into(), "8-K".into()] });
        let named = remote("sec:Apple Inc").unwrap().unwrap();
        assert_eq!(named, Remote::Sec { query: "Apple Inc".into(), forms: vec!["10-K".into(), "10-Q".into()] });
        assert!(matches!(remote("github:welshofer/scaena").unwrap(), Some(Remote::GitHub(r)) if r.repo == "scaena"));
        assert!(
            matches!(remote("https://github.com/a/b/tree/main/docs").unwrap(), Some(Remote::GitHub(r)) if r.sub_path.as_deref() == Some("docs"))
        );
        assert_eq!(remote("https://example.com/a").unwrap(), Some(Remote::Web("https://example.com/a".into())));
        assert_eq!(remote("notes/q3.md").unwrap(), None);
        let id = "1AbC_dEf-GhIjKlMnOp";
        let doc = remote(&format!("https://docs.google.com/document/d/{id}/edit?usp=sharing")).unwrap();
        assert_eq!(doc, Some(Remote::Google { kind: Google::Document, id: id.into() }));
        let sheet = remote(&format!("https://docs.google.com/spreadsheets/u/0/d/{id}/edit#gid=0")).unwrap();
        assert_eq!(sheet, Some(Remote::Google { kind: Google::Spreadsheet, id: id.into() }));
        let file = remote(&format!("https://drive.google.com/open?id={id}")).unwrap();
        assert_eq!(file, Some(Remote::Google { kind: Google::File, id: id.into() }));
        assert_eq!(Google::Presentation.export(id), format!("https://docs.google.com/presentation/d/{id}/export/pptx"));
        assert!(remote("github:nobody").is_err());
        assert!(remote("sec:").is_err());
    }

    #[test]
    fn sec_is_asked_with_the_users_contact_for_the_latest_of_each_form() {
        let remote = Remote::Sec { query: "AAPL".into(), forms: vec!["10-K".into(), "10-Q".into()] };
        assert!(Fetch::new(remote.clone(), &Reach::default()).is_err(), "no contact, no request");
        let reach = Reach { contact: Some("jay@example.com".into()), ..Reach::default() };
        let (mut f, first) = Fetch::new(remote, &reach).unwrap();
        assert_eq!(first.url, sec::TICKERS_URL);
        assert!(first.headers.contains(&("user-agent".into(), "Scaena jay@example.com".into())));
        let tickers = json!({ "0": { "cik_str": 320193, "ticker": "AAPL", "title": "Apple Inc." } }).to_string();
        let Step::Get(g) = f.answer(200, "application/json", tickers.as_bytes()).unwrap() else { panic!() };
        assert_eq!(g.url, "https://data.sec.gov/submissions/CIK0000320193.json");
        let index = json!({ "cik": "320193", "filings": { "recent": {
            "accessionNumber": ["0000320193-24-000123", "0000320193-24-000081"],
            "form": ["10-K", "10-Q"], "filingDate": ["2024-11-01", "2024-08-02"],
            "reportDate": ["2024-09-28", "2024-06-29"], "primaryDocument": ["a-10k.htm", "a-10q.htm"]
        } } })
        .to_string();
        let Step::Get(g) = f.answer(200, "application/json", index.as_bytes()).unwrap() else { panic!() };
        assert_eq!(g.url, "https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json");
        let fact = |frame: &str, val: f64| json!({ "frame": frame, "val": val });
        let facts = json!({ "facts": { "us-gaap": { "Revenues": { "units": { "USD": [
            fact("CY2023", 383_285_000_000.0), fact("CY2024", 391_035_000_000.0)
        ] } } } } })
        .to_string();
        let Step::Get(g) = f.answer(200, "application/json", facts.as_bytes()).unwrap() else { panic!() };
        assert!(g.url.ends_with("/a-10k.htm"), "the forms in the order asked: {}", g.url);
        let Step::Get(g) = f.answer(200, "text/html", b"<p>Net sales rose 2%.</p>").unwrap() else { panic!() };
        assert!(g.url.ends_with("/a-10q.htm"));
        let Step::Done(s) = f.answer(200, "text/html", b"<p>Services grew.</p>").unwrap() else { panic!() };
        assert_eq!(s.kind, Kind::Sec);
        assert_eq!(s.name, "SEC: AAPL \u{2014} 10-K (2024-11-01), 10-Q (2024-08-02)");
        assert!(s.text.contains("Net sales rose 2%.") && s.text.contains("Services grew."));
        // The figures stand before the filings, and are kept as a sheet a chart can plot.
        assert!(s.text.find("2024\t391035").unwrap() < s.text.find("Net sales rose").unwrap());
        assert_eq!(s.sheets[0].rows[2], ["2024", "391035"]);
    }

    #[test]
    fn a_browser_asks_a_page_with_no_header_a_site_must_allow() {
        let browser = Reach { browser: true, ..Reach::default() };
        let (_, first) = Fetch::new(Remote::Web("https://example.com/about".into()), &browser).unwrap();
        assert!(first.headers.is_empty(), "{:?}", first.headers);
    }

    #[test]
    fn a_page_reads_as_its_words_and_a_pdf_as_itself() {
        let (mut f, first) = Fetch::new(Remote::Web("https://example.com/about".into()), &Reach::default()).unwrap();
        assert_eq!(first.url, "https://example.com/about");
        let Step::Done(s) = f.answer(200, "text/html; charset=utf-8", b"<h1>Tidewater</h1><p>Ships.</p>").unwrap()
        else {
            panic!()
        };
        assert_eq!((s.kind, s.text.as_str()), (Kind::Html, "# Tidewater\n\nShips."));
        let (mut f, _) = Fetch::new(Remote::Web("https://example.com/q3".into()), &Reach::default()).unwrap();
        let Step::Done(s) = f.answer(200, "application/pdf", b"%PDF-1.7").unwrap() else { panic!() };
        assert_eq!((s.name.as_str(), s.pdf.is_some()), ("q3.pdf", true));
        let (mut f, _) = Fetch::new(Remote::Web("https://example.com/gone".into()), &Reach::default()).unwrap();
        assert!(f.answer(404, "text/html", b"").is_err());
    }

    #[test]
    fn a_google_file_not_shared_by_link_says_so() {
        let remote = Remote::Google { kind: Google::Document, id: "1AbC_dEf-GhIjKlMnOp".into() };
        let (mut f, first) = Fetch::new(remote, &Reach::default()).unwrap();
        assert!(first.url.ends_with("/export?format=docx"));
        let e = f.answer(200, "text/html; charset=utf-8", b"<!doctype html><title>Sign in</title>").unwrap_err();
        assert!(e.to_string().contains("not shared by link"), "{e}");
        let remote = Remote::Google { kind: Google::File, id: "1AbC_dEf-GhIjKlMnOp".into() };
        let (mut f, _) = Fetch::new(remote, &Reach::default()).unwrap();
        let Step::Done(s) = f.answer(200, "application/octet-stream", b"%PDF-1.7").unwrap() else { panic!() };
        assert_eq!((s.name.as_str(), s.kind), ("Drive file.pdf", Kind::Pdf));
    }

    #[test]
    fn a_browser_reads_a_repository_from_its_tree_file_by_file() {
        let browser = Reach { browser: true, ..Reach::default() };
        let r = github::parse("https://github.com/a/b").unwrap();
        let (mut f, first) = Fetch::new(Remote::GitHub(r), &browser).unwrap();
        assert!(first.headers.iter().all(|(k, _)| k != "user-agent"), "a browser sets its own");
        let meta = json!({ "description": "Ships freight", "default_branch": "main", "language": "Rust" });
        let Step::Get(tree) = f.answer(200, "application/json", meta.to_string().as_bytes()).unwrap() else { panic!() };
        assert_eq!(tree.url, "https://api.github.com/repos/a/b/git/trees/main?recursive=1");
        let readme = "# Tidewater\n\nCarbon-negative freight across the Gulf Coast, port to port.".repeat(4);
        let guide = "How a shipment moves, from booking to berth, and what it emits on the way.".repeat(5);
        let listed = json!({ "tree": [
            { "path": "README.md", "type": "blob", "size": readme.len() },
            { "path": "docs", "type": "tree" },
            { "path": "docs/guide.md", "type": "blob", "size": guide.len() },
            { "path": "src/main.rs", "type": "blob", "size": 4000 },
            { "path": "node_modules/x/README.md", "type": "blob", "size": 100 }
        ] });
        let Step::Get(first_file) = f.answer(200, "application/json", listed.to_string().as_bytes()).unwrap() else {
            panic!()
        };
        assert_eq!(first_file.url, "https://raw.githubusercontent.com/a/b/main/README.md");
        assert!(first_file.headers.is_empty(), "a plain GET, which no host asks leave for");
        let Step::Get(second) = f.answer(200, "text/plain", readme.as_bytes()).unwrap() else { panic!() };
        assert_eq!(second.url, "https://raw.githubusercontent.com/a/b/main/docs/guide.md");
        let Step::Done(s) = f.answer(200, "text/plain", guide.as_bytes()).unwrap() else { panic!() };
        assert_eq!((s.name.as_str(), s.kind), ("GitHub: a/b (Rust)", Kind::GitHub));
        assert!(s.text.contains("Carbon-negative freight") && s.text.contains("from booking to berth"));
        assert!(s.text.contains("1 source files"), "the code counted, not read");
    }

    #[test]
    fn a_token_reads_a_private_repositorys_files_through_the_api() {
        let reach = Reach { browser: true, token: Some("t".into()), ..Reach::default() };
        let r = github::parse("github:a/b/tree/dev/docs").unwrap();
        let (mut f, _) = Fetch::new(Remote::GitHub(r), &reach).unwrap();
        let Step::Get(tree) = f.answer(200, "application/json", b"{}").unwrap() else { panic!() };
        assert_eq!(tree.url, "https://api.github.com/repos/a/b/git/trees/dev?recursive=1");
        let listed = json!({ "tree": [{ "path": "docs/read me.md", "type": "blob", "size": 900 }] });
        let Step::Get(file) = f.answer(200, "application/json", listed.to_string().as_bytes()).unwrap() else {
            panic!()
        };
        assert_eq!(file.url, "https://api.github.com/repos/a/b/contents/docs/read%20me.md?ref=dev");
        assert!(file.headers.contains(&("accept".into(), "application/vnd.github.raw+json".into())));
        assert!(file.headers.contains(&("authorization".into(), "Bearer t".into())));
    }

    #[test]
    fn a_browser_is_told_where_sec_and_google_are_read() {
        let browser = Reach { browser: true, contact: Some("someone@example.com".into()), ..Reach::default() };
        let sec = Remote::Sec { query: "AAPL".into(), forms: vec!["10-K".into()] };
        let e = Fetch::new(sec, &browser).unwrap_err();
        assert!(e.to_string().contains("--source sec:TICKER"), "{e}");
        let google = Remote::Google { kind: Google::Presentation, id: "1AbC_dEf-GhIjKlMnOp".into() };
        let e = Fetch::new(google, &browser).unwrap_err();
        assert!(e.to_string().contains("download it and add the file"), "{e}");
    }

    #[test]
    fn a_private_repository_says_a_token_reaches_it() {
        let r = github::parse("github:a/b").unwrap();
        let (mut f, first) = Fetch::new(Remote::GitHub(r), &Reach::default()).unwrap();
        assert_eq!(first.url, "https://api.github.com/repos/a/b");
        let e = f.answer(404, "application/json", b"{}").unwrap_err();
        assert!(e.to_string().contains("token"), "{e}");
    }
}

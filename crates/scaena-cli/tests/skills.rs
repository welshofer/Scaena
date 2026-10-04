//! What agents and people read names only what exists. The skills (SPEC §7.6, PLAN 1.18) and
//! the authoring guide (`docs/authoring.md`, PLAN 2.10) never send a reader to a command, flag,
//! tool, lint code, file, or SPEC section that is not there. Each skill is served over MCP as
//! `scaena://skills/<name>`, and SPEC lists them all. The guide's deck and recipes compile and
//! lint as it says, the errors it shows are the compiler's, and its tables of names are the
//! shipped themes'.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ROOT: &str = "../..";
const GUIDE: &str = "docs/authoring.md";

/// Each skill: its directory's name and its `SKILL.md`.
fn skills() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(format!("{ROOT}/skills"))
        .unwrap()
        .map(|entry| {
            let dir = entry.unwrap().path();
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(dir.join("SKILL.md")).unwrap())
        })
        .collect();
    out.sort();
    out
}

fn guide() -> String {
    std::fs::read_to_string(format!("{ROOT}/{GUIDE}")).unwrap()
}

/// The text of each inline code span: between single backticks, outside fenced blocks.
fn spans(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            out.extend(line.split('`').skip(1).step_by(2));
        }
    }
    out
}

/// Each fenced block whose info string is `lang` (`""` for none), in order.
fn fenced(text: &str, lang: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut open: Option<(bool, String)> = None;
    for line in text.lines() {
        if let Some(info) = line.trim_start().strip_prefix("```") {
            match open.take() {
                None => open = Some((info.trim() == lang, String::new())),
                Some((true, body)) => out.push(body),
                Some((false, _)) => {}
            }
        } else if let Some((_, body)) = &mut open {
            body.push_str(line);
            body.push('\n');
        }
    }
    out
}

/// What `scaena <args> --help` prints.
fn help(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).arg("--help").output().unwrap();
    assert!(out.status.success(), "scaena {args:?} --help");
    String::from_utf8(out.stdout).unwrap()
}

/// Every `§N` and `§N.M` a text cites.
fn sections(text: &str) -> BTreeSet<String> {
    text.split('§')
        .skip(1)
        .map(|rest| {
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            n.trim_end_matches('.').to_string()
        })
        .filter(|n| !n.is_empty())
        .collect()
}

fn is_code(w: &str) -> bool {
    w.len() == 4 && w.starts_with(['E', 'W', 'I']) && w[1..].chars().all(|d| d.is_ascii_digit())
}

/// Whether a command's help shows `flag`.
fn takes(help: &str, flag: &str) -> bool {
    help.contains(&format!("{flag} ")) || help.contains(&format!("{flag}\n"))
}

/// A path in the repository that is not there, if `word` names one.
fn no_such_file(word: &str) -> Option<String> {
    let pathish = ["docs/", "skills/", "crates/", "tests/", "scripts/", "web/"].iter().any(|p| word.starts_with(p))
        && !word.contains(['*', '<', ' ', '|']);
    (pathish && !Path::new(ROOT).join(word).exists()).then(|| format!("`{word}`: no such file"))
}

/// What a text may name: the commands and the flags each takes, the MCP tools, the lint
/// catalog's codes, and SPEC's sections.
struct Names {
    spec: String,
    helps: Vec<(String, String)>,
    tools: BTreeSet<String>,
    codes: BTreeSet<String>,
}

impl Names {
    fn new() -> Self {
        let spec = std::fs::read_to_string(format!("{ROOT}/docs/SPEC.md")).unwrap();
        let top = help(&[]);
        let helps = (top.lines())
            .skip_while(|l| !l.starts_with("Commands:"))
            .skip(1)
            .take_while(|l| l.starts_with("  "))
            .filter_map(|l| l.split_whitespace().next())
            .filter(|c| *c != "help")
            .map(|c| (c.to_string(), help(&[c])))
            .collect();
        // The tools, as their committed schemas name them (`scaena-mcp/tests/schemas.rs` holds
        // those to what the server lists).
        let tools = std::fs::read_dir(format!("{ROOT}/docs/schema/mcp"))
            .unwrap()
            .map(|e| e.unwrap().path().file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        // The lint codes, as the catalog's table lists them.
        let codes = (spec.lines())
            .filter_map(|l| l.strip_prefix("| ")?.split(' ').next())
            .filter(|c| is_code(c))
            .map(str::to_string)
            .collect();
        Names { spec, helps, tools, codes }
    }

    /// What is wrong with `scaena <command> …` as shown: a command that is not there, or a
    /// flag it does not take.
    fn command(&self, shown: &str) -> Vec<String> {
        let words: Vec<&str> = shown.split_whitespace().collect();
        let Some(name) = words.get(1) else { return Vec::new() };
        let Some((_, h)) = self.helps.iter().find(|(c, _)| c == name) else {
            return vec![format!("`{shown}`: there is no command `{name}`")];
        };
        (words.iter())
            .filter(|w| w.starts_with("--") && !takes(h, w))
            .map(|flag| format!("`{shown}`: `scaena {name}` has no `{flag}`"))
            .collect()
    }

    /// Everything `text` names that is not there.
    fn missing(&self, text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for span in spans(text) {
            let words: Vec<&str> = span.split_whitespace().collect();
            // A command, and the flags it is shown with.
            if words.first() == Some(&"scaena") {
                out.extend(self.command(span));
            } else if let Some(flag) = words.first().filter(|w| w.starts_with("--")) {
                // A flag on its own belongs to some command.
                if !self.helps.iter().any(|(_, h)| takes(h, flag)) {
                    out.push(format!("`{span}`: no command has `{flag}`"));
                }
            }
            // An MCP tool.
            let toolish = ["deck_", "theme_", "data_", "spine_"].iter().any(|p| span.starts_with(p))
                && span.chars().all(|c| c.is_ascii_lowercase() || c == '_');
            if toolish && !self.tools.contains(span) {
                out.push(format!("`{span}`: there is no such MCP tool"));
            }
            // A resource the server serves.
            if span.starts_with("scaena://") && scaena_mcp::resource(span).is_none() {
                out.push(format!("`{span}`: the MCP server has no such resource"));
            }
            // A file in the repository.
            out.extend(no_such_file(span));
        }
        // Lint codes, wherever they stand.
        for word in text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| is_code(w)) {
            if !self.codes.contains(word) {
                out.push(format!("{word} is not in the lint catalog (SPEC §7.5)"));
            }
        }
        // SPEC sections.
        for n in sections(text) {
            let heading = if n.contains('.') { format!("\n### {n} ") } else { format!("\n## {n}. ") };
            if !self.spec.contains(&heading) {
                out.push(format!("SPEC has no §{n}"));
            }
        }
        out
    }
}

#[test]
fn skills_name_only_what_exists() {
    let names = Names::new();
    let mut problems = Vec::new();
    for (name, text) in skills() {
        let mut fail = |what: String| problems.push(format!("{name}: {what}"));
        // Frontmatter: the skill's name, and when to use it.
        let front = text.strip_prefix("---\n").and_then(|t| t.split_once("\n---\n")).map(|(f, _)| f);
        let field = |key: &str| front?.lines().find_map(|l| l.strip_prefix(key)).map(str::trim);
        if field("name:") != Some(name.as_str()) {
            fail(format!("frontmatter `name` is {:?}", field("name:")));
        }
        if field("description:").is_none_or(|d| d.len() < 40) {
            fail("frontmatter has no `description` that says when to use it".into());
        }
        names.missing(&text).into_iter().for_each(fail);
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn every_skill_is_served_and_listed() {
    let spec = std::fs::read_to_string(format!("{ROOT}/docs/SPEC.md")).unwrap();
    let start = spec.find("### 7.6").unwrap();
    let section = &spec[start..start + spec[start..].find("\n## ").unwrap()];
    let listed: BTreeSet<&str> = spans(section).into_iter().filter(|s| !s.contains(['/', '.'])).collect();
    let skills = skills();
    let names: BTreeSet<&str> = skills.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(listed, names, "SPEC §7.6 lists the skills there are");
    for (name, text) in &skills {
        assert_eq!(
            scaena_mcp::resource(&format!("scaena://skills/{name}")),
            Some(text.as_str()),
            "the MCP server serves `{name}` as it stands"
        );
    }
}

#[test]
fn the_guide_names_only_what_exists() {
    let names = Names::new();
    let text = guide();
    let mut problems = names.missing(&text);
    // The command lines it shows to type: each `scaena …` in them, and each path in the
    // repository, less the comments.
    for block in fenced(&text, "sh") {
        for line in block.lines().map(|l| l.split(" #").next().unwrap()) {
            for part in line.split("&&").map(str::trim).filter(|p| p.starts_with("scaena ")) {
                problems.extend(names.command(part));
            }
            problems.extend(line.split_whitespace().filter_map(no_such_file));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// A bundle for the guide's decks, `docs/examples/`'s files at the paths they name.
fn guide_bundle(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("guide-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        "themes/dusk.theme.json",
        "fonts/Fraunces-VF.ttf",
        "fonts/Inter-VF.ttf",
        "fonts/JetBrainsMono-VF.ttf",
        "data/q3-revenue.csv",
        "assets/trails-ridge.png",
    ];
    for file in files {
        std::fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
        std::fs::copy(Path::new(ROOT).join("docs/examples").join(file), dir.join(file)).unwrap();
    }
    dir
}

/// A text's lines, each without its trailing space, and without blank lines at either end.
fn lines(text: &str) -> Vec<&str> {
    let all: Vec<&str> = text.lines().map(str::trim_end).collect();
    let first = all.iter().position(|l| !l.is_empty()).unwrap_or(all.len());
    let last = all.iter().rposition(|l| !l.is_empty()).map_or(first, |i| i + 1);
    all[first..last].to_vec()
}

/// `scaena <args>`, run in `dir`.
fn scaena(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).current_dir(dir).args(args).output().unwrap()
}

/// `deck.scn`, as `source`, compiled into `deck.json` in `dir`.
fn compile(dir: &Path, source: &str) -> Output {
    std::fs::write(dir.join("deck.scn"), source).unwrap();
    scaena(dir, &["compile", "deck.scn", "-o", "deck.json"])
}

/// The codes `scaena lint` finds in the bundle at `dir`.
fn lint(dir: &Path) -> Vec<String> {
    let out = scaena(dir, &["lint", ".", "--json"]);
    let findings: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    findings.iter().map(|f| f["code"].as_str().unwrap().to_string()).collect()
}

#[test]
fn the_guides_deck_and_recipes_compile_and_lint_as_it_says() {
    let text = guide();
    let decks = fenced(&text, "scn");
    let (deck, recipes) = decks.split_first().expect("the guide shows a deck");
    let dir = guide_bundle("deck");
    let out = compile(&dir, deck);
    assert!(out.status.success(), "the guide's deck compiles:\n{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(lint(&dir), Vec::<String>::new(), "the guide's deck lints with no findings");
    // Each recipe adds to the deck. A new state that no beat names is W401, which the guide
    // says how to fix; lint finds nothing else.
    assert!(recipes.len() >= 5, "the guide's recipes");
    for recipe in recipes {
        let out = compile(&dir, &format!("{deck}\n{recipe}"));
        assert!(out.status.success(), "{recipe}compiles:\n{}", String::from_utf8_lossy(&out.stderr));
        let codes = lint(&dir);
        assert!(codes.iter().all(|c| c == "W401"), "{recipe}lints with no more than W401: {codes:?}");
    }
}

#[test]
fn the_guides_errors_are_the_compilers() {
    let text = guide();
    let shown = fenced(&text, "");
    let deck = &fenced(&text, "scn")[0];
    let misspelled = deck.replacen("role:headline semantic:claim", "role:headline sise:12 semantic:claim", 1);
    assert_ne!(&misspelled, deck);
    let cases = [
        // Source that does not parse exits 2.
        ("deck \"Q3 Review\" canvas:1920x1080\nstate a\n  t text \"unterminated\n", 2, "is not closed"),
        // A property its type does not have is E106, and exits 1.
        (misspelled.as_str(), 1, "sise"),
    ];
    let dir = guide_bundle("errors");
    for (source, exit, about) in cases {
        let block = shown.iter().find(|b| b.contains(about)).expect("the guide shows the error");
        let out = compile(&dir, source);
        assert_eq!(out.status.code(), Some(exit), "{about}");
        assert!(!dir.join("deck.json").exists(), "a deck that does not compile is not written");
        let said = String::from_utf8(out.stderr).unwrap();
        assert_eq!(lines(&said), lines(block), "the guide shows what compile says");
    }
}

#[test]
fn the_guides_theme_names_are_the_shipped_themes() {
    let text = guide();
    let start = text.find("\n## The theme's names").unwrap();
    let section = &text[start..start + 1 + text[start + 1..].find("\n## ").unwrap()];
    // Each table row with names in its second cell. A layout's row names the layout in its
    // first; any other row says there what kind of name it lists.
    let mut layouts = BTreeMap::new();
    let mut named = BTreeMap::new();
    for line in section.lines() {
        let Some(row) = line.strip_prefix('|') else { continue };
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        let names: BTreeSet<String> = spans(cells[1]).into_iter().map(str::to_string).collect();
        if names.is_empty() {
            continue;
        }
        match cells[0].strip_prefix('`').and_then(|c| c.strip_suffix('`')) {
            Some(layout) => layouts.insert(layout.to_string(), names),
            None => named.insert(cells[0].split(" (").next().unwrap().to_string(), names),
        };
    }
    let themes = [
        "docs/examples/themes/dusk.theme.json",
        "docs/examples/themes/ember.theme.json",
        "docs/examples/authorability/themes/daybreak.theme.json",
    ];
    for path in themes {
        let theme: Value = serde_json::from_str(&std::fs::read_to_string(Path::new(ROOT).join(path)).unwrap()).unwrap();
        let keys = |v: &Value| -> BTreeSet<String> { v.as_object().unwrap().keys().cloned().collect() };
        let slots: BTreeMap<String, BTreeSet<String>> =
            theme["layouts"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), keys(&v["slots"]))).collect();
        assert_eq!(layouts, slots, "{path}: the layouts and their slots");
        let names: BTreeMap<String, BTreeSet<String>> = [
            ("Text roles", &theme["type"]["roles"]),
            ("Motion presets", &theme["motion"]["presets"]),
            ("Durations", &theme["motion"]["durations"]),
            ("Easings", &theme["motion"]["easings"]),
            ("Springs", &theme["motion"]["springs"]),
            ("Colors", &theme["tokens"]["color"]),
            ("Color roles", &theme["tokens"]["roles"]),
            ("Shader presets", &theme["shaders"]["presets"]),
        ]
        .into_iter()
        .map(|(row, v)| (row.to_string(), keys(v)))
        .collect();
        assert_eq!(named, names, "{path}: the names");
        let grid = (theme["grid"]["columns"].as_u64(), theme["grid"]["rows"].as_u64());
        assert_eq!(grid, (Some(12), Some(12)), "{path}: the guide's 12 × 12 grid");
    }
}

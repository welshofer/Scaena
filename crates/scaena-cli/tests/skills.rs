//! The skills (SPEC §7.6, PLAN 1.18) name only what exists, so an agent that follows one
//! is never sent to a command, flag, tool, lint code, file, or SPEC section that is not
//! there. Each is served over MCP as `scaena://skills/<name>`, and SPEC lists them all.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

const ROOT: &str = "../..";

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

#[test]
fn skills_name_only_what_exists() {
    let spec = std::fs::read_to_string(format!("{ROOT}/docs/SPEC.md")).unwrap();
    let top = help(&[]);
    let commands: BTreeSet<&str> = (top.lines())
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .filter_map(|l| l.split_whitespace().next())
        .filter(|c| *c != "help")
        .collect();
    let helps: Vec<(&str, String)> = commands.iter().map(|c| (*c, help(&[c]))).collect();
    // The tools, as their committed schemas name them (`scaena-mcp/tests/schemas.rs` holds
    // those to what the server lists).
    let tools: BTreeSet<String> = std::fs::read_dir(format!("{ROOT}/docs/schema/mcp"))
        .unwrap()
        .map(|e| e.unwrap().path().file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    // The lint codes, as the catalog's table lists them.
    let codes: BTreeSet<&str> = (spec.lines())
        .filter_map(|l| l.strip_prefix("| ")?.split(' ').next())
        .filter(|c| c.len() == 4 && c.starts_with(['E', 'W', 'I']) && c[1..].chars().all(|d| d.is_ascii_digit()))
        .collect();
    let is_code =
        |w: &str| w.len() == 4 && w.starts_with(['E', 'W', 'I']) && w[1..].chars().all(|d| d.is_ascii_digit());

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
        for span in spans(&text) {
            let words: Vec<&str> = span.split_whitespace().collect();
            // A command, and the flags it is shown with.
            if words.first() == Some(&"scaena") && words.len() > 1 {
                match helps.iter().find(|(c, _)| *c == words[1]) {
                    None => fail(format!("`{span}`: there is no command `{}`", words[1])),
                    Some((_, h)) => {
                        for flag in words.iter().filter(|w| w.starts_with("--")) {
                            if !h.contains(&format!("{flag} ")) && !h.contains(&format!("{flag}\n")) {
                                fail(format!("`{span}`: `scaena {}` has no `{flag}`", words[1]));
                            }
                        }
                    }
                }
            } else if let Some(flag) = words.first().filter(|w| w.starts_with("--")) {
                // A flag on its own belongs to some command.
                if !helps.iter().any(|(_, h)| h.contains(&format!("{flag} ")) || h.contains(&format!("{flag}\n"))) {
                    fail(format!("`{span}`: no command has `{flag}`"));
                }
            }
            // An MCP tool.
            let toolish = ["deck_", "theme_", "data_", "spine_"].iter().any(|p| span.starts_with(p))
                && span.chars().all(|c| c.is_ascii_lowercase() || c == '_');
            if toolish && !tools.contains(span) {
                fail(format!("`{span}`: there is no such MCP tool"));
            }
            // A resource the server serves.
            if span.starts_with("scaena://") && scaena_mcp::resource(span).is_none() {
                fail(format!("`{span}`: the MCP server has no such resource"));
            }
            // A file in the repository.
            let pathish = ["docs/", "skills/", "crates/", "tests/", "scripts/"].iter().any(|p| span.starts_with(p))
                && !span.contains(['*', '<', ' ', '|']);
            if pathish && !Path::new(ROOT).join(span).exists() {
                fail(format!("`{span}`: no such file"));
            }
        }
        // Lint codes, wherever they stand.
        for word in text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| is_code(w)) {
            if !codes.contains(word) {
                fail(format!("{word} is not in the lint catalog (SPEC §7.5)"));
            }
        }
        // SPEC sections.
        for n in sections(&text) {
            let heading = if n.contains('.') { format!("\n### {n} ") } else { format!("\n## {n}. ") };
            if !spec.contains(&heading) {
                fail(format!("SPEC has no §{n}"));
            }
        }
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

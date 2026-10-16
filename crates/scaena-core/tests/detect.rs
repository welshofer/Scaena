//! A paste's language and its tidying (PLAN 2.133, ADR-0029 decision 9): code in each language
//! named or likely as itself, prose as prose, a single line code only where it says so, the
//! hints that name a language, and a paste made tidy as code, which tidying again leaves alone.

use scaena_core::code::{Detected, Hints, Lang, detect, lang, language, tidy, vscode_mode};

/// Code as it is pasted, a sample in each language.
const CODE: &[(&str, &str)] = &[
    (
        "rust",
        "use std::collections::HashMap;\n\npub fn count(words: &[&str]) -> HashMap<&str, usize> {\n    let mut counts = HashMap::new();\n    for w in words {\n        *counts.entry(*w).or_insert(0) += 1;\n    }\n    counts\n}\n",
    ),
    (
        "python",
        "import os\nfrom pathlib import Path\n\ndef sizes(root):\n    total = 0\n    for path in Path(root).rglob(\"*\"):\n        if path.is_file():\n            total += path.stat().st_size\n    return total\n",
    ),
    (
        "javascript",
        "const express = require('express');\nconst app = express();\n\napp.get('/', (req, res) => {\n  res.send('Hello');\n});\n\napp.listen(3000, () => console.log('listening'));\n",
    ),
    (
        "typescript",
        "interface User {\n  id: number;\n  name: string;\n}\n\nexport function greet(user: User): string {\n  return `Hello, ${user.name}`;\n}\n",
    ),
    (
        "go",
        "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tnums := []int{1, 2, 3}\n\tfor _, n := range nums {\n\t\tfmt.Println(n)\n\t}\n}\n",
    ),
    (
        "java",
        "package com.example;\n\nimport java.util.List;\n\npublic class Greeter {\n    @Override\n    public String toString() {\n        return \"Greeter\";\n    }\n\n    public static void main(String[] args) {\n        System.out.println(\"Hello\");\n    }\n}\n",
    ),
    (
        "c",
        "#include <stdio.h>\n#include <stdlib.h>\n\nint main(void) {\n    int *xs = malloc(10 * sizeof(int));\n    printf(\"%d\\n\", xs[0]);\n    free(xs);\n    return 0;\n}\n",
    ),
    (
        "cpp",
        "#include <iostream>\n#include <vector>\n\nint main() {\n    std::vector<int> v{1, 2, 3};\n    for (auto x : v) {\n        std::cout << x << std::endl;\n    }\n}\n",
    ),
    (
        "csharp",
        "using System;\nusing System.Collections.Generic;\n\nnamespace Demo.App\n{\n    public class Person\n    {\n        public string Name { get; set; }\n\n        static void Main(string[] args)\n        {\n            Console.WriteLine(\"Hello\");\n        }\n    }\n}\n",
    ),
    (
        "swift",
        "import SwiftUI\n\nstruct ContentView: View {\n    @State private var count = 0\n\n    var body: some View {\n        Button(\"Tap\") { count += 1 }\n    }\n}\n",
    ),
    (
        "kotlin",
        "package com.example.app\n\ndata class User(val name: String, val age: Int)\n\nfun main() {\n    val users = listOf(User(\"Ann\", 30))\n    users.forEach { println(it.name) }\n}\n",
    ),
    (
        "ruby",
        "require 'json'\n\nclass Greeter\n  attr_reader :name\n\n  def initialize(name)\n    @name = name\n  end\n\n  def greet\n    puts \"Hello, #{name}\"\n  end\nend\n",
    ),
    (
        "shell",
        "set -euo pipefail\n\nfor f in *.log; do\n  if [ -s \"$f\" ]; then\n    gzip \"$f\"\n  fi\ndone\necho \"done\"\n",
    ),
    (
        "console",
        "$ cargo build --release\n   Compiling scaena v0.1.0\n    Finished release [optimized] target(s) in 12.3s\n$ ./target/release/scaena --version\nscaena 0.1.0\n",
    ),
    (
        "powershell",
        "param(\n    [string]$Path = \".\"\n)\n\nGet-ChildItem -Path $Path -Recurse |\n    Where-Object { $_.Length -gt 1MB } |\n    ForEach-Object { Write-Host $_.FullName }\n",
    ),
    (
        "sql",
        "SELECT c.name, COUNT(o.id) AS orders\nFROM customers c\nLEFT JOIN orders o ON o.customer_id = c.id\nWHERE c.active = true\nGROUP BY c.name\nORDER BY orders DESC;\n",
    ),
    (
        "json",
        "{\n  \"name\": \"scaena\",\n  \"version\": \"1.0.0\",\n  \"private\": true,\n  \"scripts\": { \"build\": \"vite build\" }\n}\n",
    ),
    (
        "yaml",
        "name: CI\non: [push]\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: cargo test\n",
    ),
    (
        "toml",
        "[package]\nname = \"scaena\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\n",
    ),
    (
        "dockerfile",
        "FROM node:20-alpine AS build\nWORKDIR /app\nCOPY package*.json ./\nRUN npm ci\nCOPY . .\nRUN npm run build\nEXPOSE 3000\nCMD [\"node\", \"dist/server.js\"]\n",
    ),
    (
        "diff",
        "diff --git a/src/main.rs b/src/main.rs\nindex 3b18e51..a9c2f04 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    println!(\"hello\");\n+    println!(\"hello, world\");\n }\n",
    ),
    (
        "html",
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>Hello</title>\n</head>\n<body>\n  <p>Hi</p>\n</body>\n</html>\n",
    ),
    (
        "xml",
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<project>\n  <modelVersion>4.0.0</modelVersion>\n  <groupId>com.example</groupId>\n</project>\n",
    ),
    (
        "css",
        ".card {\n  display: flex;\n  padding: 16px;\n  border-radius: 8px;\n}\n\n.card:hover {\n  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.2);\n}\n",
    ),
    (
        "makefile",
        "CC := gcc\nCFLAGS := -O2 -Wall\n\n.PHONY: all clean\n\nall: app\n\napp: main.o util.o\n\t$(CC) $(CFLAGS) -o $@ $^\n\nclean:\n\trm -f *.o app\n",
    ),
    ("scss", "$gap: 8px;\n\n.card {\n  padding: $gap;\n  &:hover {\n    color: red;\n  }\n  @include shadow(2);\n}\n"),
    (
        "hcl",
        "provider \"aws\" {\n  region = \"us-east-1\"\n}\n\nresource \"aws_s3_bucket\" \"site\" {\n  bucket = var.name\n}\n",
    ),
    (
        "graphql",
        "query Viewer {\n  viewer {\n    login\n    name\n  }\n}\n\nfragment Repo on Repository {\n  name\n}\n",
    ),
    // JSX scores as JavaScript, which colors it.
    (
        "javascript",
        "export function Card({ title }) {\n  return (\n    <div className=\"card\">\n      <h2>{title}</h2>\n    </div>\n  );\n}\n",
    ),
    ("ini", "[core]\n\teditor = vim\n\tautocrlf = input\n[user]\n\tname = Ann\n\temail = ann@example.com\n"),
    // Short snippets pasted as plain text, with no fence or file to name them: what a person
    // copies from a page or a chat.
    ("python", "def greet(name):\n    print(f\"Hello, {name}\")\n"),
    ("python", "import os\n\nfor name in os.listdir(\".\"):\n    print(name)\n"),
    ("python", "class Point:\n    def __init__(self, x, y):\n        self.x = x\n        self.y = y\n"),
    ("javascript", "function total(items) {\n  return items.reduce((sum, item) => sum + item.price, 0);\n}\n"),
    ("rust", "fn main() {\n    let name = \"Ann\";\n    println!(\"Hello, {name}\");\n}\n"),
    ("sql", "SELECT name, email\nFROM users\nWHERE active = 1\nORDER BY name;\n"),
];

/// Words, not code: each pastes as text.
const PROSE: &[&str] = &[
    "Revenue grew twelve percent this quarter, led by Europe and a strong holiday season.\nMargins held steady even as we hired across every team.\n",
    "- Buy milk\n- Call the bank about the card\n- Book the venue for the offsite\n",
    "1. Open the app.\n2. Choose a theme you like.\n3. Type your first slide.\n",
    "Here is the plan for Monday: we ship the beta, then we listen.\nAfter that: we fix what people tell us is broken.\n",
    "Hi Jay,\n\nThe deck is ready for review. I moved the chart to slide 4.\n\nThanks,\nAnna\n",
    "From: Anna <anna@example.com>\nTo: Jay <jay@example.com>\nSubject: The Q3 deck\n\nHere is the draft we talked about this morning.\n",
    "1,2,3\n4,5,6\n7,8,9\n",
    "Region\tQ1\tQ2\nNorth\t12\t15\nSouth\t9\t11\n",
    "# Results\n\nRevenue grew 12% this quarter, led by Europe.\nMargins held steady, and churn fell for the third month in a row.\n",
    "9:00 Welcome and coffee\n9:30 Keynote: The road ahead\n10:15 Break\n",
    "Goal: grow revenue in the north\nOwner: Dana\nDue: Friday\n",
    "Preheat the oven to 200 degrees.\nWhisk the eggs with the sugar until pale.\nFold in the flour, then bake for twenty minutes.\n",
    "Two roads diverged in a yellow wood,\nAnd sorry I could not travel both\nAnd be one traveler, long I stood\n",
    "Problem -> Solution -> Impact\nWhy now? Because the market moved.\n",
    "Q: What does it cost?\nA: Nothing to start, and $5 a month after that.\n",
    "We met on Tuesday to talk about the launch,\nfrom the timeline to the budget, and\nwhere the money should go if we raise more.\n",
    "## 2.1.0\n\n- Faster export to PDF\n- Fixed: charts lost their colors in dark themes\n- New: a theme picker\n",
    "Launch day! #scaena #slides\nThank you all for the support, it means everything.\n",
    "Anna Lee <anna@example.com>\nJay Park <jay@example.com>\nDana Cho <dana@example.com>\n",
    "Select the text you want to change.\nUpdate the theme from the Theme tab.\nOrder the slides by dragging them.\n",
    "If the chart looks squashed, make it larger.\nFor more room, choose a layout with one column.\n",
    // Close to Python's shape: a lowercase head with a colon, lines indented under it, a print.
    "if we win the deal:\n    we hire two people\n    we open the Berlin office\n",
    "Notes from class:\n    try the new layout\n    print the handouts, twenty copies\n",
    "for the offsite:\n    book the venue\n    order lunch for twelve\n",
];

fn named(name: &str) -> Detected {
    Detected::Named(language(name).unwrap())
}

fn likely(name: &str) -> Detected {
    Detected::Likely(language(name).unwrap())
}

#[test]
fn code_is_named_or_likely_as_its_language() {
    let mut wrong = Vec::new();
    for (name, text) in CODE {
        let found = detect(text, &Hints::default());
        if found.language().map(|l| l.name) != Some(name) {
            wrong.push(format!("{name}: {found:?}"));
        }
    }
    assert!(wrong.is_empty(), "misread:\n{}", wrong.join("\n"));
}

#[test]
fn prose_is_prose() {
    for text in PROSE {
        assert_eq!(detect(text, &Hints::default()), Detected::Prose, "{text}");
    }
    assert_eq!(detect("", &Hints::default()), Detected::Prose);
}

/// One line is code only where it says so: a hint, a fence, or a script's first line.
/// Nothing panics, whatever the text: a non-breaking space and a multi-byte letter at every
/// character of every sample, read and tidied in every language.
#[test]
fn any_text_is_read_whole() {
    for (name, text) in CODE {
        for (i, _) in text.char_indices() {
            let text = format!("{}\u{a0}é日{}", &text[..i], &text[i..]);
            let _ = detect(&text, &Hints::default());
            let _ = tidy(&text, lang(name), 4);
        }
    }
}

#[test]
fn a_single_line_is_code_only_where_it_says_so() {
    for line in ["SELECT id FROM users;", "fn main() {}", "<div class=\"x\">", "x = 1", "$ ls -la", "import os"] {
        assert_eq!(detect(line, &Hints::default()), Detected::Prose, "{line}");
        assert_eq!(detect(&format!("\n{line}\n\n"), &Hints::default()), Detected::Prose, "{line}");
    }
    let sql = Hints { fence: Some("sql"), ..Hints::default() };
    assert_eq!(detect("SELECT id FROM users;", &sql), named("sql"));
    assert_eq!(detect("#!/bin/sh", &Hints::default()), named("shell"));
}

#[test]
fn a_fence_names_the_language() {
    let hint = |fence| detect("x", &Hints { fence: Some(fence), ..Hints::default() });
    assert_eq!(hint("rust"), named("rust"));
    assert_eq!(hint("rust,ignore"), named("rust"));
    assert_eq!(hint("{.python}"), named("python"));
    assert_eq!(hint("python title=\"app.py\""), named("python"));
    assert_eq!(hint("language-go"), named("go"));
    assert_eq!(hint("text"), named("plain"));
    // A fence that names nothing Scaena knows: the language its words score, else plain text.
    assert_eq!(hint("mermaid"), named("plain"));
    let fence = Hints { fence: Some(""), ..Hints::default() };
    assert_eq!(detect(CODE[0].1, &fence), likely("rust"));
    // A fence in the text itself, closed or not, with blank lines before it.
    assert_eq!(detect("\n```ts\nlet x = 1\n```\n", &Hints::default()), named("typescript"));
    assert_eq!(detect("~~~~ yaml\na: 1", &Hints::default()), named("yaml"));
    assert_eq!(detect("```\nnothing to see\n```", &Hints::default()), named("plain"));
    // A fence after words is Markdown, and prose.
    assert_eq!(detect("Run this:\n\n```sh\nls\n```\n", &Hints::default()), Detected::Prose);
}

#[test]
fn a_file_name_names_the_language() {
    let file = |name| detect("x", &Hints { file: Some(name), ..Hints::default() });
    assert_eq!(file("main.rs"), named("rust"));
    assert_eq!(file("src/App.TSX"), named("tsx"));
    assert_eq!(file("C:\\code\\build.ps1"), named("powershell"));
    assert_eq!(file("Dockerfile"), named("dockerfile"));
    assert_eq!(file("deploy/Dockerfile.prod"), named("dockerfile"));
    assert_eq!(file("Makefile"), named("makefile"));
    assert_eq!(file(".editorconfig"), named("ini"));
    assert_eq!(file("Cargo.lock"), named("toml"));
    // Plain text, and a name Scaena knows no language by, name nothing: the words decide.
    assert_eq!(file("notes.txt"), Detected::Prose);
    assert_eq!(file("README"), Detected::Prose);
    assert_eq!(detect(CODE[1].1, &Hints { file: Some("notes.txt"), ..Hints::default() }), likely("python"));
}

#[test]
fn a_class_names_the_language() {
    let class = |class| detect("x", &Hints { class: Some(class), ..Hints::default() });
    assert_eq!(class("language-rust"), named("rust"));
    assert_eq!(class("lang-py"), named("python"));
    assert_eq!(class("hljs python"), named("python"));
    assert_eq!(class("highlight highlight-source-js"), named("javascript"));
    assert_eq!(class("highlight-text-html-basic"), named("html"));
    assert_eq!(class("sourceCode notranslate"), Detected::Prose);
}

/// VS Code's mode names the language, its own names too, and Chromium's custom data gives it.
#[test]
fn vs_codes_mode_names_the_language() {
    let mode = |mode| detect("x", &Hints { mode: Some(mode), ..Hints::default() });
    assert_eq!(mode("rust"), named("rust"));
    assert_eq!(mode("typescriptreact"), named("tsx"));
    assert_eq!(mode("javascriptreact"), named("jsx"));
    assert_eq!(mode("shellscript"), named("shell"));
    assert_eq!(mode("dockercompose"), named("yaml"));
    // Plain text names nothing: the words decide.
    assert_eq!(mode("plaintext"), Detected::Prose);
    assert_eq!(mode("markdown"), Detected::Prose);
    // The mode comes before a file's name.
    let both = Hints { mode: Some("go"), file: Some("x.rs"), ..Hints::default() };
    assert_eq!(detect("x", &both), named("go"));

    let data = r#"{"version":1,"isFromEmptySelection":false,"multicursorText":null,"mode":"typescriptreact"}"#;
    assert_eq!(vscode_mode(data), Some("typescriptreact"));
    assert_eq!(vscode_mode(r#" { "note" : "mode", "x": {"mode": "no"}, "mode" : "rust" } "#), Some("rust"));
    assert_eq!(vscode_mode(r#"{"a":["mode",{"mode":"no"}],"mode":"go"}"#), Some("go"));
    for bad in ["", "[]", "mode", r#"{"version":1}"#, r#"{"mode":1}"#, r#"{"mode":"a\"b"}"#, r#"{"mode":"ru"#] {
        assert_eq!(vscode_mode(bad), None, "{bad}");
    }
}

#[test]
fn a_scripts_first_line_names_the_language() {
    let script = |first: &str| detect(&format!("{first}\nwork\n"), &Hints::default());
    assert_eq!(script("#!/bin/bash"), named("shell"));
    assert_eq!(script("#!/usr/bin/env python3"), named("python"));
    assert_eq!(script("#!/usr/bin/python3.12"), named("python"));
    assert_eq!(script("#!/usr/bin/env -S deno run --allow-net"), named("typescript"));
    assert_eq!(script("#!/usr/bin/env NODE_ENV=production node"), named("javascript"));
    assert_eq!(script("#!/usr/bin/make -f"), named("makefile"));
    assert_eq!(script("#!/usr/bin/env pwsh"), named("powershell"));
    assert_eq!(script("#!/usr/bin/env perl"), Detected::Prose);
}

/// Languages that score alike go by the table's order: C before C++, CSS before SCSS, INI before
/// TOML, JavaScript before TypeScript.
#[test]
fn ties_go_by_the_tables_order() {
    let none = Hints::default();
    assert_eq!(detect("#define MAX 10\n#define MIN 0\n#define MID 5\n", &none), likely("c"));
    assert_eq!(detect("a {\n  color: red;\n}\np {\n  margin: 0;\n}\n", &none), likely("css"));
    assert_eq!(detect("[core]\n[remote]\n[branch]\n", &none), likely("ini"));
    assert_eq!(
        detect(
            "export default {\n  data: () => ({ count: 0 }),\n  render() {\n    return h('div');\n  },\n};\n",
            &none
        ),
        likely("javascript")
    );
}

/// Tidying a tidy text changes nothing, in every language.
#[test]
fn tidy_is_idempotent() {
    let samples = CODE.iter().map(|(n, t)| (*n, t.to_string())).chain(PROSE.iter().map(|t| ("plain", t.to_string())));
    let indented = CODE.iter().map(|(n, t)| (*n, t.lines().map(|l| format!("\t  {l}  \r\n")).collect::<String>()));
    for (name, text) in samples.chain(indented) {
        let once = tidy(&text, lang(name), 4);
        let twice = tidy(&once.text, lang(name), 4);
        assert_eq!(twice.text, once.text, "{name}:\n{text}");
        assert_eq!(twice.fence, None, "{name}");
    }
}

#[test]
fn tabs_stand_at_stops() {
    let plain = lang("plain");
    assert_eq!(tidy("ab\tc\nabcd\te\n", plain, 4).text, "ab  c\nabcd    e");
    assert_eq!(tidy("é\tx\nab\tc", plain, 4).text, "é   x\nab  c");
    assert_eq!(tidy("\tx\n\t\ty\n", plain, 4).text, "x\n    y");
    assert_eq!(tidy("\tx\n\t\ty\n", plain, 2).text, "x\n  y");
    assert_eq!(tidy("a\tb", plain, 0).text, "a b");
    // A Makefile keeps the tabs that open its lines, and turns the rest into spaces.
    assert_eq!(tidy("all:\tapp\n\tcc -o app\n", lang("makefile"), 4).text, "all:    app\n\tcc -o app");
}

#[test]
fn line_ends_and_spaces_are_made_one() {
    let plain = lang("plain");
    let tidied = tidy("\u{feff}\r\n\r\nlet\u{a0}x = 1;  \r\nlet y = 2;\t\rlet z = 3;\n\n\n", plain, 4);
    assert_eq!(tidied.text, "let x = 1;\nlet y = 2;\nlet z = 3;");
    assert_eq!(tidied.fence, None);
}

#[test]
fn a_fence_is_taken_off() {
    let plain = lang("plain");
    let tidied = tidy("\n```rust\nfn main() {}\n```\n", plain, 4);
    assert_eq!((tidied.text.as_str(), tidied.fence.as_deref()), ("fn main() {}", Some("rust")));
    let tidied = tidy("~~~~\ncode\n```\n~~~~~", plain, 4);
    assert_eq!((tidied.text.as_str(), tidied.fence.as_deref()), ("code\n```", Some("")));
    let tidied = tidy("```py\nx = 1", plain, 4);
    assert_eq!((tidied.text.as_str(), tidied.fence.as_deref()), ("x = 1", Some("py")));
    // An indented fence: its lines' indentation goes with it.
    let tidied = tidy("  ```js\n  if (a) {\n    b();\n  }\n  ```", plain, 4);
    assert_eq!(tidied.text, "if (a) {\n  b();\n}");
}

/// The four cases of the indentation the lines share.
#[test]
fn the_indentation_the_lines_share_is_taken_off() {
    let rust = lang("rust");
    // Every line indented: what they share is taken off.
    assert_eq!(tidy("    fn a() {\n        b();\n    }\n", rust, 4).text, "fn a() {\n    b();\n}");
    // The first line cut at its first character, the rest indented alike.
    assert_eq!(tidy("let x = 1;\n    let y = 2;\n    x + y\n", rust, 4).text, "let x = 1;\nlet y = 2;\nx + y");
    // The first line cut, the rest at several depths: a block's body deeper than its close.
    let cut = "fn main() {\n        body();\n        if x {\n            y();\n        }\n    }";
    assert_eq!(tidy(cut, rust, 4).text, "fn main() {\n    body();\n    if x {\n        y();\n    }\n}");
    // Nothing shared: nothing is taken off, and a whole block under its opener stays.
    assert_eq!(tidy("a\nb\n  c", rust, 4).text, "a\nb\n  c");
    assert_eq!(tidy("def f():\n    return 1\n", lang("python"), 4).text, "def f():\n    return 1");
    assert_eq!(tidy("foo(a,\n    b)", rust, 4).text, "foo(a,\n    b)");
    // A diff keeps its own: a context line's space is what it is.
    let diff = " fn main() {\n-    a();\n+    b();\n }";
    assert_eq!(tidy(diff, lang("diff"), 4).text, diff);
}

#[test]
fn json_is_reindented_only_where_it_is_minified() {
    let json = lang("json");
    let minified = r#"{"a":1,"b":[1.50,-2E+3,"x\u00e9\"",true,null],"c":{},"d":[]}"#;
    let expected = "{\n  \"a\": 1,\n  \"b\": [\n    1.50,\n    -2E+3,\n    \"x\\u00e9\\\"\",\n    true,\n    null\n  ],\n  \"c\": {},\n  \"d\": []\n}";
    assert_eq!(tidy(minified, json, 4).text, expected);
    assert_eq!(tidy(expected, json, 4).text, expected);
    // A long line that goes on after its commas is minified; a long string is not.
    let long = format!("{{\n  \"rows\": [{}]\n}}", (0..30).map(|n| n.to_string()).collect::<Vec<_>>().join(","));
    assert!(tidy(&long, json, 4).text.contains("\n    29\n  ]"));
    let string = format!("{{\n  \"note\": \"{}, and more\",\n  \"n\": 1\n}}", "word ".repeat(30));
    assert_eq!(tidy(&string, json, 4).text, string);
    // JSON Lines, a scalar, and JSON in another language stay as written.
    for (text, name) in
        [("{\"a\":1}\n{\"a\":2}", "json"), ("42", "json"), (minified, "javascript"), ("{\"a\":", "json")]
    {
        assert_eq!(tidy(text, lang(name), 4).text, text);
    }
}

#[test]
fn curly_quotes_are_reported_by_line() {
    let tidied = tidy("\nlet s = “hi”;\nlet t = 'x';\nlet u = ‘y’;\n", Lang::Plain, 4);
    assert_eq!(tidied.curly, [1, 3]);
    assert!(tidied.text.contains('“'), "curly quotes are reported, not changed");
}

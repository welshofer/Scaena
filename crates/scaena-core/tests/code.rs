//! Code colored by its language (PLAN 2.130, 2.134, and 2.135, ADR-0029): the scanners' tokens,
//! held to reviewed goldens, and what every scanner keeps to on any text at all.
//!
//! Each sample in `tests/code/<language>/*.txt` is colored as its golden `.spans` beside it says:
//! a token to a line, its class and then its text, in `«»`, with `\n`, `\r`, `\t`, and `\\`
//! written so. The directory names the language, and `generic` colors by the generic scanner. A
//! failing sample writes what it colors to `tests/code/actual/`; after a reviewed change,
//! `SCAENA_BLESS=1 cargo test -p scaena-core --test code` (or `just bless`) writes the goldens.

use scaena_core::code::{Class, LANGUAGES, Lang, MAX_LINES, MAX_NESTING, Token, highlight, lang, language};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[test]
fn each_sample_colors_as_its_golden_says() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/code");
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    let mut stale = Vec::new();
    let mut named = Vec::new();
    for dir in entries(&root).into_iter().filter(|d| d.is_dir() && !d.ends_with("actual")) {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let lang = lang(&name);
        assert!(lang != Lang::Plain, "tests/code/{name}: plain text colors nothing");
        assert_eq!(lang == Lang::Generic, name == "generic", "tests/code/{name} names no language Scaena knows");
        named.push(name.clone());
        let samples: Vec<PathBuf> =
            entries(&dir).into_iter().filter(|f| f.extension().is_some_and(|e| e == "txt")).collect();
        assert!(!samples.is_empty(), "tests/code/{name} holds no sample");
        for sample in samples {
            let text = std::fs::read_to_string(&sample).unwrap();
            let spans = spans(&text, &highlight(&text, lang));
            let golden = sample.with_extension("spans");
            if bless {
                std::fs::write(&golden, &spans).unwrap();
                continue;
            }
            if std::fs::read_to_string(&golden).unwrap_or_default() != spans {
                let actual = root.join("actual").join(&name).join(golden.file_name().unwrap());
                std::fs::create_dir_all(actual.parent().unwrap()).unwrap();
                std::fs::write(&actual, &spans).unwrap();
                stale.push(format!("{} (now in {})", golden.display(), actual.display()));
            }
        }
    }
    assert!(stale.is_empty(), "not what the scanners color: {stale:?}. Review, then bless with SCAENA_BLESS=1.");
    // Every language a scanner colors has a sample.
    for language in LANGUAGES.iter().filter(|l| lang(l.name) != Lang::Plain) {
        assert!(named.iter().any(|n| n == language.name), "tests/code/{} has no sample", language.name);
    }
}

/// Every language's scanner, and the generic one, on text made at random from pieces that open
/// and close what the scanners know, multi-byte text and U+00A0 among them; on 10 kB lines; on
/// brackets and interpolations 100 deep; and on the samples cut and spliced.
#[test]
fn spans_cover_the_text_in_order_on_char_boundaries() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let samples = samples();
    for lang in langs() {
        for _ in 0..300 {
            let text = random(&mut rng, 200, true);
            check(&text, lang);
        }
        for _ in 0..3 {
            let mut line = String::new();
            while line.len() < 10_000 {
                line.push_str(rng.pick(PIECES).trim_end_matches(['\n', '\r']));
            }
            check(&line, lang);
        }
        for (open, close) in DEEP {
            let deep = format!("{}x{}", open.repeat(100), close.repeat(100));
            check(&deep, lang);
            check(&open.repeat(100), lang);
            check(&close.repeat(100), lang);
        }
        for sample in &samples {
            for _ in 0..20 {
                let mut text = sample.clone();
                for _ in 0..4 {
                    let at = boundary(&text, rng.below(text.len() + 1));
                    let cut = boundary(&text, (at + rng.below(40)).min(text.len()));
                    text.replace_range(at..cut, &random(&mut rng, 3, false));
                }
                check(&text, lang);
            }
        }
    }
}

/// Long runs of what opens and does not close, each read in one pass, never again from each of its
/// bytes: a raw string's `#`s, a regular expression's `/`, type arguments, interpolations, JSX,
/// C#'s attributes and formats, and the spaces before a name a type declares; f-strings, heredocs
/// and their quoted delimiters, `%`-literals, substitutions, prompts, dollar quotes, names in
/// brackets and braces, and the spaces a soft keyword, a Ruby method's argument, or a heredoc's
/// delimiter is looked for past; and YAML's block scalars and flow collections, TOML's headers,
/// a Dockerfile's words and directives, a hunk's headers, templates, Make's directives and
/// expansions, tags, attributes, entities, and CSS's statements.
#[test]
fn long_runs_read_in_one_pass() {
    let runs = [
        "#".repeat(30_000),
        format!("r{}", "#".repeat(30_000)),
        "/[".repeat(15_000),
        "(/".repeat(15_000),
        "f<".repeat(15_000),
        "a<b".repeat(10_000),
        "`${".repeat(10_000),
        "<a>{".repeat(8_000),
        "\\(\"".repeat(10_000),
        "R\"".repeat(15_000),
        format!("x{}y", " ".repeat(30_000)),
        "'".repeat(30_000),
        "@".repeat(30_000),
        "<A<".repeat(10_000),
        "[A.".repeat(10_000),
        format!("$\"{{{}", "x:".repeat(15_000)),
        format!("string{}Name", " ".repeat(30_000)),
        "f\"{".repeat(10_000),
        "\"#{".repeat(10_000),
        "%w(".repeat(10_000),
        "%w[(".repeat(10_000),
        "<<A ".repeat(10_000),
        "<<~A\n".repeat(10_000),
        "<<'A' ".repeat(6_000),
        "\"$(".repeat(10_000),
        "${".repeat(15_000),
        "${a}".repeat(8_000),
        "$((".repeat(10_000),
        "[[ ".repeat(10_000),
        "$ ".repeat(15_000),
        "PS> ".repeat(10_000),
        "@\"".repeat(15_000),
        "$a$".repeat(10_000),
        "[a".repeat(15_000),
        "?".repeat(30_000),
        ":".repeat(30_000),
        format!("match{}x:", " ".repeat(30_000)),
        format!("type{}T", " ".repeat(30_000)),
        format!("puts{}\"x\"", " ".repeat(30_000)),
        format!("cat <<{}", " ".repeat(30_000)),
        format!("cat <<{d}\n{}", format!("{}\n", "X".repeat(999)).repeat(20), d = "X".repeat(1_000)),
        "- ".repeat(15_000),
        "key: |\n".repeat(5_000),
        "[{".repeat(15_000),
        "[a.".repeat(10_000),
        "\"\"\"".repeat(10_000),
        "ENV a=".repeat(5_000),
        "COPY a ".repeat(5_000),
        "RUN [\"a\", ".repeat(3_000),
        "@@ -1".repeat(6_000),
        "\"%{".repeat(10_000),
        "<<EOT ".repeat(5_000),
        "override ".repeat(3_000),
        "$(a ".repeat(8_000),
        "<a b=".repeat(6_000),
        "&#x".repeat(10_000),
        "<!x \"".repeat(6_000),
        "a{".repeat(15_000),
        "#{a".repeat(10_000),
        "a\\{".repeat(10_000),
    ];
    for lang in langs() {
        for run in &runs {
            check(run, lang);
        }
    }
}

/// What looks for its close no further than its line's end reads that line once, however many
/// open on it: a heredoc's quoted delimiter in Shell, a console's command, and Ruby, and
/// PowerShell's `${…}`; a Dockerfile's words, its exec form's strings, and its escapes; and Make's
/// directives. A quarter of a megabyte of each on one line reads in milliseconds, where reading
/// the rest of the line again from each opener took up to half a minute.
#[test]
fn a_line_of_openers_is_read_once() {
    // Each run is a head, and openers after it.
    let runs = [
        ("shell", "", "cat <<'A' "),
        ("shell", "", "cat <<\"A\" "),
        ("console", "", "$ cat <<'A' "),
        ("ruby", "", "f(<<~'A', "),
        ("powershell", "", "${a}"),
        ("powershell", "", "\"${a}\" "),
        ("dockerfile", "ENV ", "a="),
        ("dockerfile", "COPY ", "a "),
        ("dockerfile", "CMD [", "\"a\", "),
        ("dockerfile", "ENV ", "\\a "),
        ("makefile", "", "override "),
        ("makefile", "", "else "),
    ];
    for (name, head, piece) in runs {
        let text = format!("{head}{}", piece.repeat((256 << 10) / piece.len()));
        let started = std::time::Instant::now();
        let tokens = highlight(&text, lang(name));
        let took = started.elapsed();
        assert_eq!(tokens.last().map(|t| t.end), Some(text.len()), "{name}: {piece:?}");
        assert!(took < std::time::Duration::from_secs(5), "{name}: 256 KB of {piece:?} in {took:?}");
    }
}

/// 2.135's and 2.145's languages each read a megabyte of each of their constructs' openers in one
/// pass, on one line and on lines of a kilobyte: none reads on again from each opener, as a
/// Dockerfile's words and Make's directives once did, a megabyte of either taking minutes.
#[test]
fn a_megabyte_of_openers_reads_in_one_pass() {
    const OPENERS: &[(&str, &[&str])] = &[
        ("yaml", &["- ", "? ", "key: ", "\"", "'", "&a ", "*a ", "!!str ", "|", ">-", "[", "{", "#", "---", "a: |\n "]),
        ("toml", &["[", "[[", "[a.", "\"", "'", "\"\"\"", "'''", "{", "a=", "a = [", "x = {", "1979-05-27 ", "\"\\"]),
        ("dockerfile", &["RUN ", "RUN [\"a\", ", "CMD [\"a\",", "COPY <<EOF ", "ENV a=", "ENV ", "ENV \\a "]),
        ("dockerfile", &["ENV a=\"", "FROM a AS ", "LABEL \"a\"=", "ARG ", "EXPOSE 1 ", "COPY a ", "COPY '"]),
        ("dockerfile", &["COPY ${", "ONBUILD ", "RUN echo \"$(", "\\", "--a=", "a "]),
        ("diff", &["+", "-", " ", "@@ ", "@@ -1", "@@@", "--- ", "+++ ", "diff ", "\\ "]),
        ("hcl", &["${", "%{", "\"${", "\"%{", "<<EOT ", "<<a", "a = ", "a {", "a.", "/*", "\"", "a b ", "$${"]),
        ("graphql", &["{", "(", "[", "\"", "\"\"\"", "$a", "@a(", "...", "query ", "a: ", "on ", "type A {", "= "]),
        ("makefile", &["$(", "${", "$(shell ", "a:", "a=", "\techo $(", "ifeq (", "export ", "else "]),
        ("makefile", &["override ", "a: ; ", "$$", "\techo \"$(", "\techo '", "define a\n", "a: b | "]),
        ("ini", &["[", "a=", "${", "%(", "a=${", "a=%(", "a=$", "\"", ";", "\\"]),
        ("html", &["<", "<a ", "<a b=", "<a b=\"", "<!--", "<![CDATA[", "<?", "<!", "<!x \"", "&", "&#x"]),
        ("html", &["&a", "</a", "<title>&a", "<script>"]),
        ("xml", &["<", "<a b=\"&", "<!DOCTYPE a [", "<!ENTITY a \"", "]]>", "&#"]),
        ("css", &["{", "a{", "a:", ".a", "#a", "#fff ", "@media (a:", "url(", "url(a", "\"", "(", "[", "/*"]),
        ("css", &["a[", "a\\{", "-1px ", "a{b:c"]),
        ("scss", &["#{", "\"#{", "#{a", "$a:", "&-", "@include a(", "a.b(", "a:#{", "a{#{", "//", "%a"]),
        ("html", &["<script>(", "<style>a{", "<script type=\"a\">", "<script lang=\""]),
        ("markdown", &["# ", "*a", "_a", "**a", "`a", "``", "[a", "[a](", "[a][", "![", "<a", "<!--", "<http:"]),
        ("markdown", &["\\*", "| ", "> ", "- [ ] ", "1. ", "```\n", "```a\n", "~~~\n", "---\n", "a\n---\n"]),
        ("php", &["<?php ", "<?= ", "?>", "<?php '", "<?php \"", "<?php /*", "<?php //", "<<<A\n", "<<<'A'\n"]),
        ("php", &["$a", "\"{$", "\"${", "#[", "#", "<?php #", "<p><?php <<<A\n", "<a b=\"<?= "]),
        ("vue", &["{{", "{{a}}", "<a :b=\"", "<a @b=\"", "<a v-if=\"", "<script>", "<style lang=\"scss\">"]),
        ("svelte", &["{", "{#if ", "{:else if ", "{/if}", "{@html ", "<a b={", "<a b=\"{", "<a {...", "{\"", "{`"]),
    ];
    std::thread::scope(|scope| {
        for &(name, pieces) in OPENERS {
            scope.spawn(move || {
                for piece in pieces {
                    let row = piece.repeat(1024 / piece.len()).replace(['\n', '\r'], " ");
                    let one = piece.repeat((1 << 20) / piece.len());
                    let many = format!("{row}\n").repeat((1 << 20) / (row.len() + 1));
                    for text in [one, many] {
                        let started = std::time::Instant::now();
                        let tokens = highlight(&text, lang(name));
                        let took = started.elapsed();
                        assert_eq!(tokens.last().map(|t| t.end), Some(text.len()), "{name}: {piece:?}");
                        assert!(
                            took < std::time::Duration::from_secs(5),
                            "{name}: a megabyte of {piece:?} in {took:?}"
                        );
                    }
                }
            });
        }
    });
}

#[test]
fn what_never_closes_runs_to_the_end_in_its_class() {
    let cases: &[(&str, &str, &str, Class)] = &[
        ("c", "int x; /* never closed\nint y;\n", "/*", Class::Comment),
        ("c", "char *s = \"never closed", "\"", Class::String),
        ("cpp", "auto s = R\"x(raw )\" )y\" still\nint y;", "R\"", Class::String),
        ("csharp", "var s = @\"verbatim \"\" still\nint y;", "@\"", Class::String),
        ("csharp", "var s = \"\"\"\nraw \"\" still\n", "\"\"\"", Class::String),
        ("go", "s := `raw\nnever closed", "`", Class::String),
        ("java", "String s = \"\"\"\n  a text block\n  never closed;\n", "\"\"\"", Class::String),
        ("javascript", "const s = `a template\nnever closed;", "`", Class::String),
        ("kotlin", "val s = \"\"\"\nraw\n", "\"\"\"", Class::String),
        ("swift", "let s = #\"\"\"\nraw \"\"\" still\n", "#\"", Class::String),
        ("rust", "let s = \"never closed\nfn main() {}\n", "\"", Class::String),
        ("rust", "let s = r##\"raw \"# still\nfn main() {}", "r##", Class::String),
        ("rust", "/* outer /* inner */ still\nfn main() {}", "/*", Class::Comment),
        ("dart", "var s = '''\nmulti\n", "'''", Class::String),
        ("scala", "val s = \"\"\"\nmulti\n", "\"\"\"", Class::String),
        ("zig", "const s =\n    \\\\ a line of a string", "\\\\", Class::String),
        ("objc", "NSString *s = @\"never closed", "@\"", Class::String),
        ("protobuf", "string s = \"never closed", "\"", Class::String),
        ("typescript", "let x = 1; /* never\nclosed", "/*", Class::Comment),
        ("json", "{\"key\": \"never closed", "\"never", Class::String),
        ("jsonc", "{ /* never closed\n\"a\": 1 }", "/*", Class::Comment),
        ("jsx", "const a = <div>never closed\nlet x = 1;", "never", Class::Plain),
        ("elixir", "x = 1 /* never closed\ny = 2", "/*", Class::Comment),
        ("elixir", "s = \"never closed", "\"", Class::String),
        ("python", "s = \"\"\"never closed\nx = 1\n", "\"\"\"", Class::String),
        ("python", "s = rb'''never closed\nx = 1", "rb'''", Class::String),
        ("ruby", "s = \"never closed\nx = 1\n", "\"", Class::String),
        ("ruby", "s = <<~EOS\n  never terminated\nx = 1\n", "<<~EOS", Class::String),
        ("ruby", "x = %w[never closed\ny = 1", "%w[", Class::String),
        ("ruby", "=begin\nnever ended\nx = 1", "=begin", Class::Comment),
        ("shell", "echo 'never closed\nls -la", "'", Class::String),
        ("shell", "cat <<EOF\nnever terminated\nls -la\n", "EOF", Class::String),
        ("console", "$ echo 'never closed\nprinted\n$ ls\n", "'", Class::String),
        ("powershell", "$s = 'never closed\nWrite-Host 1", "'", Class::String),
        ("powershell", "$s = @'\nnever closed\nWrite-Host 1", "@'", Class::String),
        ("powershell", "<# never closed\nWrite-Host 1", "<#", Class::Comment),
        ("sql", "SELECT 'never closed\nFROM t;", "'", Class::String),
        ("sql", "SELECT $$ never closed\nSELECT 1;", "$$", Class::String),
        ("sql", "SELECT 1; /* never closed\nSELECT 2;", "/*", Class::Comment),
        ("yaml", "key: \"never closed\nnext: 1\n", "\"", Class::String),
        ("yaml", "key: 'never closed\nnext: 1", "'", Class::String),
        ("yaml", "run: |\n  never dedented\n  next: 1\n", "  never", Class::String),
        ("toml", "s = \"\"\"\nnever closed\nx = 1\n", "\"\"\"", Class::String),
        ("toml", "s = '''never closed\nx = 1", "'''", Class::String),
        ("dockerfile", "COPY <<EOF /etc/x\nnever terminated\nRUN ls\n", "never", Class::String),
        ("hcl", "x = <<EOT\nnever terminated\ny = 1\n", "never", Class::String),
        ("hcl", "x = 1 /* never closed\ny = 2", "/*", Class::Comment),
        ("graphql", "\"\"\"never closed\ntype A { a: B }", "\"\"\"", Class::String),
        ("html", "<p>a <!-- never closed\n<b>x</b>", "<!--", Class::Comment),
        ("html", "<script>let s = `never closed\n<p>", "`", Class::String),
        ("html", "<style>a { } /* never closed\n<p>", "/*", Class::Comment),
        ("markdown", "# Title\n```rust\nlet s = \"never closed\n", "\"", Class::String),
        ("markdown", "Text.\n<!-- never closed\n# Heading", "<!--", Class::Comment),
        ("php", "<p><?php echo 'never closed\n?>", "'", Class::String),
        ("php", "<p><?php /* never closed\n?>", "/*", Class::Comment),
        ("php", "$s = <<<EOT\nnever terminated\n", "<<<", Class::String),
        ("vue", "<p>{{ count + `never closed }}</p>\n", "`", Class::String),
        ("svelte", "<p>{ \"never closed }</p>", "\"", Class::String),
        ("html", "<a href=\"never closed\n<b>x</b>", "\"", Class::String),
        ("xml", "<a><![CDATA[never closed <b/>\n", "never", Class::String),
        ("xml", "<?xml never closed\n<a/>", "<?", Class::Meta),
        ("css", "a { color: red; } /* never closed\nb { }", "/*", Class::Comment),
        ("scss", "a { b: c; } /* never closed\nd { }", "/*", Class::Comment),
    ];
    for &(name, text, opener, class) in cases {
        let tokens = highlight(text, lang(name));
        let at = text.find(opener).unwrap();
        let (found, end) = token_at(&tokens, at);
        assert_eq!((found, end), (class, text.len()), "{name}: {text:?} from {opener:?}");
    }
    // A string that cannot run past its line ends at the line's end.
    let text = "char *s = \"cut short\nint y;";
    assert_eq!(token_at(&highlight(text, lang("c")), text.find("int").unwrap()).0, Class::Type);
    let text = "s = 'cut short\nx = int(s)";
    assert_eq!(token_at(&highlight(text, lang("python")), text.find("int").unwrap()).0, Class::Type);
    let cut = [
        ("toml", "s = \"cut short\nx = 1", "x", Class::Property),
        ("toml", "s = \"cut short \\\nx = 1", "x", Class::Property),
        ("hcl", "x = \"cut short\ny = 1", "y", Class::Property),
        ("graphql", "\"cut short\nquery Q { a }", "query", Class::Keyword),
        ("dockerfile", "ENV A=\"cut short\nRUN ls", "RUN", Class::Keyword),
    ];
    for (name, text, after, class) in cut {
        assert_eq!(token_at(&highlight(text, lang(name)), text.find(after).unwrap()).0, class, "{name}: {text:?}");
    }
}

#[test]
fn nesting_stops_at_eight() {
    assert_eq!(MAX_NESTING, 8);
    // Templates in templates: the first eight `${` interpolate, and the ninth is its string's own.
    let text = format!("x = `{}b{}`", "a${`".repeat(12), "`}".repeat(12));
    let tokens = highlight(&text, lang("javascript"));
    let opens: Vec<usize> = text.match_indices("${").map(|(i, _)| i).take(MAX_NESTING + 1).collect();
    for (k, &at) in opens.iter().enumerate() {
        let want = if k < MAX_NESTING { Class::Punctuation } else { Class::String };
        assert_eq!(token_at(&tokens, at).0, want, "the {}th `${{`", k + 1);
    }
    // Swift's `\(` alike.
    let text = format!("let s = {}x", "\"a\\(".repeat(12));
    let tokens = highlight(&text, lang("swift"));
    let opens: Vec<usize> = text.match_indices("\\(").map(|(i, _)| i).take(MAX_NESTING + 1).collect();
    for (k, &at) in opens.iter().enumerate() {
        let want = if k < MAX_NESTING { Class::Punctuation } else { Class::String };
        assert_eq!(token_at(&tokens, at).0, want, "the {}th `\\(`", k + 1);
    }
    // Python's f-strings in f-strings, Ruby's `#{` in strings, and Shell's and PowerShell's `$(` in
    // strings alike: the ninth is its string's own.
    let cases = [
        ("python", "f\"{", "}\"", "{"),
        ("ruby", "\"#{", "}\"", "#{"),
        ("shell", "\"$(", ")\"", "$("),
        ("powershell", "\"$(", ")\"", "$("),
    ];
    for (name, open, close, opener) in cases {
        let text = format!("x = {}y{}", open.repeat(12), close.repeat(12));
        let tokens = highlight(&text, lang(name));
        let opens: Vec<usize> = text.match_indices(opener).map(|(i, _)| i).take(MAX_NESTING + 1).collect();
        for (k, &at) in opens.iter().enumerate() {
            let want = if k < MAX_NESTING { Class::Punctuation } else { Class::String };
            assert_eq!(token_at(&tokens, at).0, want, "{name}: the {}th {opener:?}", k + 1);
        }
    }
    // HCL's templates in strings and SCSS's interpolations in strings alike.
    let cases = [("hcl", "x = ", "\"${", "}\"", "${"), ("scss", "a { b: ", "\"#{", "}\"", "#{")];
    for (name, before, open, close, opener) in cases {
        let text = format!("{before}{}y{} }}", open.repeat(12), close.repeat(12));
        let tokens = highlight(&text, lang(name));
        let opens: Vec<usize> = text.match_indices(opener).map(|(i, _)| i).take(MAX_NESTING + 1).collect();
        for (k, &at) in opens.iter().enumerate() {
            let want = if k < MAX_NESTING { Class::Punctuation } else { Class::String };
            assert_eq!(token_at(&tokens, at).0, want, "{name}: the {}th {opener:?}", k + 1);
        }
    }
    // Make's calls in calls: the ninth `$(` is its call's own text.
    let text = format!("x = {}y{}\n", "$(call ".repeat(12), ")".repeat(12));
    let tokens = highlight(&text, lang("makefile"));
    let opens: Vec<usize> = text.match_indices("$(").map(|(i, _)| i).take(MAX_NESTING + 1).collect();
    for (k, &at) in opens.iter().enumerate() {
        let want = if k < MAX_NESTING { Class::Punctuation } else { Class::Plain };
        assert_eq!(token_at(&tokens, at).0, want, "makefile: the {}th `$(`", k + 1);
    }
    // Heredocs that wait for their bodies: the ninth `<<` on a line opens none.
    for (name, text) in [
        ("shell", format!("cat {}\n", "<<A ".repeat(12))),
        ("ruby", format!("f({})\n", "<<~A, ".repeat(12))),
        ("dockerfile", format!("COPY {}/x\n", "<<A ".repeat(12))),
    ] {
        let tokens = highlight(&text, lang(name));
        let ids: Vec<usize> = text.match_indices('A').map(|(i, _)| i).take(MAX_NESTING + 1).collect();
        for (k, &at) in ids.iter().enumerate() {
            assert_eq!(token_at(&tokens, at).0 == Class::String, k < MAX_NESTING, "{name}: the {}th heredoc", k + 1);
        }
    }
    // PHP's `{$` in strings alike.
    let text = format!("x = {}y{}", "\"{$".repeat(12), "}\"".repeat(12));
    let tokens = highlight(&text, lang("php"));
    let opens: Vec<usize> = text.match_indices("{$").map(|(i, _)| i).take(MAX_NESTING + 1).collect();
    for (k, &at) in opens.iter().enumerate() {
        let want = if k < MAX_NESTING { Class::Punctuation } else { Class::String };
        assert_eq!(token_at(&tokens, at).0, want, "php: the {}th `{{$`", k + 1);
    }
    // Parts in another language inside one another (PLAN 2.145): Markdown fenced in Markdown, `n`
    // deep, around a fence of Rust. Its `fn` is a keyword while the Rust is at most eight parts
    // deep, and its container's text after.
    for n in 0..12 {
        let mut text = "```rust\nfn main() {}\n```".to_string();
        for k in 0..n {
            let fence = "`".repeat(4 + k);
            text = format!("{fence}markdown\n{text}\n{fence}");
        }
        let tokens = highlight(&text, lang("markdown"));
        let at = text.find("fn").unwrap();
        let want = if n < MAX_NESTING { Class::Keyword } else { Class::Plain };
        assert_eq!(token_at(&tokens, at).0, want, "rust fenced in markdown {n} deep");
    }
    // JSX elements in elements: the ninth `<a>` is its parent's text.
    let text = format!("x = {}", "<a>".repeat(12));
    let tokens = highlight(&text, lang("jsx"));
    let names: Vec<usize> = text.match_indices("<a").map(|(i, _)| i + 1).take(MAX_NESTING + 1).collect();
    for (k, &at) in names.iter().enumerate() {
        let want = if k < MAX_NESTING { Class::Tag } else { Class::Plain };
        assert_eq!(token_at(&tokens, at).0, want, "the {}th element", k + 1);
    }
}

/// A part in another language colors as that language colors it alone (PLAN 2.145): a Markdown
/// fence's Rust and Python and its front matter's YAML, HTML's script and style by their `type`
/// and `lang`, and Vue's and Svelte's expressions, byte for byte past the white space it starts
/// with.
#[test]
fn a_part_colors_as_its_language_alone() {
    let cases = [
        (
            "markdown",
            "Intro.\n\n```rust\nfn main() { let s = \"x\"; }\n```\n",
            "fn main() { let s = \"x\"; }\n",
            "rust",
        ),
        ("markdown", "- a\n  ```py title=\"f.py\"\n  def f(x): return f\"{x}\"\n  ```\n", "def f(x)", "python"),
        ("markdown", "---\ntitle: x\ntags: [a, b]\n---\n# Hi\n", "title: x\ntags: [a, b]\n", "yaml"),
        (
            "html",
            "<p>a</p><script>const n = 1 < 2 ? `a${b}` : /re/g;</script>",
            "const n = 1 < 2 ? `a${b}` : /re/g;",
            "javascript",
        ),
        ("html", "<script lang=\"ts\">let n: number = 1;</script>", "let n: number = 1;", "typescript"),
        ("html", "<script type=\"importmap\">{\"imports\": {}}</script>", "{\"imports\": {}}", "json"),
        ("html", "<style>a > b { color: #fff; }</style>", "a > b { color: #fff; }", "css"),
        ("vue", "<p :a=\"x.y(1)\">{{ x ? 'a' : b }}</p>", "x.y(1)", "javascript"),
        ("vue", "<p :a=\"x.y(1)\">{{ x ? 'a' : b }}</p>", " x ? 'a' : b ", "javascript"),
        ("vue", "<style lang=\"scss\">a { &.b { c: $d; } }</style>", "a { &.b { c: $d; } }", "scss"),
        ("svelte", "<p>{items.map((i) => i * 2)}</p>", "items.map((i) => i * 2)", "javascript"),
    ];
    for (container, text, part, language) in cases {
        let at = text.find(part).unwrap();
        // A part found inside its line: the alone reading starts where the part does.
        let whole = highlight(text, lang(container));
        let alone = highlight(part, lang(language));
        let lead = part.bytes().take_while(u8::is_ascii_whitespace).count();
        for k in lead..part.len() {
            assert_eq!(token_at(&whole, at + k).0, token_at(&alone, k).0, "{container}: {part:?} at {k}");
        }
    }
    // A script whose type no browser runs is a template's text, plain.
    let text = "<script type=\"text/x-template\">if (a) { b(); }</script>";
    assert_eq!(token_at(&highlight(text, lang("html")), text.find("if").unwrap()).0, Class::Plain);
}

/// PHP switches between HTML and its code at `<?php`, `<?=`, and `?>`, in text and in an
/// attribute's value; a `?>` in a string is the string's, and one in a line comment ends the
/// code, as PHP ends it. A PHP text with no opener is code throughout, and `<?xml` opens none.
#[test]
fn a_php_file_switches_between_html_and_php() {
    let text = "<?xml version=\"1.0\"?>\n<p class=\"<?= $c ?>\"><?php echo '?>'; // done ?></p>\n<?php $n = 1;";
    let tokens = highlight(text, lang("php"));
    let class = |at: usize| token_at(&tokens, at).0;
    let find = |what: &str| text.find(what).unwrap();
    assert_eq!(class(find("<?xml")), Class::Meta);
    assert_eq!(class(find("p class")), Class::Tag);
    assert_eq!(class(find("<?=")), Class::Meta);
    assert_eq!(class(find("$c")), Class::Variable);
    assert_eq!(class(find(" ?>\"") + 1), Class::Meta);
    assert_eq!(class(find("<?php echo")), Class::Meta);
    assert_eq!(class(find("echo")), Class::Keyword);
    assert_eq!(class(find("'?>'") + 1), Class::String);
    assert_eq!(class(find("// done")), Class::Comment);
    assert_eq!(class(find("done ?>") + 5), Class::Meta);
    assert_eq!(class(find("</p>") + 2), Class::Tag);
    assert_eq!(class(find("$n")), Class::Variable);
    assert_eq!(class(find("1;")), Class::Number);
    // A snippet, with no opener: code throughout.
    let text = "$double = fn($y) => $y * 2; // twice";
    let tokens = highlight(text, lang("php"));
    assert_eq!(token_at(&tokens, 0).0, Class::Variable);
    assert_eq!(token_at(&tokens, text.find("fn").unwrap()).0, Class::Keyword);
    assert_eq!(token_at(&tokens, text.find("//").unwrap()).0, Class::Comment);
}

/// Markdown colors its headings, emphasis, code spans, links, and fences, a fence in the language
/// its info string names; and it is never detected by itself, only named by a fence.
#[test]
fn markdown_colors_what_it_marks_and_is_never_detected_by_itself() {
    use scaena_core::code::{Detected, Hints, detect};
    let text =
        "# Title\n\nSome *em*, **strong**, `code`, a_snake_case and [a](https://x.y).\n\n```{.python}\nx = 1\n```\n";
    let tokens = highlight(text, lang("markdown"));
    let class = |what: &str| token_at(&tokens, text.find(what).unwrap()).0;
    assert_eq!(class("# Title"), Class::Keyword);
    assert_eq!(class("*em*"), Class::Variable);
    assert_eq!(class("**strong**"), Class::Constant);
    assert_eq!(class("`code`"), Class::String);
    assert_eq!(class("a_snake"), Class::Plain);
    assert_eq!(class("https"), Class::String);
    assert_eq!(class("1\n```"), Class::Number);
    let readme =
        "# Trails\n\nVolunteers rebuilt *fifty-two* miles.\n\n- One\n- Two\n\nSee [the report](https://x.y).\n";
    assert_ne!(detect(readme, &Hints::default()).language().map(|l| l.name), Some("markdown"));
    let file = Hints { file: Some("README.md"), ..Hints::default() };
    assert_ne!(detect(readme, &file).language().map(|l| l.name), Some("markdown"));
    let fenced = Hints { fence: Some("markdown"), ..Hints::default() };
    assert_eq!(detect(readme, &fenced), Detected::Named(language("markdown").unwrap()));
}

#[test]
fn a_console_reads_its_commands_as_shell_or_powershell() {
    // A prompt is meta, the command after it reads as Shell, or as PowerShell after a PowerShell
    // prompt, byte for byte, and the lines without a prompt are what the commands printed.
    let shell = "grep -c \"TODO\" src/*.rs | sort -n # count\n";
    let powershell = "Get-ChildItem -Path . -Filter *.rs | Measure-Object\n";
    let text = format!("$ {shell}3\n42 files done\nPS C:\\src> {powershell}Count : 7\n");
    let console = classes(&text, lang("console"));
    let command = 2;
    assert!(console[..command].iter().all(|&c| c == Class::Meta), "the prompt");
    assert_eq!(&console[command..command + shell.len()], classes(shell, lang("shell")).as_slice());
    let prompt = text.find("PS C:").unwrap();
    assert!(console[command + shell.len()..prompt].iter().all(|&c| c == Class::Plain), "what it printed");
    let command = prompt + "PS C:\\src> ".len();
    assert!(console[prompt..command].iter().all(|&c| c == Class::Meta), "PowerShell's prompt");
    assert_eq!(&console[command..command + powershell.len()], classes(powershell, lang("powershell")).as_slice());
    assert!(console[command + powershell.len()..].iter().all(|&c| c == Class::Plain));
    // A command goes on over the lines what it opens holds open, past their continuation prompts.
    let text = "$ cat <<EOF\n> $HOME\n> EOF\ndone\nPS> Get-Item . |\n>> Select-Object Name\nName\n";
    let tokens = highlight(text, lang("console"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("> $HOME"), Class::Meta);
    assert_eq!(class("$HOME"), Class::Variable);
    assert_eq!(class("EOF\ndone"), Class::String);
    assert_eq!(class("done"), Class::Plain);
    assert_eq!(class(">> "), Class::Meta);
    assert_eq!(class("Select-Object"), Class::Function);
    // Prompts of many shapes; and lines that only look like one are output.
    let prompts = [
        "$ ",
        "# ",
        "% ",
        "> ",
        "user@host:~/src$ ",
        "[me@box src]# ",
        "me@box ~ % ",
        "(venv) $ ",
        "bash-5.2$ ",
        "PS> ",
        "PS C:\\Users\\me> ",
    ];
    for prompt in prompts {
        let line = format!("{prompt}git status\n");
        let tokens = highlight(&line, lang("console"));
        assert_eq!(tokens[0], Token { end: prompt.len(), class: Class::Meta }, "{line:?}");
        assert_eq!(tokens[1].class, Class::Function, "{line:?}");
    }
    for line in ["$5 a month\n", "total 48\n", "=> nil\n", ">>> print(1)\n", "  indented output\n"] {
        assert_eq!(highlight(line, lang("console")), vec![Token { end: line.len(), class: Class::Plain }], "{line:?}");
    }
}

#[test]
fn an_f_string_nests_its_braces() {
    let text = r#"f"{value:{width}.{digits}f} {d["key"]!r} {{literal}} { {1: 2}[1] } {f"{inner}"}" + tail"#;
    let tokens = highlight(text, lang("python"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    // A format spec is a string that holds fields of its own.
    assert_eq!(class("value"), Class::Plain);
    assert_eq!(class("{width}"), Class::Punctuation);
    assert_eq!(class("width"), Class::Plain);
    assert_eq!(class(".{digits}"), Class::String);
    assert_eq!(class("f}"), Class::String);
    // A field's strings may take the field's own quotes (PEP 701), and its conversion is an operator.
    assert_eq!(class("\"key\""), Class::String);
    assert_eq!(class("key"), Class::String);
    assert_eq!(class("!r"), Class::Operator);
    // `{{` is a brace; a dict in a field holds a `:` that begins no format spec.
    assert_eq!(class("{{literal}}"), Class::String);
    assert_eq!(class("{1: 2}"), Class::Punctuation);
    assert_eq!(class("2}"), Class::Number);
    // An f-string in a field, and the string closes where it began.
    assert_eq!(class("inner"), Class::Plain);
    assert_eq!(class("+ tail"), Class::Operator);
    assert_eq!(class("tail"), Class::Plain);
    // A named escape's braces are its own, and a raw f-string's `\{` opens a field.
    let text = r#"f"\N{EM DASH} {a}" rf"\{b}""#;
    let tokens = highlight(text, lang("python"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("EM DASH"), Class::String);
    assert_eq!(class("a}"), Class::Plain);
    assert_eq!(class("b}"), Class::Plain);
}

#[test]
fn a_shell_hash_inside_a_word_is_no_comment() {
    let text = "echo a#b x=1#2 ${#items[@]} $# \"#quoted\" 'a#b';#after\nls # a comment\n";
    let tokens = highlight(text, lang("shell"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("#b"), Class::Plain);
    assert_eq!(class("#2"), Class::Plain);
    assert_eq!(class("#items"), Class::Variable);
    assert_eq!(class("$#"), Class::Variable);
    assert_eq!(class("#quoted"), Class::String);
    // A word starts after `;`, and after a space.
    assert_eq!(token_at(&tokens, text.find("#after").unwrap()), (Class::Comment, text.find("\nls").unwrap() + 1));
    assert_eq!(class("ls"), Class::Function);
    assert_eq!(class("# a comment"), Class::Comment);
    // In a console's command too.
    let text = "$ echo a#b # a note\na#b\n";
    let tokens = highlight(text, lang("console"));
    assert_eq!(token_at(&tokens, text.find("#b").unwrap()).0, Class::Plain);
    assert_eq!(token_at(&tokens, text.find("# a note").unwrap()).0, Class::Comment);
    assert_eq!(token_at(&tokens, text.rfind("a#b").unwrap()).0, Class::Plain);
}

#[test]
fn a_heredoc_runs_to_its_terminator() {
    // Shell: the body expands, and runs to a line that is the delimiter alone; one indented is not.
    let text = "cat <<EOF > out\nhome: $HOME\n  EOF\nEOF\necho done\n";
    let tokens = highlight(text, lang("shell"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("out"), Class::Plain);
    assert_eq!(class("home"), Class::String);
    assert_eq!(class("$HOME"), Class::Variable);
    assert_eq!(class("  EOF"), Class::String);
    assert_eq!(class("echo"), Class::Function);
    // `<<-` strips its tabs; a quoted delimiter expands nothing; two on a line are read in turn.
    let text = "cat <<-'END' && diff <<A <<B\n\tno $expansion\n\tEND\na $x\nA\nb\nB\nls\n";
    let tokens = highlight(text, lang("shell"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("$expansion"), Class::String);
    assert_eq!(class("$x"), Class::Variable);
    assert_eq!(class("A\nb"), Class::String);
    assert_eq!(class("B\nls"), Class::String);
    assert_eq!(class("ls"), Class::Function);
    // Ruby: `<<~` lets its terminator be indented and interpolates; code goes on after the opener.
    let text = "sql = <<~SQL.strip\n  SELECT #{cols} FROM t\n  SQL\nputs sql\nx = <<-'EOS'\n  #{not}\n  EOS\ny = 1\n";
    let tokens = highlight(text, lang("ruby"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("strip"), Class::Function);
    assert_eq!(class("SELECT"), Class::String);
    assert_eq!(class("#{cols}"), Class::Punctuation);
    assert_eq!(class("cols"), Class::Plain);
    assert_eq!(class("  SQL"), Class::String);
    assert_eq!(class("puts"), Class::Function);
    assert_eq!(class("#{not}"), Class::String);
    assert_eq!(class("y = 1"), Class::Plain);
    // PowerShell's here-string runs to the line that starts with its close.
    let text = "$s = @\"\nHi $name, \"quoted\"\n\"@\nWrite-Host $s\n";
    let tokens = highlight(text, lang("powershell"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("\"quoted\""), Class::String);
    assert_eq!(class("$name"), Class::Variable);
    assert_eq!(class("Write-Host"), Class::Function);
}

#[test]
fn a_sql_dollar_body_is_one_string() {
    let body = "$$\nBEGIN\n  -- no comment here\n  RETURN 'it''s';\nEND;\n$$";
    let text = format!("CREATE FUNCTION f() RETURNS text AS {body} LANGUAGE plpgsql;\n");
    let tokens = highlight(&text, lang("sql"));
    let at = text.find("$$").unwrap();
    // One string, and the space after it, which goes to the token before.
    assert_eq!(token_at(&tokens, at), (Class::String, at + body.len() + 1));
    assert_eq!(token_at(&tokens, text.find("LANGUAGE").unwrap()).0, Class::Keyword);
    // A tagged body runs to its own tag, past a `$$` in it; `$1` is a variable.
    let body = "$fn$ SELECT '$$'; $$ $fn$";
    let text = format!("DO {body}; SELECT $1;");
    let tokens = highlight(&text, lang("sql"));
    let at = text.find("$fn$").unwrap();
    assert_eq!(token_at(&tokens, at), (Class::String, at + body.len()));
    assert_eq!(token_at(&tokens, text.find("$1").unwrap()).0, Class::Variable);
}

#[test]
fn soft_keywords_are_keywords_only_where_they_act_as_keywords() {
    let text = "match point:\n    case Point(x=0, y=_):\n        pass\n    case _:\n        pass\n\
                match = re.match(p, s)\ncase(1)\n_ = 1\ntype Pair = tuple[int, int]\nkind = type(x)\n";
    let tokens = highlight(text, lang("python"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("match point"), Class::Keyword);
    assert_eq!(class("case Point"), Class::Keyword);
    assert_eq!(class("_)"), Class::Keyword);
    assert_eq!(class("_:"), Class::Keyword);
    assert_eq!(class("match ="), Class::Plain);
    assert_eq!(class("match(p"), Class::Function);
    assert_eq!(class("case(1)"), Class::Function);
    assert_eq!(class("_ = 1"), Class::Plain);
    assert_eq!(class("type Pair"), Class::Keyword);
    assert_eq!(class("Pair"), Class::Type);
    assert_eq!(class("type(x)"), Class::Type);
}

/// `ARG` names each of its variables, a default after its `=`; `ENV`'s old form, a name then its
/// value, takes the rest of the line as the value.
#[test]
fn a_dockerfile_arg_names_each_variable() {
    let text = "ARG TARGETOS TARGETARCH\nARG A B=2\nENV PATH /usr/bin:$PATH\n";
    let tokens = highlight(text, lang("dockerfile"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("TARGETOS"), Class::Variable);
    assert_eq!(class("TARGETARCH"), Class::Variable);
    assert_eq!(class("A B"), Class::Variable);
    assert_eq!(class("B=2"), Class::Variable);
    assert_eq!(class("2\n"), Class::Number);
    assert_eq!(class("PATH /"), Class::Variable);
    assert_eq!(class("/usr"), Class::Plain);
}

#[test]
fn a_dockerfile_reads_its_commands_as_shell() {
    // `RUN`, `CMD`, `ENTRYPOINT`, and a health check's `CMD` read as Shell, byte for byte, over the
    // lines the escape joins.
    let joined = "apt-get update && \\\n    apt-get install -y --no-install-recommends \"curl=$CURL\" \\\n    && rm -rf /var/lib/apt/lists/*\n";
    let looped = "for f in *.conf; do cp \"$f\" /etc/; done\n";
    let tested = "[ -f /run/ready ] || exit 1\n";
    let cases = [("RUN", joined), ("CMD", looped), ("ENTRYPOINT", looped), ("HEALTHCHECK --interval=5s CMD", tested)];
    for (instruction, command) in cases {
        let text = format!("FROM debian:12\n{instruction} {command}COPY . /app\n");
        let docker = classes(&text, lang("dockerfile"));
        // An image's tag is a name, not a number.
        assert!(docker[5..14].iter().all(|&c| c == Class::Plain), "debian:12");
        let at = text.find(command).unwrap();
        assert_eq!(&docker[at..at + command.len()], classes(command, lang("shell")).as_slice(), "{instruction}");
        assert_eq!(docker[text.find("COPY").unwrap()], Class::Keyword, "{instruction}: the next instruction");
    }
    // A heredoc that is the whole command is its script, read as Shell to its terminator.
    let script = "set -e\nif [ -n \"$CI\" ]; then\n  make test\nfi\n";
    let text = format!("RUN <<EOF\n{script}EOF\nCOPY . .\n");
    let docker = classes(&text, lang("dockerfile"));
    let at = text.find(script).unwrap();
    assert_eq!(&docker[at..at + script.len()], classes(script, lang("shell")).as_slice());
    assert_eq!(docker[text.rfind("EOF").unwrap()], Class::String);
    // Docker drops a comment and a blank line among the lines it joins, so the words after them
    // go on the command: `curl` and `git` are arguments, not commands.
    let text = "RUN apt-get install -y \\\n  # the tools\n  curl \\\n\n  git\n";
    let tokens = highlight(text, lang("dockerfile"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("# the tools"), Class::Comment);
    assert_eq!(class("curl"), Class::Plain);
    assert_eq!(class("git"), Class::Plain);
    // Where `SHELL` names PowerShell, or the escape is a backtick, commands read as PowerShell.
    let command = "Get-ChildItem -Path C:\\app | Remove-Item -Recurse\n";
    for head in ["SHELL [\"pwsh\", \"-Command\"]\n", "# escape=`\nFROM windows\n"] {
        let text = format!("{head}RUN {command}");
        let docker = classes(&text, lang("dockerfile"));
        let at = text.find(command).unwrap();
        assert_eq!(&docker[at..], classes(command, lang("powershell")).as_slice(), "{head:?}");
    }
}

#[test]
fn a_makefile_reads_its_recipes_as_shell() {
    // A recipe's line reads as Shell, byte for byte, after its tab.
    let recipe = "if [ -d build ]; then rm -rf build; fi && echo \"cleaned\" | tee log\n";
    let text = format!("clean:\n\t{recipe}");
    let make = classes(&text, lang("makefile"));
    let at = text.find(recipe).unwrap();
    assert_eq!(&make[at..], classes(recipe, lang("shell")).as_slice());
    // Make expands what it writes first: `$(CC)` and `$@` are variables, in quotes too, and `$$`
    // is the shell's `$`.
    let text = "app: main.o\n\t@$(CC) -o $@ $^ && echo \"$(VERSION) in $$HOME\"\n";
    let tokens = highlight(text, lang("makefile"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("@"), Class::Operator);
    assert_eq!(class("$(CC)"), Class::Variable);
    assert_eq!(class("$@"), Class::Variable);
    assert_eq!(class("$(VERSION)"), Class::Variable);
    assert_eq!(class(" in "), Class::String);
    assert_eq!(class("$$HOME"), Class::Variable);
    // What follows a rule's `;` is its recipe, prefixes and all, and an `=` in it is the shell's:
    // the rule sets no variable. One it does set may be made private first.
    let text = "print-%: ; @echo $*=$($*)\ntest: private CFLAGS += -g\n";
    let tokens = highlight(text, lang("makefile"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("@echo"), Class::Operator);
    assert_eq!(class("echo"), Class::Function);
    assert_eq!(class("$*="), Class::Variable);
    assert_eq!(class("=$("), Class::Plain);
    assert_eq!(class("private"), Class::Keyword);
    assert_eq!(class("CFLAGS"), Class::Variable);
}

#[test]
fn a_yaml_block_scalar_runs_to_its_dedent() {
    // A body runs over the lines indented past the node that holds it, blank lines among them, to
    // the first line that is not: `#` in it begins no comment.
    let text = "steps:\n  - run: |\n      echo one # no comment\n\n      echo two\n    name: build\n  - name: >-\n      \
                folded\n      lines\nnext: |2\n    indented past\n  its indicator\nlast: 1\n";
    let tokens = highlight(text, lang("yaml"));
    let at = |needle: &str| text.find(needle).unwrap();
    let class = |needle: &str| token_at(&tokens, at(needle)).0;
    assert_eq!(class("|\n"), Class::Operator);
    assert_eq!(token_at(&tokens, at("echo one")), (Class::String, at("name: build")));
    assert_eq!(class("# no comment"), Class::String);
    assert_eq!(class("echo two"), Class::String);
    assert_eq!(class("name: build"), Class::Property);
    assert_eq!(token_at(&tokens, at("folded")), (Class::String, at("next")));
    assert_eq!(token_at(&tokens, at("indented")), (Class::String, at("last")));
    assert_eq!(class("last"), Class::Property);
    assert_eq!(class("1\n"), Class::Number);
    // At a document's top, a body runs to the marker that ends the document.
    let text = "--- |\nline one\nline two\n...\n--- >\nnext document\n";
    let tokens = highlight(text, lang("yaml"));
    let at = |needle: &str| text.find(needle).unwrap();
    assert_eq!(token_at(&tokens, at("line one")), (Class::String, at("...")));
    assert_eq!(token_at(&tokens, at("...")).0, Class::Meta);
    assert_eq!(token_at(&tokens, at("next document")), (Class::String, text.len()));
}

#[test]
fn a_diff_line_is_what_its_first_character_makes_it() {
    // In a hunk, its header's counts say how many lines it holds, so lines that only look like a
    // file's header are taken away and added.
    let text = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@ fn main() {\n context\n-old\n+new\n--- \
                removed, though it looks like a header\n+++ added, though it looks like one\n";
    let tokens = highlight(text, lang("diff"));
    let at = |needle: &str| text.find(needle).unwrap();
    let class = |needle: &str| token_at(&tokens, at(needle)).0;
    assert_eq!(class("diff --git"), Class::Meta);
    assert_eq!(class("--- a/f"), Class::Meta);
    assert_eq!(class("+++ b/f"), Class::Meta);
    assert_eq!(class("@@ -1,3"), Class::Meta);
    assert_eq!(class("fn main"), Class::Plain);
    assert_eq!(class(" context"), Class::Plain);
    assert_eq!(token_at(&tokens, at("-old")), (Class::Deleted, at("+new")));
    assert_eq!(class("+new"), Class::Inserted);
    assert_eq!(class("--- removed"), Class::Deleted);
    assert_eq!(class("+++ added"), Class::Inserted);
    // With no hunk, a line is what its first character makes it all the same, but for the line
    // that begins a mail's signature.
    let text = "-before\n+after\n unchanged\n\\ No newline at end of file\n-- \n2.45.0\n";
    let tokens = highlight(text, lang("diff"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("-before"), Class::Deleted);
    assert_eq!(class("+after"), Class::Inserted);
    assert_eq!(class("unchanged"), Class::Plain);
    assert_eq!(class("\\ No newline"), Class::Comment);
    assert_eq!(class("-- \n"), Class::Plain);
    // A mail's message, from its headers to its patch, is plain, whatever its lines begin with.
    let text = "From 3f2a9c1 Mon Sep 17 00:00:00 2001\nSubject: [PATCH] Fix the cache\n\n- keeps entries\n\
                + drops none\n---\n f | 2 +-\n\ndiff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n";
    let tokens = highlight(text, lang("diff"));
    let class = |needle: &str| token_at(&tokens, text.find(needle).unwrap()).0;
    assert_eq!(class("- keeps"), Class::Plain);
    assert_eq!(class("+ drops"), Class::Plain);
    assert_eq!(class("---\n f"), Class::Meta);
    assert_eq!(class("-a"), Class::Deleted);
    assert_eq!(class("+b"), Class::Inserted);
}

#[test]
fn a_css_color_is_a_color_not_a_comment() {
    let text = "a { color: #fff; border: 1px solid #3366CC80; background: linear-gradient(#000, #abcd) }\n\
                #main > .x { color: #F00 }\n";
    for name in ["css", "scss"] {
        let tokens = highlight(text, lang(name));
        for color in ["#fff", "#3366CC80", "#000", "#abcd", "#F00"] {
            let at = text.find(color).unwrap();
            assert_eq!(token_at(&tokens, at).0, Class::Constant, "{name}: {color}");
            assert_eq!(token_at(&tokens, at + color.len() - 1).0, Class::Constant, "{name}: {color}");
        }
        // An id in a selector is a selector's.
        assert_eq!(token_at(&tokens, text.find("#main").unwrap()).0, Class::Attribute, "{name}");
    }
    // The generic scanner, which knows no CSS, read a color after a space as a comment.
    assert_eq!(token_at(&highlight(text, Lang::Generic), text.find("#fff").unwrap()).0, Class::Comment);
}

#[test]
fn lines_past_a_thousand_are_plain() {
    // A comment that never closes runs to the end of the thousandth line, and the rest is plain:
    // a line ends at a line feed, a carriage return and a line feed, or a carriage return alone.
    for end in ["\n", "\r\n", "\r"] {
        let text = format!("/*{end}{}", format!("x{end}").repeat(MAX_LINES + 4));
        let cut = text.match_indices(end).nth(MAX_LINES - 1).map(|(i, _)| i + end.len()).unwrap();
        let want = vec![Token { end: cut, class: Class::Comment }, Token { end: text.len(), class: Class::Plain }];
        assert_eq!(highlight(&text, lang("rust")), want, "lines ended by {end:?}");
    }
    // So do what other scanners leave open: a Python string, a Ruby heredoc and embedded
    // document, a Shell heredoc and a console's, a PowerShell comment, a SQL comment, YAML's block
    // scalar and quoted scalar, TOML's multi-line string, the heredocs of a Dockerfile and of HCL,
    // HCL's and CSS's comments, GraphQL's block string, and markup's comment and CDATA.
    let open = [
        ("python", "\"\"\"", Class::String),
        ("ruby", "x = <<~EOS", Class::String),
        ("ruby", "=begin", Class::Comment),
        ("shell", "cat <<EOF", Class::String),
        ("console", "$ cat <<EOF", Class::String),
        ("powershell", "<#", Class::Comment),
        ("sql", "/*", Class::Comment),
        ("yaml", "--- |", Class::String),
        ("yaml", "key: \"", Class::String),
        ("toml", "s = \"\"\"", Class::String),
        ("dockerfile", "COPY <<EOF /x", Class::String),
        ("hcl", "x = <<EOT", Class::String),
        ("hcl", "/*", Class::Comment),
        ("graphql", "\"\"\"", Class::String),
        ("html", "<!--", Class::Comment),
        ("xml", "<![CDATA[", Class::String),
        ("css", "/*", Class::Comment),
        ("scss", "/*", Class::Comment),
    ];
    for (name, opener, class) in open {
        let text = format!("{opener}\n{}", "x\n".repeat(MAX_LINES + 4));
        let cut = text.match_indices('\n').nth(MAX_LINES - 1).map(|(i, _)| i + 1).unwrap();
        let tokens = highlight(&text, lang(name));
        let [.., before, last] = tokens.as_slice() else { panic!("{name}: too few tokens") };
        assert_eq!((before.end, before.class), (cut, class), "{name}: {opener:?}");
        assert_eq!((last.end, last.class), (text.len(), Class::Plain), "{name}: {opener:?}");
    }
    // A diff's lines are what their first characters make them, to the thousandth.
    let text = "+x\n".repeat(MAX_LINES + 4);
    let want =
        vec![Token { end: 3 * MAX_LINES, class: Class::Inserted }, Token { end: text.len(), class: Class::Plain }];
    assert_eq!(highlight(&text, lang("diff")), want);
    // Code past the thousandth line is one plain token.
    let line = "fn f() { g(1); }\n";
    let text = line.repeat(MAX_LINES + 5);
    let tokens = highlight(&text, lang("rust"));
    let [.., before, last] = tokens.as_slice() else { panic!("too few tokens") };
    assert_eq!((before.end, before.class), (line.len() * MAX_LINES, Class::Punctuation));
    assert_eq!((last.end, last.class), (text.len(), Class::Plain));
    // A thousand lines are colored whole.
    let text = line.repeat(MAX_LINES);
    assert_eq!(highlight(&text, lang("rust")).last().map(|t| t.class), Some(Class::Punctuation));
}

#[test]
fn languages_are_sorted_and_each_alias_names_one() {
    assert!(LANGUAGES.windows(2).all(|w| w[0].name < w[1].name), "LANGUAGES is not in the order of its names");
    let mut names = BTreeMap::new();
    let mut extensions = BTreeMap::new();
    let mut interpreters = BTreeMap::new();
    let mut files = BTreeMap::new();
    for known in LANGUAGES {
        for name in std::iter::once(&known.name).chain(known.aliases) {
            assert!(lowercase_name(name), "{name}: a language's name is lowercase");
            if let Some(other) = names.insert(*name, known.name) {
                panic!("{name} names {other} and {}", known.name);
            }
            assert_eq!(language(name), Some(known), "{name}");
            assert_eq!(language(&name.to_uppercase()), Some(known), "{name} in capitals");
        }
        for extension in known.extensions {
            assert!(lowercase_name(extension), "{extension}");
            if let Some(other) = extensions.insert(*extension, known.name) {
                panic!(".{extension} is {other}'s and {}'s", known.name);
            }
        }
        for interpreter in known.interpreters {
            if let Some(other) = interpreters.insert(*interpreter, known.name) {
                panic!("{interpreter} runs {other} and {}", known.name);
            }
        }
        for file in known.files {
            assert!(!file.is_empty() && !file.contains(['/', '\\']), "{file:?}: a file's name");
            if let Some(other) = files.insert(*file, known.name) {
                panic!("{file} is {other}'s and {}'s", known.name);
            }
        }
        assert!(!known.said.is_empty(), "{} says nothing", known.name);
    }
    // The files a language's files are named whole: a Dockerfile, and a Makefile.
    for (file, name) in [("Dockerfile", "dockerfile"), ("Makefile", "makefile"), ("GNUmakefile", "makefile")] {
        assert_eq!(files.get(file), Some(&name), "{file}");
    }
    assert_eq!(lang("RS"), lang("rust"));
    assert!(matches!(lang("c++"), Lang::Known(l) if l.name == "cpp"));
    assert_eq!(lang("Text"), Lang::Plain);
    // Each name of 2.134's languages names its own scanner's language.
    let names = [
        ("python", ["py", "python3", "PYTHON"].as_slice()),
        ("ruby", &["rb", "Ruby"]),
        ("shell", &["bash", "sh", "zsh", "shellscript", "shell-script"]),
        ("console", &["sh-session", "shellsession", "shell-session"]),
        ("powershell", &["ps1", "pwsh", "posh", "ps", "PowerShell"]),
        ("sql", &["postgres", "postgresql", "pgsql", "plpgsql", "mysql", "sqlite", "tsql", "SQL"]),
    ];
    for (name, aliases) in names {
        for alias in aliases {
            assert!(matches!(lang(alias), Lang::Known(l) if l.name == name), "{alias} names {name}");
        }
    }
    // And each of 2.135's.
    let names = [
        ("yaml", ["yml", "YAML"].as_slice()),
        ("toml", &["TOML"]),
        ("dockerfile", &["docker", "containerfile", "Dockerfile"]),
        ("diff", &["patch", "udiff"]),
        ("hcl", &["tf", "terraform", "tfvars"]),
        ("graphql", &["gql"]),
        ("makefile", &["make", "mk"]),
        ("ini", &["cfg", "conf", "properties", "editorconfig", "dosini"]),
        ("html", &["htm", "xhtml"]),
        ("xml", &["svg", "plist", "xsl", "xslt", "xsd", "rss"]),
        ("css", &["CSS"]),
        ("scss", &["SCSS"]),
    ];
    for (name, aliases) in names {
        for alias in aliases {
            assert!(matches!(lang(alias), Lang::Known(l) if l.name == name), "{alias} names {name}");
        }
    }
}

#[test]
fn an_unknown_name_colors_by_the_generic_scanner() {
    for name in ["elixir", "lua", "", "rust2", "c--"] {
        assert_eq!(lang(name), Lang::Generic, "{name:?}");
        assert_eq!(language(name), None, "{name:?}");
    }
    let text = "local s = \"x\" -- note\nprint(s, 42)\n";
    let tokens = highlight(text, lang("lua"));
    assert_eq!(tokens, highlight(text, Lang::Generic));
    let classes: Vec<Class> = tokens.iter().map(|t| t.class).collect();
    for class in [Class::String, Class::Comment, Class::Number, Class::Punctuation, Class::Operator] {
        assert!(classes.contains(&class), "{class:?} in {classes:?}");
    }
    // Plain text is one plain token, and an empty text none.
    assert_eq!(highlight("fn main() {}", lang("plain")), vec![Token { end: 12, class: Class::Plain }]);
    assert_eq!(highlight("", lang("rust")), vec![]);
    assert_eq!(highlight("", Lang::Generic), vec![]);
}

#[test]
fn the_sixteen_kinds_are_named_once() {
    let names: Vec<&str> = Class::KINDS.iter().map(|k| k.name()).collect();
    assert_eq!(
        names,
        [
            "keyword",
            "type",
            "function",
            "string",
            "number",
            "constant",
            "comment",
            "operator",
            "punctuation",
            "property",
            "attribute",
            "tag",
            "variable",
            "meta",
            "inserted",
            "deleted"
        ]
    );
    assert!(!Class::KINDS.contains(&Class::Plain));
    assert_eq!(Class::Plain.name(), "plain");
}

/// The pieces random text is made of: what opens and closes what the scanners know, and text.
/// Those after `5.` are the constructs of Python, Ruby, Shell, consoles, PowerShell, and SQL,
/// those after `?|` of YAML, TOML, Dockerfiles, diffs, HCL, GraphQL, Makefiles, INI, HTML, XML,
/// CSS, and SCSS, and those after `&-` of Markdown, PHP, Vue, and Svelte.
const PIECES: &[&str] = &[
    " ",
    "  ",
    "\t",
    "\n",
    "\r\n",
    "\r",
    "\u{c}",
    "\u{a0}",
    "é",
    "漢字",
    "🦀",
    "\u{2028}",
    "\u{0}",
    "a",
    "x1",
    "_",
    "$",
    "$x",
    "Foo",
    "MAX",
    "fn",
    "class",
    "return",
    "this",
    "self",
    "true",
    "null",
    "function",
    "import",
    "case",
    "\"",
    "'",
    "`",
    "\"\"\"",
    "'''",
    "\\",
    "\\\\",
    "\\\"",
    "\\(",
    "\\#(",
    "${",
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "<",
    ">",
    "</",
    "/>",
    "<>",
    "</>",
    "<div ",
    "<A.B",
    "=",
    "==",
    "=>",
    "->",
    "!",
    "!=",
    "?",
    "?.",
    ".",
    "..",
    "...",
    ",",
    ";",
    ":",
    "::",
    "+",
    "-",
    "--",
    "*",
    "/",
    "//",
    "/*",
    "*/",
    "#",
    "#!",
    "#[",
    "#![",
    "#\"",
    "\"#",
    "##",
    "#include",
    "#if",
    "@",
    "@\"",
    "$\"",
    "$@\"",
    "@$\"",
    "$\"\"\"",
    "{{",
    "}}",
    "r\"",
    "r#\"",
    "br#\"",
    "r#x",
    "b'",
    "c\"",
    "R\"x(",
    ")x\"",
    "u8\"",
    "L'",
    "s\"",
    "f\"\"\"",
    "'a",
    "'\\''",
    "'é'",
    "''",
    "0",
    "1",
    "0x1F",
    "1e-5",
    "1'000",
    "1_000",
    ".5",
    "1..2",
    "/re/g",
    "/[/]/",
    "%",
    "&&",
    "|",
    "^",
    "~",
    "println!",
    "@Override",
    "@import",
    "=\"v\"",
    "{/* c */}",
    "\\\\ zig",
    "<T,>",
    "<T extends ",
    "<A<B>",
    "[A]",
    "{x:C2}",
    "string? Name",
    "->int",
    "a++ / 2",
    "x.0.1",
    "::<&str>",
    "| ",
    "5.",
    "f\"{",
    "rb'",
    "t'{",
    "!r",
    ":>{",
    "\\N{",
    "@dataclass",
    "match ",
    "case _:",
    "type T = ",
    "def ",
    "#{",
    "#@x",
    "<<~EOS",
    "<<-EOS",
    "<<'EOS'",
    "<<EOF",
    "<<-'EOF'",
    "\nEOS\n",
    "\n\tEOF\n",
    "%w[",
    "%i(",
    "%q{",
    "%Q<",
    "%r|",
    "%(",
    ":sym",
    ":\"",
    "@@x",
    "$!",
    "?a",
    "\n=begin\n",
    "\n=end\n",
    "\n__END__\n",
    "|x|",
    "&.",
    "$(",
    "$((",
    "))",
    "${",
    "$'",
    ";;",
    "esac",
    "case ",
    " in ",
    "[[ ",
    " ]]",
    "2>&1",
    "<(",
    "a#b",
    "\\\n",
    "do\n",
    "done",
    "then",
    "\n$ ",
    "\n# ",
    "\nPS C:\\> ",
    "\n> ",
    "\n>> ",
    "\nu@h:~$ ",
    "\n(venv) % ",
    "@\"\n",
    "\n\"@",
    "@'\n",
    "\n'@",
    "<#",
    "#>",
    "@(",
    "@{",
    "[string]",
    "[Parameter(",
    "-eq",
    "Get-Item",
    "$env:x",
    "$$",
    "$t$",
    "E'",
    "N'",
    "[x y]",
    "SELECT ",
    "?|",
    "\n---\n",
    "\n...\n",
    "- ",
    "? ",
    ": ",
    "key: ",
    "|\n  ",
    ">-\n",
    "|2\n",
    "&anchor ",
    "*alias",
    "!!str ",
    "%YAML 1.2\n",
    "\n[table]\n",
    "[[",
    "]]",
    "a.b = ",
    "1979-05-27T07:32:00Z",
    "0xDEAD_BEEF",
    "-inf",
    "\nRUN ",
    "\nFROM ",
    "\nENV ",
    "\nCOPY ",
    "\nCMD [\"",
    "# escape=`\n",
    " \\\n",
    "--mount=",
    "<<EOF\n",
    "\nEOF\n",
    "\n+",
    "\n-",
    "\n@@ -1,2 +1,3 @@",
    "\n--- a/x\n",
    "\n+++ b/x\n",
    "\ndiff --git ",
    "\n\\ No newline",
    "%{",
    "$${",
    "<<EOT\n",
    "\nEOT\n",
    "resource \"",
    "query ",
    "fragment ",
    "...",
    "@include(",
    "\n\t",
    ":=",
    "?=",
    "\ndefine ",
    "\nendef\n",
    "ifeq (",
    ".PHONY:",
    "$@",
    "\n[section]\n",
    "%(x)s",
    "<!--",
    "-->",
    "<![CDATA[",
    "]]>",
    "<?",
    "?>",
    "<!DOCTYPE ",
    "&amp;",
    "&#x1F;",
    "<script>",
    "</script>",
    "<style>",
    "</style>",
    "<title>",
    "@media ",
    "#fff",
    "!important",
    "url(",
    "--gap",
    "::before",
    "[type=",
    "@mixin ",
    "&-",
    "%placeholder",
    "```rust\n",
    "```\n",
    "~~~\n",
    "---\n",
    "+++\n",
    "# ",
    "**",
    "_",
    "[a](",
    "](",
    "![",
    "<https:",
    "<?php ",
    "<?= ",
    "?>",
    "<<<A\n",
    "\nA;\n",
    "\"{$",
    "$a",
    "{{",
    "}}",
    ":a=\"",
    "{#if ",
    "{:else}",
    "{/if}",
    "{@html ",
];

/// Openers, and what closes each, nested 100 deep.
const DEEP: &[(&str, &str)] = &[
    ("(", ")"),
    ("[", "]"),
    ("{", "}"),
    ("`${", "}`"),
    ("\"\\(", ")\""),
    ("$\"{", "}\""),
    ("s\"${", "}\""),
    ("<a>", "</a>"),
    ("<a b={", "}/>"),
    ("<a>{", "}</a>"),
    ("/*", "*/"),
    ("#[", "]"),
    ("r#\"", "\"#"),
    ("f\"{", "}\""),
    ("\"#{", "}\""),
    ("%w(", ")"),
    ("$(", ")"),
    ("\"$(", ")\""),
    ("${", "}"),
    ("$((", "))"),
    ("@(", ")"),
    ("<#", "#>"),
    ("$$", "$$"),
    ("- [", "]"),
    ("{a: ", "}"),
    ("[[", "]]"),
    ("x = {", "}"),
    ("\"%{", "}\""),
    ("<<EOT\n", "\nEOT\n"),
    ("$(a ", ")"),
    ("<!--", "-->"),
    ("<![CDATA[", "]]>"),
    ("<a b=\"", "\">"),
    ("a { b: #{", "} }"),
    ("@media (", ") {}"),
    ("```markdown\n", "\n```\n"),
    ("<?php \"{$", "}\" ?>"),
    ("{{ (", ") }}"),
    ("{#if (", ")}{/if}"),
    ("<script>(", ")</script>"),
];

/// A text of `pieces` pieces at most, picked at random; with `breaks`, line breaks among them.
fn random(rng: &mut Rng, pieces: usize, breaks: bool) -> String {
    let mut text = String::new();
    for _ in 0..rng.below(pieces + 1) {
        let piece = rng.pick(PIECES);
        if breaks || !piece.contains(['\n', '\r']) {
            text.push_str(piece);
        }
    }
    text
}

/// Every way a code text is colored: each language's own scanner, plain text, and the generic.
fn langs() -> Vec<Lang> {
    let mut langs: Vec<Lang> = LANGUAGES.iter().map(|l| lang(l.name)).collect();
    langs.push(Lang::Generic);
    langs
}

/// `text`'s tokens, held to what every scanner keeps to: they cover the text in order, each
/// ending on a character's boundary after the one before, neighbors in other classes. A second
/// reading gives the same tokens.
fn check(text: &str, lang: Lang) {
    let tokens = highlight(text, lang);
    let mut start = 0;
    let mut before = None;
    for token in &tokens {
        assert!(token.end > start, "{lang:?}: a token ends at {} before {start} in {text:?}", token.end);
        assert!(text.is_char_boundary(token.end), "{lang:?}: {} is inside a character of {text:?}", token.end);
        assert_ne!(before, Some(token.class), "{lang:?}: two {:?} tokens side by side in {text:?}", token.class);
        start = token.end;
        before = Some(token.class);
    }
    assert_eq!(start, text.len(), "{lang:?}: the tokens stop short of the end of {text:?}");
    assert_eq!(tokens, highlight(text, lang), "{lang:?}: read twice, {text:?} reads otherwise");
}

/// The class of each byte of `text` in `lang`.
fn classes(text: &str, lang: Lang) -> Vec<Class> {
    let mut out = Vec::with_capacity(text.len());
    for token in highlight(text, lang) {
        out.resize(token.end, token.class);
    }
    out
}

/// The class of the token that holds byte `at`, and where that token ends.
fn token_at(tokens: &[Token], at: usize) -> (Class, usize) {
    let token = tokens.iter().find(|t| t.end > at).expect("a token holds it");
    (token.class, token.end)
}

/// The tokens as the goldens write them.
fn spans(text: &str, tokens: &[Token]) -> String {
    let mut out = String::new();
    let mut start = 0;
    for token in tokens {
        let mut shown = String::new();
        for c in text[start..token.end].chars() {
            match c {
                '\n' => shown.push_str("\\n"),
                '\r' => shown.push_str("\\r"),
                '\t' => shown.push_str("\\t"),
                '\\' => shown.push_str("\\\\"),
                c => shown.push(c),
            }
        }
        out.push_str(&format!("{:<11} «{shown}»\n", token.class.name()));
        start = token.end;
    }
    out
}

/// Whether `name` is as a deck's language is written: lowercase, at most 32 characters.
fn lowercase_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 32
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"+#._-".contains(&c))
}

/// The samples' texts.
fn samples() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/code");
    let mut out = Vec::new();
    for dir in entries(&root).into_iter().filter(|d| d.is_dir() && !d.ends_with("actual")) {
        for file in entries(&dir).into_iter().filter(|f| f.extension().is_some_and(|e| e == "txt")) {
            out.push(std::fs::read_to_string(file).unwrap());
        }
    }
    out
}

/// What a directory holds, in the order of its names.
fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    out.sort();
    out
}

/// The nearest character boundary at or after `at`.
fn boundary(text: &str, at: usize) -> usize {
    (at..=text.len()).find(|&i| text.is_char_boundary(i)).unwrap()
}

/// A deterministic xorshift generator: the test's texts are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// The word tables the scanners read are a file each, which the editor's page hands its module the
/// first time a code text it colors lays out (PLAN 2.132, ADR-0015): `crates/scaena-core/code/`
/// holds each file as the tables compiled in write it, which `just bless` writes, and a file reads
/// back as those tables, key for key and word for word.
#[test]
fn each_file_of_tables_is_the_tables_compiled_in() {
    use scaena_core::code::tables::{FILES, compiled, read, written};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("code");
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    let mut keys = Vec::new();
    for name in FILES {
        let text = written(name).expect("this build compiles the tables in");
        let path = root.join(format!("{name}.txt"));
        if bless {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(&path, &text).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap_or_default(),
            text,
            "{}: bless with SCAENA_BLESS=1",
            path.display()
        );
        let tables = compiled(name).unwrap();
        let back = read(&text);
        assert_eq!(back.len(), tables.len(), "{name}");
        for ((key, words), (read_key, read_words)) in tables.iter().zip(&back) {
            assert_eq!((key, words.to_vec()), (read_key, read_words.clone()), "{name}");
            assert!(key.starts_with(&format!("{name}/")) && !words.is_empty(), "{key}");
            assert!(words.iter().all(|w| !w.is_empty() && !w.contains('\n')), "{key}");
            keys.push(key.to_string());
        }
    }
    let count = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), count, "a key names two tables");
}

/// Every word list a scanner reads is a table of its file (`words!`), so the editor's module
/// leaves none of them in: no scanner keeps a list of words a module carries whole, and each
/// module lists each table it writes in its `WORDS`.
#[test]
fn every_word_list_a_scanner_reads_is_a_table() {
    use scaena_core::code::tables::{FILES, compiled};
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/code");
    for file in entries(&dir) {
        let source = std::fs::read_to_string(&file).unwrap();
        let code = source.split("#[cfg(test)]").next().unwrap_or_default();
        let name = file.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(!code.contains(": &[&str] = &["), "{name}: a list of words is a table (`words!`)");
        // A table's key may stand on the line after `words!(`.
        let tables = code.split("words!(").skip(1).filter(|rest| rest.trim_start().starts_with('"')).count();
        match FILES.iter().find(|f| **f == name) {
            Some(f) => assert_eq!(compiled(f).unwrap().len(), tables, "{name}: a table missing from its `WORDS`"),
            None => assert_eq!(tables, 0, "{name} writes tables but is no file of them"),
        }
    }
}

/// A file of tables is taken only for its own name, with its own tables, and a build that compiles
/// them in has every file already.
#[test]
fn a_file_of_tables_is_taken_only_as_itself() {
    use scaena_core::code::tables::{FILES, files, hand, has};
    assert!(hand("cobol", "cobol/keywords\nmove\n\n").unwrap_err().contains("no file of code tables"));
    assert!(FILES.iter().all(|f| has(f)));
    assert_eq!(files(lang("rust")), ["clike"]);
    assert_eq!(files(lang("console")), ["powershell", "shell"]);
    assert!(files(lang("json")).is_empty() && files(lang("plain")).is_empty() && files(lang("lua")).is_empty());
    for language in LANGUAGES {
        for file in files(lang(language.name)) {
            assert!(FILES.contains(file), "{}: {file}", language.name);
        }
    }
}

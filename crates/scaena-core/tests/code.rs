//! Code colored by its language (PLAN 2.130 and 2.134, ADR-0029): the scanners' tokens, held to
//! reviewed goldens, and what every scanner keeps to on any text at all.
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
/// C#'s attributes and formats, and the spaces before a name a type declares; f-strings, heredocs,
/// `%`-literals, substitutions, prompts, dollar quotes, names in brackets, and the spaces a soft
/// keyword, a Ruby method's argument, or a heredoc's delimiter is looked for past.
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
        "\"$(".repeat(10_000),
        "${".repeat(15_000),
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
    ];
    for lang in langs() {
        for run in &runs {
            check(run, lang);
        }
    }
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
    // Heredocs that wait for their bodies: the ninth `<<` on a line opens none.
    for (name, text) in
        [("shell", format!("cat {}\n", "<<A ".repeat(12))), ("ruby", format!("f({})\n", "<<~A, ".repeat(12)))]
    {
        let tokens = highlight(&text, lang(name));
        let ids: Vec<usize> = text.match_indices('A').map(|(i, _)| i).take(MAX_NESTING + 1).collect();
        for (k, &at) in ids.iter().enumerate() {
            assert_eq!(token_at(&tokens, at).0 == Class::String, k < MAX_NESTING, "{name}: the {}th heredoc", k + 1);
        }
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
    // document, a Shell heredoc and a console's, a PowerShell comment, and a SQL comment.
    let open = [
        ("python", "\"\"\"", Class::String),
        ("ruby", "x = <<~EOS", Class::String),
        ("ruby", "=begin", Class::Comment),
        ("shell", "cat <<EOF", Class::String),
        ("console", "$ cat <<EOF", Class::String),
        ("powershell", "<#", Class::Comment),
        ("sql", "/*", Class::Comment),
    ];
    for (name, opener, class) in open {
        let text = format!("{opener}\n{}", "x\n".repeat(MAX_LINES + 4));
        let cut = text.match_indices('\n').nth(MAX_LINES - 1).map(|(i, _)| i + 1).unwrap();
        let tokens = highlight(&text, lang(name));
        let [.., before, last] = tokens.as_slice() else { panic!("{name}: too few tokens") };
        assert_eq!((before.end, before.class), (cut, class), "{name}: {opener:?}");
        assert_eq!((last.end, last.class), (text.len(), Class::Plain), "{name}: {opener:?}");
    }
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
        assert!(!known.said.is_empty(), "{} says nothing", known.name);
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
/// Those after `5.` are the constructs of Python, Ruby, Shell, consoles, PowerShell, and SQL.
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

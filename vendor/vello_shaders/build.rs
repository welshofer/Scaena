// Copyright 2023 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Build step.

// These modules are also included in the main crate, where the items are reachable
#[allow(warnings, reason = "Checked elsewhere")]
#[path = "src/compile/mod.rs"]
mod compile;
#[allow(warnings, reason = "Checked elsewhere")]
#[path = "src/types.rs"]
mod types;

use std::env;
use std::fmt::{self, Write as _};
use std::path::Path;

use compile::ShaderInfo;

fn main() {
    log::set_logger(&BUILD_SCRIPT_LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("shaders.rs");

    println!("cargo:rerun-if-changed={}", compile::shader_dir().display());

    let mut shaders = match ShaderInfo::from_default() {
        Ok(s) => s,
        Err(err) => {
            let mut target = String::new();
            // Ideally, we'd write into stdout directly here, but the duck-typing of `write!`
            // makes that a bit annoying - we'd have to implement `CargoWarningAdapter` for both `io::Write` and `fmt::Write`
            writeln!(CargoWarningAdapter::new(&mut target), "{err}").unwrap();
            print!("{target}");
            return;
        }
    };

    // Drop the HashMap and sort by name so that we get deterministic order.
    let mut shaders = shaders.drain().collect::<Vec<_>>();
    shaders.sort_by(|x, y| x.0.cmp(&y.0));
    let mut buf = String::default();
    write_types(&mut buf, &shaders).unwrap();
    write_shaders(&mut buf, &shaders).unwrap();
    std::fs::write(dest_path, &buf).unwrap();
}

fn write_types(buf: &mut String, shaders: &[(String, ShaderInfo)]) -> Result<(), fmt::Error> {
    writeln!(buf, "pub struct Shaders<'a> {{")?;
    for (name, _) in shaders {
        writeln!(buf, "    pub {name}: ComputeShader<'a>,")?;
    }
    writeln!(buf, "}}")?;
    Ok(())
}

fn write_shaders(buf: &mut String, shaders: &[(String, ShaderInfo)]) -> Result<(), fmt::Error> {
    writeln!(buf, "mod generated {{")?;
    writeln!(buf, "    use super::*;")?;
    writeln!(buf, "    use BindType::*;")?;
    writeln!(buf, "    pub const SHADERS: Shaders<'static> = Shaders {{")?;
    for (name, info) in shaders {
        let bind_tys = info
            .bindings
            .iter()
            .map(|binding| binding.ty)
            .collect::<Vec<_>>();
        let wg_bufs = &info.workgroup_buffers;
        writeln!(buf, "        {name}: ComputeShader {{")?;
        writeln!(buf, "            name: Cow::Borrowed({name:?}),")?;
        writeln!(
            buf,
            "            workgroup_size: {:?},",
            info.workgroup_size
        )?;
        writeln!(buf, "            bindings: Cow::Borrowed(&{bind_tys:?}),")?;
        writeln!(
            buf,
            "            workgroup_buffers: Cow::Borrowed(&{wg_bufs:?}),"
        )?;
        if cfg!(feature = "wgsl") {
            let indices = info
                .bindings
                .iter()
                .map(|binding| binding.location.1)
                .collect::<Vec<_>>();
            writeln!(buf, "            wgsl: WgslSource {{")?;
            writeln!(
                buf,
                "                code: Cow::Borrowed({:?}),",
                minified(&info.source)
            )?;
            writeln!(
                buf,
                "                binding_indices : Cow::Borrowed(&{indices:?}),"
            )?;
            writeln!(buf, "            }},")?;
        }
        if cfg!(feature = "msl") {
            write_msl(buf, info)?;
        }
        writeln!(buf, "        }},")?;
    }
    writeln!(buf, "    }};")?;
    writeln!(buf, "}}")?;
    Ok(())
}

/// The shader as WGSL with nothing a compiler reads taken out (Scaena's patch, `SCAENA.md`): its
/// comments, line and block (which WGSL nests), each line's indentation and trailing space, and
/// its blank lines. WGSL has no strings, so `//` and `/*` start a comment wherever they stand, and
/// whitespace parts no tokens a newline does not; each comment leaves a space, so the tokens either
/// side of it stay apart.
fn minified(wgsl: &str) -> String {
    let mut bare = String::with_capacity(wgsl.len());
    let mut chars = wgsl.chars().peekable();
    let mut depth = 0usize;
    while let Some(c) = chars.next() {
        if depth > 0 {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                depth -= 1;
                if depth == 0 {
                    bare.push(' ');
                }
            } else if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                depth += 1;
            } else if c == '\n' {
                bare.push('\n');
            }
        } else if c == '/' && chars.peek() == Some(&'/') {
            while chars.peek().is_some_and(|&n| n != '\n') {
                chars.next();
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            depth = 1;
        } else {
            bare.push(c);
        }
    }
    let mut out = String::with_capacity(bare.len());
    for line in bare.lines().map(str::trim).filter(|l| !l.is_empty()) {
        out.push_str(line);
        out.push('\n');
    }
    // What is embedded reads as the shader written: naga parses both to one module, whole, but
    // for where in the text each part stood.
    let before = naga::front::wgsl::parse_str(wgsl).expect("the shader reads");
    let after = naga::front::wgsl::parse_str(&out).expect("the shader reads minified");
    assert_eq!(spanless(&before), spanless(&after), "the shader minified reads as it did");
    out
}

/// A module as naga reads it, every part of it but its spans: its `Debug`, each
/// `Span { start: …, end: … }` in it said as `Span`. A span is all `Debug` prints that tells where
/// in the text a part stood, so two texts naga reads alike but for their spacing and comments
/// give one, and any other difference, a token merged or a value changed, shows.
fn spanless(module: &naga::Module) -> String {
    let written = format!("{module:?}");
    let mut out = String::with_capacity(written.len());
    let mut rest = written.as_str();
    while let Some(at) = rest.find("Span { start: ") {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let end = tail.find(" }").expect("a span's end") + 2;
        out.push_str("Span");
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(not(feature = "msl"))]
fn write_msl(_: &mut String, _: &ShaderInfo) -> Result<(), fmt::Error> {
    Ok(())
}

#[cfg(feature = "msl")]
fn write_msl(buf: &mut String, info: &ShaderInfo) -> Result<(), fmt::Error> {
    let mut index_iter = compile::msl::BindingIndexIterator::default();
    let indices = info
        .bindings
        .iter()
        .map(|binding| index_iter.next(binding.ty))
        .collect::<Vec<_>>();
    writeln!(buf, "            msl: MslSource {{")?;
    writeln!(
        buf,
        "                code: Cow::Borrowed({:?}),",
        compile::msl::translate(info).unwrap()
    )?;
    writeln!(
        buf,
        "                binding_indices : Cow::Borrowed(&{indices:?}),",
    )?;
    writeln!(buf, "            }},")?;
    Ok(())
}

/// A very simple logger for build scripts, which ensures that warnings and above
/// are visible to the user.
///
/// We don't use an external crate here to keep build times down.
struct BuildScriptLog;

static BUILD_SCRIPT_LOGGER: BuildScriptLog = BuildScriptLog;

impl log::Log for BuildScriptLog {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        // "more serious" levels are lower
        if record.level() <= log::Level::Warn {
            let mut target = String::new();
            write!(
                CargoWarningAdapter::new(&mut target),
                "{}: {}",
                record.level(),
                record.args()
            )
            .unwrap();
            println!("{target}");
        } else {
            // If the user wants more verbose output from the build script, they would pass
            // `-vv` to `cargo build`. In that case, we should provide all of the logs that
            // people chose to provide.
            // TODO: Maybe this should fall back to `env_logger`?
            eprintln!("{}: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {
        // Nothing to do; we use `println` which is "self-flushing"
    }
}

/// An adapter for `fmt::Write` which prepends `cargo:warning=` to each line, ensuring that every
/// output line is shown to build script users.
struct CargoWarningAdapter<W: fmt::Write> {
    writer: W,
    needs_warning: bool,
}

impl<W: fmt::Write> CargoWarningAdapter<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            needs_warning: true,
        }
    }
}

impl<W: fmt::Write> fmt::Write for CargoWarningAdapter<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for line in s.split_inclusive('\n') {
            if self.needs_warning {
                write!(&mut self.writer, "cargo:warning=")?;
                self.needs_warning = false;
            }
            write!(&mut self.writer, "{line}")?;
            if line.ends_with('\n') {
                self.needs_warning = true;
            }
        }
        Ok(())
    }
}

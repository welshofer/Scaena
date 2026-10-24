//! Each theme that ships drawn small by the CPU painter, for the browser to carry beside its pages
//! (PLAN T.0c): a deck made in it as New makes one, its one state a title slide in the theme's
//! `title` layout with its words to type over and its backdrop in its picture's place (PLAN 3.30,
//! `states::titled`), at rest, 480 × 270. The web build runs
//! it before it bundles the pages (`web/vite.config.ts`):
//!
//!     cargo run -q --locked -p scaena-cli --example theme_thumbnails -- target/themes
//!
//! It writes `<slug>.png` there for each theme in the registry, and nothing else.

use anyhow::{Context, Result};
use scaena_ops::render::{Painter, Request, render};
use scaena_ops::{Bundle, create, patch, shipped, states};
use serde_json::json;
use std::path::PathBuf;

fn main() -> Result<()> {
    let out = PathBuf::from(std::env::args().nth(1).context("usage: theme_thumbnails OUT_DIR")?);
    std::fs::create_dir_all(&out)?;
    let work = std::env::temp_dir().join(format!("scaena-theme-thumbnails-{}", std::process::id()));
    for theme in shipped::THEMES {
        let dir = work.join(theme.name);
        let _ = std::fs::remove_dir_all(&dir);
        let req = create::Create { theme: theme.name.into(), title: Some("Untitled".into()), ..Default::default() };
        let made = create::create(&dir, &req).with_context(|| format!("{}: making a deck in it", theme.name))?;
        anyhow::ensure!(made.created, "{}: its deck is not made: {:?}", theme.name, made.findings);
        let bundle = Bundle::open(&dir)?;
        let blank = bundle.deck.states.first().context("a new deck has a state")?.id.clone();
        let model = serde_json::from_str(theme.text).with_context(|| format!("{}: its theme", theme.name))?;
        let started = states::titled(&bundle.deck, &model, &blank)?;
        let mut ops = started.patch;
        ops.push(json!({ "op": "remove_state", "id": blank }));
        let patched = patch::patch(&bundle, &json!(ops), false)?;
        anyhow::ensure!(patched.applied, "{}: its title slide is refused", theme.name);
        let req =
            Request { state: started.id, size: Some("480x270".into()), painter: Painter::Cpu, ..Default::default() };
        let drawn = render(&dir, &req).with_context(|| format!("{}: drawing its title slide", theme.name))?;
        std::fs::write(out.join(format!("{}.png", theme.name)), drawn.png)?;
    }
    let _ = std::fs::remove_dir_all(&work);
    println!("{} themes drawn into {}", shipped::THEMES.len(), out.display());
    Ok(())
}

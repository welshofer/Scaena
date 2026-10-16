//! A forecast's icons (ADR-0030, PLAN O.2b): Phosphor's, in their regular weight, a line each in
//! `icons/weather.txt`, `name path`. They are compiled in natively and in the player's module
//! (`weather-icons`). The editor's module leaves them out, and its page hands the file over the
//! first time a forecast lays out, held to its SHA-256, as the hyphenation patterns are (ADR-0015).

use crate::EngineError;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::rc::Rc;

#[cfg(feature = "weather-icons")]
fn compiled() -> Option<&'static str> {
    Some(include_str!("../icons/weather.txt"))
}

#[cfg(not(feature = "weather-icons"))]
fn compiled() -> Option<&'static str> {
    None
}

/// What gives the icons' file where it is not compiled in: its bytes, or none.
type Loader = Box<dyn Fn() -> Option<Vec<u8>>>;

thread_local! {
    /// The file handed over.
    static HANDED: RefCell<Option<Rc<str>>> = RefCell::default();
    static LOADER: RefCell<Option<Loader>> = RefCell::default();
}

/// Where this thread's engines get the icons' file where it is not compiled in, the first time a
/// forecast lays out: `loader` gives the bytes of `icons/weather.txt`, or none. What it gives is
/// held to the file's SHA-256.
pub fn set_loader(loader: impl Fn() -> Option<Vec<u8>> + 'static) {
    LOADER.with(|l| *l.borrow_mut() = Some(Box::new(loader)));
}

/// The icons' file: compiled in, or handed over and checked.
fn file() -> Result<Rc<str>, EngineError> {
    if let Some(file) = compiled() {
        return Ok(file.into());
    }
    if let Some(file) = HANDED.with(|h| h.borrow().clone()) {
        return Ok(file);
    }
    let bytes = LOADER.with(|l| l.borrow().as_ref().and_then(|load| load())).ok_or_else(|| {
        EngineError::Layout("a forecast's icons were not handed to the engine (`setIcons`, ADR-0015)".into())
    })?;
    let hex: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if hex != crate::pin::WEATHER_SHA256 {
        return Err(EngineError::Layout(format!(
            "what was handed over as a forecast's icons ({} bytes) is not the engine's file",
            bytes.len()
        )));
    }
    let file: Rc<str> = String::from_utf8(bytes)
        .map_err(|_| EngineError::Layout("a forecast's icons' file is not UTF-8".into()))?
        .into();
    HANDED.with(|h| *h.borrow_mut() = Some(file.clone()));
    Ok(file)
}

/// The path of the forecast's icon `name` on its 256 by 256 view, as SVG writes it.
pub fn weather(name: &str) -> Result<String, EngineError> {
    let file = file()?;
    file.lines()
        .find_map(|line| line.strip_prefix(name)?.strip_prefix(' ').map(str::to_string))
        .ok_or_else(|| EngineError::Layout(format!("`{name}` is not one of a forecast's icons")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_sky_has_its_icon() {
        for code in scaena_core::weather::CODES {
            for day in [true, false] {
                let path = super::weather(code.sky.icon(day)).unwrap();
                assert!(path.starts_with('M'), "{}", code.code);
            }
        }
        assert!(super::weather("drop").is_ok() && super::weather("wind").is_ok());
        assert!(super::weather("umbrella").is_err());
    }
}

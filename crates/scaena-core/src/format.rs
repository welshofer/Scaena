//! Number and date formats (SPEC §3.7, `docs/spec/format.md`): a subset of d3-format and
//! d3-time-format, so a format an agent has seen elsewhere works here, plus one type of
//! our own (`k`, compact business numbers). Formatting is pure: the same value, format,
//! and locale give the same text on every platform (SPEC §13). Numbers round on their
//! exact binary value, ties away from zero, as JavaScript's `toFixed` does, so the text
//! matches d3's.

use std::fmt;

/// A format string that does not parse, with what was wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError(pub String);

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FormatError {}

fn bad(what: impl Into<String>) -> FormatError {
    FormatError(what.into())
}

// --- locales -------------------------------------------------------------------------

/// How a language writes numbers and dates. [`Locale::of`] picks one by BCP 47 tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    pub tag: &'static str,
    pub decimal: &'static str,
    pub group: &'static str,
    /// What `$` puts before and after the number.
    pub currency: (&'static str, &'static str),
    /// What `%` puts after the number.
    pub percent: &'static str,
    /// What `k` puts after a number of thousands, millions, billions, and trillions.
    pub compact: [&'static str; 4],
    pub months: [&'static str; 12],
    pub short_months: [&'static str; 12],
    /// From Sunday.
    pub days: [&'static str; 7],
    pub short_days: [&'static str; 7],
    pub periods: [&'static str; 2],
}

/// The minus sign numbers are set with (U+2212), as d3 sets them; the engine sets a
/// hyphen-minus where a font has none.
pub const MINUS: char = '\u{2212}';

const NBSP: &str = "\u{a0}";

const EN_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const EN_SHORT_MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const EN_DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const EN_SHORT_DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The locales formats know; any other tag formats as its language's first entry here,
/// else as `en-US`. d3's locale files for separators and currency, CLDR for the compact
/// suffixes.
pub const LOCALES: [Locale; 6] = [
    Locale {
        tag: "en-US",
        decimal: ".",
        group: ",",
        currency: ("$", ""),
        percent: "%",
        compact: ["K", "M", "B", "T"],
        months: EN_MONTHS,
        short_months: EN_SHORT_MONTHS,
        days: EN_DAYS,
        short_days: EN_SHORT_DAYS,
        periods: ["AM", "PM"],
    },
    Locale {
        tag: "en-GB",
        decimal: ".",
        group: ",",
        currency: ("£", ""),
        percent: "%",
        compact: ["K", "M", "B", "T"],
        months: EN_MONTHS,
        short_months: EN_SHORT_MONTHS,
        days: EN_DAYS,
        short_days: EN_SHORT_DAYS,
        periods: ["AM", "PM"],
    },
    Locale {
        tag: "de-DE",
        decimal: ",",
        group: ".",
        currency: ("", "\u{a0}€"),
        percent: "%",
        compact: ["\u{a0}Tsd.", "\u{a0}Mio.", "\u{a0}Mrd.", "\u{a0}Bio."],
        months: [
            "Januar",
            "Februar",
            "März",
            "April",
            "Mai",
            "Juni",
            "Juli",
            "August",
            "September",
            "Oktober",
            "November",
            "Dezember",
        ],
        short_months: ["Jan", "Feb", "Mrz", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"],
        days: ["Sonntag", "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag"],
        short_days: ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"],
        periods: ["AM", "PM"],
    },
    Locale {
        tag: "fr-FR",
        decimal: ",",
        group: NBSP,
        currency: ("", "\u{a0}€"),
        percent: "\u{a0}%",
        compact: ["\u{a0}k", "\u{a0}M", "\u{a0}Md", "\u{a0}Bn"],
        months: [
            "janvier",
            "février",
            "mars",
            "avril",
            "mai",
            "juin",
            "juillet",
            "août",
            "septembre",
            "octobre",
            "novembre",
            "décembre",
        ],
        short_months: [
            "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc.",
        ],
        days: ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"],
        short_days: ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."],
        periods: ["AM", "PM"],
    },
    Locale {
        tag: "es-ES",
        decimal: ",",
        group: ".",
        currency: ("", "\u{a0}€"),
        percent: "\u{a0}%",
        compact: ["\u{a0}mil", "\u{a0}M", "\u{a0}mil\u{a0}M", "\u{a0}B"],
        months: [
            "enero",
            "febrero",
            "marzo",
            "abril",
            "mayo",
            "junio",
            "julio",
            "agosto",
            "septiembre",
            "octubre",
            "noviembre",
            "diciembre",
        ],
        short_months: ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"],
        days: ["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"],
        short_days: ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"],
        periods: ["AM", "PM"],
    },
    Locale {
        tag: "nl-NL",
        decimal: ",",
        group: ".",
        currency: ("€\u{a0}", ""),
        percent: "%",
        compact: ["K", "\u{a0}mln.", "\u{a0}mld.", "\u{a0}bln."],
        months: [
            "januari",
            "februari",
            "maart",
            "april",
            "mei",
            "juni",
            "juli",
            "augustus",
            "september",
            "oktober",
            "november",
            "december",
        ],
        short_months: ["jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec"],
        days: ["zondag", "maandag", "dinsdag", "woensdag", "donderdag", "vrijdag", "zaterdag"],
        short_days: ["zo", "ma", "di", "wo", "do", "vr", "za"],
        periods: ["a.m.", "p.m."],
    },
];

impl Locale {
    /// The locale for a BCP 47 tag: the entry with that tag, else the first with its
    /// language, else `en-US`. Case does not matter, and `_` reads as `-`.
    pub fn of(tag: Option<&str>) -> &'static Locale {
        let tag = tag.unwrap_or("en-US").replace('_', "-");
        let language = tag.split('-').next().unwrap_or_default();
        LOCALES
            .iter()
            .find(|l| l.tag.eq_ignore_ascii_case(&tag))
            .or_else(|| {
                LOCALES.iter().find(|l| l.tag.split('-').next().is_some_and(|x| x.eq_ignore_ascii_case(language)))
            })
            .unwrap_or(&LOCALES[0])
    }
}

// --- exact decimals ------------------------------------------------------------------

/// A non-negative number as decimal digits: `d0.d1d2… × 10^exp`, no trailing zeros
/// (zero is `[0]`, exponent 0).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Decimal {
    digits: Vec<u8>,
    exp: i32,
}

/// Significant digits the quick expansion keeps. Rounding to `n ≤ 40 - 2` digits from it
/// decides the same way as from the exact expansion unless what follows the cut reads as
/// a tie, or one place short of it (see [`Decimal::exact`]).
const QUICK_DIGITS: usize = 40;

impl Decimal {
    /// `|x|`'s decimal expansion (finite). An f64 is a dyadic rational, so its expansion
    /// ends; it has at most 767 significant digits, which Rust's exact formatting gives.
    fn of(x: f64, digits_after_first: usize) -> Decimal {
        let s = format!("{:.*e}", digits_after_first, x.abs());
        let (mantissa, exp) = s.split_once('e').expect("exponent form");
        let mut digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        Decimal { digits, exp: exp.parse().expect("an integer exponent") }
    }

    fn is_zero(&self) -> bool {
        self.digits == [0]
    }

    /// `|x|` rounded to `n` significant digits, ties away from zero, on its exact value.
    fn round_significant(x: f64, n: i32) -> Decimal {
        let quick = Decimal::of(x, QUICK_DIGITS);
        if n >= 0 && (n as usize) < QUICK_DIGITS - 2 {
            // The quick expansion is itself rounded in its last place, so a tail of
            // 5000…0 or 4999…9 may be a rounding artifact: decide those exactly.
            let tail = quick.digits.get(n as usize..).unwrap_or(&[]);
            let suspect = match tail.split_first() {
                Some((5, rest)) => rest.iter().all(|&d| d == 0),
                Some((4, rest)) => !rest.is_empty() && rest.iter().all(|&d| d == 9),
                _ => false,
            };
            if !suspect {
                return quick.rounded(n);
            }
        }
        Decimal::of(x, 1100).rounded(n)
    }

    /// Keep `n` significant digits, ties away from zero; `n ≤ 0` rounds to zero or to
    /// the unit `10^(exp - n + 1)`.
    fn rounded(&self, n: i32) -> Decimal {
        if self.is_zero() {
            return self.clone();
        }
        if n < 0 {
            return Decimal { digits: vec![0], exp: 0 };
        }
        let n = n as usize;
        if n >= self.digits.len() {
            return self.clone();
        }
        let up = self.digits[n] >= 5;
        let mut digits = self.digits[..n].to_vec();
        let mut exp = self.exp;
        if up {
            let mut i = digits.len();
            loop {
                if i == 0 {
                    digits.insert(0, 1);
                    exp += 1;
                    break;
                }
                i -= 1;
                if digits[i] == 9 {
                    digits[i] = 0;
                } else {
                    digits[i] += 1;
                    break;
                }
            }
        } else if digits.is_empty() {
            return Decimal { digits: vec![0], exp: 0 };
        }
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        Decimal { digits, exp }
    }

    /// Fixed-point digits with `places` after the point: (integer part, fraction).
    fn fixed(&self, places: usize) -> (String, String) {
        let digit = |i: i64| -> char {
            if i < 0 { '0' } else { self.digits.get(i as usize).map_or('0', |&d| (b'0' + d) as char) }
        };
        // digits[i] has place value 10^(exp - i).
        let exp = i64::from(if self.is_zero() { 0 } else { self.exp });
        let int: String = if exp < 0 { "0".into() } else { (0..=exp).map(digit).collect() };
        let frac: String = (1..=places as i64).map(|k| digit(exp + k)).collect();
        (int, frac)
    }
}

/// The decimal exponent of `|x|`'s leading digit (`floor(log10 |x|)`, 0 for zero). The
/// shortest round-trip form has the
/// same exponent as the exact value: it lies in the value's rounding interval, which a
/// power of ten splits only when the power itself is in it, and then it is that power.
pub fn exponent_of(x: f64) -> i32 {
    let s = format!("{:e}", x.abs());
    s.split_once('e').and_then(|(_, e)| e.parse().ok()).unwrap_or(0)
}

/// `x` rounded to `places` decimals, as (integer digits, fraction digits) of its
/// magnitude, ties away from zero on the exact value.
fn to_fixed(x: f64, places: usize) -> (String, String) {
    let n = exponent_of(x) + 1 + places as i32;
    Decimal::round_significant(x, n).fixed(places)
}

// --- numbers ---------------------------------------------------------------------------

/// A parsed number format: `[sign][$][,][.precision][~][type]` (`docs/spec/format.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberFormat {
    sign: Sign,
    currency: bool,
    group: bool,
    precision: Option<usize>,
    trim: bool,
    kind: Kind,
    /// For `s` and `k` on an axis: every value in this tier (thousands = 1), with
    /// `precision` decimals, as d3's `formatPrefix` writes ticks.
    tier: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sign {
    /// A minus sign for negatives.
    Minus,
    /// A plus sign for zero and positives, too.
    Plus,
    /// Parentheses around negatives.
    Parens,
    /// A space for zero and positives.
    Space,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fixed,
    Integer,
    Percent,
    Exponent,
    General,
    Rounded,
    Si,
    Compact,
    None,
}

impl NumberFormat {
    /// No format string: d3's default (12 significant digits, trailing zeros trimmed).
    pub fn plain() -> NumberFormat {
        NumberFormat {
            sign: Sign::Minus,
            currency: false,
            group: false,
            precision: None,
            trim: true,
            kind: Kind::None,
            tier: None,
        }
    }

    /// `places` decimals, no grouping (`.{places}f`).
    pub fn fixed(places: usize) -> NumberFormat {
        NumberFormat { precision: Some(places), trim: false, kind: Kind::Fixed, ..NumberFormat::plain() }
    }

    /// Every character besides the digits that this format can print in `locale`: what a
    /// counting label must have glyphs for.
    pub fn alphabet(&self, locale: &Locale) -> String {
        let mut s = String::from(locale.decimal);
        s.push(MINUS);
        if self.group {
            s.push_str(locale.group);
        }
        if self.currency {
            s.push_str(locale.currency.0);
            s.push_str(locale.currency.1);
        }
        match self.sign {
            Sign::Plus => s.push('+'),
            Sign::Parens => s.push_str("()"),
            Sign::Space => s.push(' '),
            Sign::Minus => {}
        }
        match self.kind {
            Kind::Percent => s.push_str(locale.percent),
            Kind::Exponent | Kind::General | Kind::None => s.push_str("e+-"),
            Kind::Si => SI.iter().for_each(|p| s.push_str(p)),
            Kind::Compact => locale.compact.iter().for_each(|p| s.push_str(p)),
            Kind::Fixed | Kind::Integer | Kind::Rounded => {}
        }
        let mut seen = std::collections::BTreeSet::new();
        s.chars().filter(|c| !c.is_ascii_digit() && seen.insert(*c)).collect()
    }

    /// This format for the ticks of an axis whose ticks are `step` apart and reach
    /// `max` in size, as d3's `tickFormat` makes it. A format with no precision takes
    /// the precision the step needs: decimals for `f` and `%`, significant digits for
    /// the others. An `f` or `%` format's own precision is the most its ticks take, so
    /// ticks a whole step apart drop the decimals the values keep (`$10`, not `$10.0`).
    /// `s` and `k` write every tick in the tier of `max` (`0k`, `20k`, `40k`). With no
    /// format at all ([`NumberFormat::ticks`]), ticks group thousands.
    pub fn for_ticks(&self, step: f64, max: f64) -> NumberFormat {
        let mut f = self.clone();
        let step_exp = exponent_of(step);
        match f.kind {
            Kind::Si | Kind::Compact => {
                let exp = exponent_of(max);
                let tier = match f.kind {
                    Kind::Si => exp.div_euclid(3).clamp(-8, 8),
                    _ => exp.div_euclid(3).clamp(0, 4),
                };
                f.tier = Some(tier);
                if f.precision.is_none() {
                    f.precision = Some((3 * tier - step_exp).max(0) as usize);
                }
            }
            Kind::Fixed | Kind::Percent => {
                let shift = if f.kind == Kind::Percent { 2 } else { 0 };
                let needs = (-step_exp - shift).max(0) as usize;
                f.precision = Some(f.precision.map_or(needs, |p| p.min(needs)));
            }
            Kind::None | Kind::Exponent | Kind::General | Kind::Rounded if f.precision.is_none() => {
                let round = (exponent_of(max) - step_exp).max(0) as usize + 1;
                f.precision = Some(if f.kind == Kind::Exponent { round.saturating_sub(1) } else { round });
            }
            _ => {}
        }
        f
    }

    /// d3's default for ticks with no format (`,f`), at the step's precision.
    pub fn ticks(step: f64) -> NumberFormat {
        NumberFormat { group: true, ..NumberFormat::fixed(0) }.for_ticks_fixed(step)
    }

    fn for_ticks_fixed(mut self, step: f64) -> NumberFormat {
        self.precision = Some((-exponent_of(step)).max(0) as usize);
        self
    }

    pub fn parse(spec: &str) -> Result<NumberFormat, FormatError> {
        let mut rest = spec;
        let mut take = |c: char| -> bool {
            match rest.strip_prefix(c) {
                Some(r) => {
                    rest = r;
                    true
                }
                None => false,
            }
        };
        let sign = if take('+') {
            Sign::Plus
        } else if take('(') {
            Sign::Parens
        } else if take(' ') {
            Sign::Space
        } else {
            take('-');
            Sign::Minus
        };
        let currency = take('$');
        let group = take(',');
        let precision = match rest.strip_prefix('.') {
            Some(r) => {
                let end = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
                let digits = &r[..end];
                if digits.is_empty() {
                    return Err(bad(format!("`{spec}`: a `.` needs a precision after it")));
                }
                rest = &r[end..];
                Some(digits.parse::<usize>().map_err(|_| bad(format!("`{spec}`: precision too large")))?.min(100))
            }
            None => None,
        };
        let trim = match rest.strip_prefix('~') {
            Some(r) => {
                rest = r;
                true
            }
            None => false,
        };
        let kind = match rest {
            "f" => Kind::Fixed,
            "d" => Kind::Integer,
            "%" => Kind::Percent,
            "e" => Kind::Exponent,
            "g" => Kind::General,
            "r" => Kind::Rounded,
            "s" => Kind::Si,
            "k" => Kind::Compact,
            "" => Kind::None,
            other => {
                return Err(bad(format!(
                    "`{spec}`: `{other}` is not a number type; expected f, d, %, e, g, r, s, k, or none \
                     (the format is [sign][$][,][.precision][~][type])"
                )));
            }
        };
        Ok(NumberFormat { sign, currency, group, precision, trim: trim || kind == Kind::None, kind, tier: None })
    }

    /// `x` in this format and `locale`.
    pub fn format(&self, x: f64, locale: &Locale) -> String {
        if x.is_nan() {
            return "NaN".into();
        }
        let negative = x.is_sign_negative();
        let precision = self.precision.unwrap_or(if self.kind == Kind::None { 12 } else { 6 });
        let (mut body, suffix) =
            if x.is_infinite() { ("∞".to_string(), String::new()) } else { self.body(x, precision, locale) };
        if self.trim {
            body = trim_zeros(&body, locale.decimal);
        }
        // A negative value that rounds to zero shows no sign, unless `+` asks for one.
        let zero = body.chars().all(|c| !c.is_ascii_digit() || c == '0');
        let negative = negative && !(zero && self.sign != Sign::Plus);
        let (pre, post) = if self.currency { locale.currency } else { ("", "") };
        let number = format!("{pre}{body}{suffix}{post}");
        match (self.sign, negative) {
            (Sign::Parens, true) => format!("({number})"),
            (_, true) => format!("{MINUS}{number}"),
            (Sign::Plus, false) => format!("+{number}"),
            (Sign::Space, false) => format!(" {number}"),
            _ => number,
        }
    }

    /// The digits of `|x|` (grouped, with the locale's decimal) and what follows them.
    fn body(&self, x: f64, p: usize, locale: &Locale) -> (String, String) {
        let fixed = |x: f64, places: usize| {
            let (int, frac) = to_fixed(x, places);
            self.join(&int, &frac, locale)
        };
        let significant = |x: f64, p: usize| Decimal::round_significant(x, p.max(1) as i32);
        match self.kind {
            Kind::Fixed => (fixed(x, p), String::new()),
            Kind::Integer => (fixed(x, 0), String::new()),
            Kind::Percent => (fixed(x * 100.0, p), locale.percent.into()),
            Kind::Exponent => {
                let d = Decimal::round_significant(x, p as i32 + 1);
                (self.exponent(&d, p, locale), String::new())
            }
            Kind::Rounded => {
                let d = significant(x, p);
                (self.join_decimal(&d, p, locale), String::new())
            }
            Kind::General | Kind::None => {
                let d = significant(x, p);
                let exp = if d.is_zero() { 0 } else { d.exp };
                if exp < -6 || exp >= p as i32 {
                    (self.exponent(&d, p.max(1) - 1, locale), String::new())
                } else {
                    (self.join_decimal(&d, p, locale), String::new())
                }
            }
            Kind::Si | Kind::Compact if self.tier.is_some() => {
                let tier = self.tier.unwrap_or_default();
                let suffix = match self.kind {
                    Kind::Si => SI[(tier + 8) as usize].to_string(),
                    _ if tier == 0 => String::new(),
                    _ => locale.compact[tier as usize - 1].to_string(),
                };
                // x / 1000^tier at `p` decimals: round at `p + 3·tier` decimals of x,
                // exactly, then move the point.
                let n = exponent_of(x) + 1 + p as i32 + 3 * tier;
                let d = Decimal::round_significant(x, n);
                let scaled = Decimal { exp: d.exp - 3 * tier, ..d };
                let (int, frac) = scaled.fixed(p);
                (self.join(&int, &frac, locale), suffix)
            }
            Kind::Si | Kind::Compact => {
                let d = significant(x, p);
                let exp = if d.is_zero() { 0 } else { d.exp };
                let (tier, suffix) = match self.kind {
                    Kind::Si => {
                        let tier = (exp.div_euclid(3)).clamp(-8, 8);
                        (tier, SI[(tier + 8) as usize].to_string())
                    }
                    _ => {
                        let tier = (exp.div_euclid(3)).clamp(0, 4);
                        (tier, if tier == 0 { String::new() } else { locale.compact[tier as usize - 1].to_string() })
                    }
                };
                let scaled = Decimal { digits: d.digits.clone(), exp: d.exp - 3 * tier };
                (self.join_decimal(&scaled, p, locale), suffix)
            }
        }
    }

    /// `d` (already rounded to `significant` digits) in fixed notation, showing those
    /// significant digits.
    fn join_decimal(&self, d: &Decimal, significant: usize, locale: &Locale) -> String {
        let exp = if d.is_zero() { 0 } else { d.exp };
        let places = (significant as i32 - 1 - exp).max(0) as usize;
        let (int, frac) = d.fixed(places);
        self.join(&int, &frac, locale)
    }

    fn exponent(&self, d: &Decimal, places: usize, locale: &Locale) -> String {
        let exp = if d.is_zero() { 0 } else { d.exp };
        let mantissa = Decimal { digits: d.digits.clone(), exp: 0 };
        let (int, frac) = mantissa.fixed(places);
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{}", self.join(&int, &frac, locale), exp.unsigned_abs())
    }

    fn join(&self, int: &str, frac: &str, locale: &Locale) -> String {
        let int = if self.group { group(int, locale.group) } else { int.to_string() };
        if frac.is_empty() { int } else { format!("{int}{}{frac}", locale.decimal) }
    }
}

/// SI prefixes from 10^-24 to 10^24.
const SI: [&str; 17] = ["y", "z", "a", "f", "p", "n", "µ", "m", "", "k", "M", "G", "T", "P", "E", "Z", "Y"];

/// `digits` with `sep` between each group of three, from the right.
fn group(digits: &str, sep: &str) -> String {
    let n = digits.len();
    let mut out = String::with_capacity(n + n / 3 * sep.len());
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) {
            out.push_str(sep);
        }
        out.push(c);
    }
    out
}

/// Drop zeros after the decimal point that change nothing, and the point if nothing
/// follows it; an exponent stays.
fn trim_zeros(body: &str, decimal: &str) -> String {
    let (number, exp) = match body.find('e') {
        Some(i) => body.split_at(i),
        None => (body, ""),
    };
    let Some(point) = number.find(decimal) else { return body.to_string() };
    let trimmed = number.trim_end_matches('0');
    let trimmed = if trimmed.len() == point + decimal.len() { &number[..point] } else { trimmed };
    format!("{trimmed}{exp}")
}

/// `x` with no format string ([`NumberFormat::plain`]) in `locale`.
pub fn format_number(x: f64, locale: &Locale) -> String {
    NumberFormat::plain().format(x, locale)
}

// --- dates -----------------------------------------------------------------------------

/// A civil date and time, in seconds since 1970-01-01T00:00:00. It has no time zone:
/// 2025-03-01T09:00 is nine o'clock wherever the deck plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTime(pub i64);

/// A date and time's parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    /// 1–12.
    pub month: u32,
    /// 1–31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

const DAY: i64 = 86_400;

/// Days from 1970-01-01 to `y-m-d` (proleptic Gregorian; Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        2 if leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl DateTime {
    /// `None` if any part is out of range (a 31st of April, a 25th hour).
    pub fn from_civil(c: Civil) -> Option<DateTime> {
        let ok = (1..=12).contains(&c.month)
            && (1..=days_in_month(c.year, c.month)).contains(&c.day)
            && c.hour < 24
            && c.minute < 60
            && c.second < 60;
        ok.then(|| {
            DateTime(
                days_from_civil(c.year, c.month, c.day) * DAY
                    + i64::from(c.hour) * 3600
                    + i64::from(c.minute) * 60
                    + i64::from(c.second),
            )
        })
    }

    pub fn ymd(year: i64, month: u32, day: u32) -> Option<DateTime> {
        DateTime::from_civil(Civil { year, month, day, hour: 0, minute: 0, second: 0 })
    }

    pub fn civil(self) -> Civil {
        let (days, secs) = (self.0.div_euclid(DAY), self.0.rem_euclid(DAY));
        let (year, month, day) = civil_from_days(days);
        Civil {
            year,
            month,
            day,
            hour: (secs / 3600) as u32,
            minute: (secs / 60 % 60) as u32,
            second: (secs % 60) as u32,
        }
    }

    /// 0 for Sunday.
    pub fn weekday(self) -> u32 {
        (self.0.div_euclid(DAY) + 4).rem_euclid(7) as u32
    }

    /// 1 for January 1st.
    pub fn day_of_year(self) -> u32 {
        let c = self.civil();
        (self.0.div_euclid(DAY) - days_from_civil(c.year, 1, 1) + 1) as u32
    }
}

/// A parsed date format: strftime directives (`%Y`, `%b`, …) among literal text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateFormat(Vec<Piece>);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Field(Field, Pad),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    ShortWeekday,
    Weekday,
    ShortMonth,
    Month,
    Day,
    Hour24,
    Hour12,
    Period,
    Minute,
    Second,
    MonthNumber,
    Quarter,
    ShortYear,
    Year,
    DayOfYear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pad {
    Zero,
    Space,
    None,
}

impl Field {
    /// Digits a number field pads to.
    fn width(self) -> usize {
        match self {
            Field::DayOfYear => 3,
            Field::Year => 4,
            Field::Quarter => 1,
            _ => 2,
        }
    }
}

impl DateFormat {
    /// The ISO 8601 forms a date column reads with no `parse` format, longest first.
    pub fn iso() -> [DateFormat; 5] {
        ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d", "%Y-%m", "%Y"]
            .map(|f| DateFormat::parse(f).expect("ISO formats parse"))
    }

    /// Whether the format reads a date's month, quarter, day, or weekday but not its
    /// year, so that every date it reads falls in 1900. A time of day alone reads no
    /// date, and is not.
    pub fn misses_year(&self) -> bool {
        let reads = |f: &dyn Fn(Field) -> bool| self.0.iter().any(|p| matches!(p, Piece::Field(x, _) if f(*x)));
        let calendar = |f: Field| {
            use Field::*;
            matches!(f, ShortWeekday | Weekday | ShortMonth | Month | Day | MonthNumber | Quarter | DayOfYear)
        };
        reads(&calendar) && !reads(&|f| matches!(f, Field::Year | Field::ShortYear))
    }

    pub fn parse(spec: &str) -> Result<DateFormat, FormatError> {
        let mut pieces = Vec::new();
        let mut text = String::new();
        let mut chars = spec.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                text.push(c);
                continue;
            }
            let mut d = chars.next().ok_or_else(|| bad(format!("`{spec}` ends with a lone `%`")))?;
            let pad = match d {
                '-' | '_' | '0' => {
                    let pad = match d {
                        '-' => Pad::None,
                        '_' => Pad::Space,
                        _ => Pad::Zero,
                    };
                    d = chars.next().ok_or_else(|| bad(format!("`{spec}` ends inside a directive")))?;
                    Some(pad)
                }
                _ => None,
            };
            let field = match d {
                '%' => {
                    text.push('%');
                    continue;
                }
                'a' => Field::ShortWeekday,
                'A' => Field::Weekday,
                'b' => Field::ShortMonth,
                'B' => Field::Month,
                'd' | 'e' => Field::Day,
                'H' => Field::Hour24,
                'I' => Field::Hour12,
                'p' => Field::Period,
                'M' => Field::Minute,
                'S' => Field::Second,
                'm' => Field::MonthNumber,
                'q' => Field::Quarter,
                'y' => Field::ShortYear,
                'Y' => Field::Year,
                'j' => Field::DayOfYear,
                other => {
                    return Err(bad(format!(
                        "`{spec}`: `%{other}` is not a date directive; expected one of %a %A %b %B %d %e %H %I %p %M %S %m %q %y %Y %j %%"
                    )));
                }
            };
            let pad = pad.unwrap_or(if d == 'e' { Pad::Space } else { Pad::Zero });
            if !text.is_empty() {
                pieces.push(Piece::Text(std::mem::take(&mut text)));
            }
            pieces.push(Piece::Field(field, pad));
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }
        Ok(DateFormat(pieces))
    }

    pub fn format(&self, t: DateTime, locale: &Locale) -> String {
        let c = t.civil();
        let mut out = String::new();
        for piece in &self.0 {
            let (field, pad) = match piece {
                Piece::Text(s) => {
                    out.push_str(s);
                    continue;
                }
                Piece::Field(f, p) => (*f, *p),
            };
            let number = |n: i64| -> String {
                let s = n.unsigned_abs().to_string();
                let w = field.width();
                let padded = match pad {
                    Pad::Zero if s.len() < w => format!("{}{s}", "0".repeat(w - s.len())),
                    Pad::Space if s.len() < w => format!("{}{s}", " ".repeat(w - s.len())),
                    _ => s,
                };
                if n < 0 { format!("{MINUS}{padded}") } else { padded }
            };
            match field {
                Field::ShortWeekday => out.push_str(locale.short_days[t.weekday() as usize]),
                Field::Weekday => out.push_str(locale.days[t.weekday() as usize]),
                Field::ShortMonth => out.push_str(locale.short_months[c.month as usize - 1]),
                Field::Month => out.push_str(locale.months[c.month as usize - 1]),
                Field::Day => out.push_str(&number(i64::from(c.day))),
                Field::Hour24 => out.push_str(&number(i64::from(c.hour))),
                Field::Hour12 => out.push_str(&number(i64::from((c.hour + 11) % 12 + 1))),
                Field::Period => out.push_str(locale.periods[usize::from(c.hour >= 12)]),
                Field::Minute => out.push_str(&number(i64::from(c.minute))),
                Field::Second => out.push_str(&number(i64::from(c.second))),
                Field::MonthNumber => out.push_str(&number(i64::from(c.month))),
                Field::Quarter => out.push_str(&number(i64::from((c.month - 1) / 3 + 1))),
                Field::ShortYear => out.push_str(&number(c.year.rem_euclid(100))),
                Field::Year => out.push_str(&number(c.year)),
                Field::DayOfYear => out.push_str(&number(i64::from(t.day_of_year()))),
            }
        }
        out
    }

    /// Read `text` in this format. Fields it does not name default to 1900-01-01 at
    /// midnight, as d3's do; `%y` reads 00–68 as 2000–2068 and 69–99 as 1969–1999.
    pub fn read(&self, text: &str, locale: &Locale) -> Result<DateTime, FormatError> {
        let fail = |why: &str| bad(format!("`{text}` does not read as a date here: {why}"));
        let mut c = Civil { year: 1900, month: 1, day: 1, hour: 0, minute: 0, second: 0 };
        let (mut hour12, mut pm, mut day_of_year) = (None, None, None);
        let mut rest = text.trim();
        for piece in &self.0 {
            let field = match piece {
                Piece::Text(s) => {
                    rest = rest.strip_prefix(s.as_str()).ok_or_else(|| fail(&format!("expected `{s}`")))?;
                    continue;
                }
                Piece::Field(field, _) => *field,
            };
            let mut number =
                |max_digits: usize| read_number(&mut rest, max_digits).ok_or_else(|| fail("expected a number"));
            match field {
                Field::Day => c.day = number(2)? as u32,
                Field::Hour24 => c.hour = number(2)? as u32,
                Field::Hour12 => hour12 = Some(number(2)? as u32),
                Field::Minute => c.minute = number(2)? as u32,
                Field::Second => c.second = number(2)? as u32,
                Field::MonthNumber => c.month = number(2)? as u32,
                Field::Quarter => {
                    let q = number(1)?;
                    if !(1..=4).contains(&q) {
                        return Err(fail("a quarter is 1 to 4"));
                    }
                    c.month = 3 * (q as u32 - 1) + 1;
                }
                Field::ShortYear => {
                    let y = number(2)?;
                    c.year = y + if y > 68 { 1900 } else { 2000 };
                }
                Field::Year => c.year = number(4)?,
                Field::DayOfYear => day_of_year = Some(number(3)?),
                Field::ShortWeekday | Field::Weekday | Field::ShortMonth | Field::Month | Field::Period => {
                    let (list, what): (&[&str], &str) = match field {
                        Field::Weekday => (&locale.days, "a weekday"),
                        Field::ShortWeekday => (&locale.short_days, "a weekday"),
                        Field::Month => (&locale.months, "a month name"),
                        Field::ShortMonth => (&locale.short_months, "a month name"),
                        _ => (&locale.periods, "AM or PM"),
                    };
                    let (i, len) = read_name(rest, list).ok_or_else(|| fail(&format!("expected {what}")))?;
                    rest = &rest[len..];
                    match field {
                        Field::Month | Field::ShortMonth => c.month = i as u32 + 1,
                        Field::Period => pm = Some(i == 1),
                        _ => {}
                    }
                }
            }
        }
        if !rest.is_empty() {
            return Err(fail(&format!("`{rest}` is left over")));
        }
        if let Some(h) = hour12 {
            if !(1..=12).contains(&h) {
                return Err(fail("a 12-hour clock runs 1 to 12"));
            }
            c.hour = h % 12 + if pm == Some(true) { 12 } else { 0 };
        } else if pm == Some(true) && c.hour < 12 {
            c.hour += 12;
        }
        if let Some(j) = day_of_year {
            let start = DateTime::ymd(c.year, 1, 1).ok_or_else(|| fail("bad year"))?;
            let days = if leap(c.year) { 366 } else { 365 };
            if !(1..=days).contains(&j) {
                return Err(fail("day of the year out of range"));
            }
            let d = DateTime(start.0 + (j - 1) * DAY).civil();
            (c.month, c.day) = (d.month, d.day);
        }
        DateTime::from_civil(c).ok_or_else(|| fail("no such date"))
    }
}

/// Up to `max_digits` digits at the start of `rest` (after spaces, which `%e` pads
/// with), consumed.
fn read_number(rest: &mut &str, max_digits: usize) -> Option<i64> {
    let r = rest.trim_start_matches(' ');
    let end = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len()).min(max_digits);
    let n = r.get(..end).filter(|d| !d.is_empty())?.parse().ok()?;
    *rest = &r[end..];
    Some(n)
}

/// The longest name in `list` that starts `rest`, ignoring case: its index and length
/// in bytes.
fn read_name(rest: &str, list: &[&str]) -> Option<(usize, usize)> {
    list.iter()
        .enumerate()
        .filter(|(_, name)| rest.get(..name.len()).is_some_and(|head| head.to_lowercase() == name.to_lowercase()))
        .max_by_key(|(_, name)| name.len())
        .map(|(i, name)| (i, name.len()))
}

/// Read a date in the ISO 8601 forms [`DateFormat::iso`] lists.
pub fn read_iso(text: &str) -> Result<DateTime, FormatError> {
    let text = text.trim().strip_suffix('Z').unwrap_or(text.trim());
    let locale = Locale::of(None);
    DateFormat::iso().iter().find_map(|f| f.read(text, locale).ok()).ok_or_else(|| {
        bad(format!("`{text}` is not an ISO 8601 date (YYYY, YYYY-MM, YYYY-MM-DD, or with Thh:mm[:ss])"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(spec: &str, x: f64) -> String {
        NumberFormat::parse(spec).unwrap().format(x, Locale::of(None))
    }

    #[test]
    fn fixed_point_rounds_ties_away_from_zero_on_the_exact_value() {
        assert_eq!(n(".2f", 1.005), "1.00", "1.005 is 1.00499999999999989… in binary");
        assert_eq!(n(".1f", 0.25), "0.3");
        assert_eq!(n(".1f", 0.35), "0.3", "0.35 is 0.34999999999999997… in binary");
        assert_eq!(n(".0f", 2.5), "3");
        assert_eq!(n(".0f", -2.5), "\u{2212}3");
        assert_eq!(n(".2f", 1234.5), "1234.50");
        assert_eq!(n(".3f", 0.0), "0.000");
        assert_eq!(n("f", 1.5), "1.500000", "d3's default precision is 6");
        assert_eq!(n(".1f", 9.96), "10.0");
        assert_eq!(n(".2f", 0.004), "0.00");
        assert_eq!(n(".2f", 0.005), "0.01", "0.005 is 0.005000000000000000104… in binary");
    }

    #[test]
    fn signs_currency_and_groups_follow_d3() {
        assert_eq!(n(",.0f", 1234567.89), "1,234,568");
        assert_eq!(n("$,.2f", -1234.5), "\u{2212}$1,234.50");
        assert_eq!(n("($,.2f", -1234.5), "($1,234.50)");
        assert_eq!(n("($,.2f", 1234.5), "$1,234.50");
        assert_eq!(n("+.1%", 0.123), "+12.3%");
        assert_eq!(n("+.1f", 0.0), "+0.0");
        assert_eq!(n(" .1f", 2.0), " 2.0");
        assert_eq!(n(".1f", -0.04), "0.0", "a negative that rounds to zero loses its sign");
        assert_eq!(n("+.1f", -0.04), "\u{2212}0.0", "unless + asks for signs");
        assert_eq!(n(",d", 1234.5), "1,235");
        assert_eq!(n(",", 1234567.0), "1,234,567");
        assert_eq!(n(",.2~f", 1500.0), "1,500");
        assert_eq!(n(".2~%", 0.125), "12.5%");
    }

    #[test]
    fn significant_digits_exponents_and_prefixes() {
        assert_eq!(n(".3r", 1234.5), "1230");
        assert_eq!(n(".3r", 0.012345), "0.0123");
        assert_eq!(n(".1e", 12345.0), "1.2e+4");
        assert_eq!(n(".2e", 0.000123), "1.23e-4");
        assert_eq!(n(".3g", 0.000123456), "0.000123");
        assert_eq!(n(".3g", 1234567.0), "1.23e+6");
        assert_eq!(n(".2s", 1234.0), "1.2k");
        assert_eq!(n("~s", 1500.0), "1.5k");
        assert_eq!(n(".3s", 999_950.0), "1.00M", "rounding carries into the next prefix");
        assert_eq!(n(".2s", 0.0012), "1.2m");
        assert_eq!(n("$.2~k", 1_234_000_000.0), "$1.2B");
        assert_eq!(n(".3k", 999.0), "999");
        assert_eq!(n(".2k", 2_500_000_000_000_000.0), "2500T");
        assert_eq!(n("", 0.1 + 0.2), "0.3");
        assert_eq!(n("", 1234567.0), "1234567");
        assert_eq!(n("", 1e21), "1e+21");
        assert_eq!(n("", 24.0), "24");
    }

    #[test]
    fn locales_set_separators_currency_and_compact_suffixes() {
        let de = Locale::of(Some("de"));
        assert_eq!(de.tag, "de-DE");
        let f = NumberFormat::parse("$,.2f").unwrap();
        assert_eq!(f.format(1234.5, de), "1.234,50\u{a0}€");
        assert_eq!(NumberFormat::parse(".2~k").unwrap().format(2_500_000.0, de), "2,5\u{a0}Mio.");
        assert_eq!(NumberFormat::parse(".1~k").unwrap().format(2_500_000.0, de), "3\u{a0}Mio.", "one digit, a tie up");
        let fr = Locale::of(Some("fr-CA"));
        assert_eq!(fr.tag, "fr-FR", "a language's first entry");
        assert_eq!(NumberFormat::parse(",.1%").unwrap().format(0.1234, fr), "12,3\u{a0}%");
        assert_eq!(Locale::of(Some("ja-JP")).tag, "en-US", "unknown languages format as en-US");
        assert_eq!(Locale::of(Some("en_gb")).tag, "en-GB");
    }

    #[test]
    fn a_formats_alphabet_is_every_character_it_can_print() {
        let en = Locale::of(None);
        assert_eq!(NumberFormat::plain().alphabet(en), ".\u{2212}e+-");
        assert_eq!(NumberFormat::parse("($,.1~k").unwrap().alphabet(en), ".\u{2212},$()KMBT");
        assert_eq!(NumberFormat::parse(".0%").unwrap().alphabet(Locale::of(Some("fr"))), ",\u{2212}\u{a0}%");
        assert_eq!(NumberFormat::fixed(2).format(-0.004, en), "0.00");
    }

    #[test]
    fn tick_formats_take_the_precision_their_step_needs() {
        let en = Locale::of(None);
        let ticks = |f: &NumberFormat, values: &[f64]| values.iter().map(|&v| f.format(v, en)).collect::<Vec<_>>();
        assert_eq!(ticks(&NumberFormat::ticks(20.0), &[0.0, 20.0, 1000.0]), ["0", "20", "1,000"]);
        assert_eq!(ticks(&NumberFormat::ticks(0.5), &[0.0, 0.5, 1.0]), ["0.0", "0.5", "1.0"]);
        let money = NumberFormat::parse("$,f").unwrap().for_ticks(0.05, 1.0);
        assert_eq!(ticks(&money, &[0.0, 0.75]), ["$0.00", "$0.75"]);
        let share = NumberFormat::parse("%").unwrap().for_ticks(0.05, 0.2);
        assert_eq!(ticks(&share, &[0.0, 0.05, 0.2]), ["0%", "5%", "20%"]);
        // One prefix for every tick, from the largest.
        let si = NumberFormat::parse("s").unwrap().for_ticks(20_000.0, 100_000.0);
        assert_eq!(ticks(&si, &[0.0, 20_000.0, 100_000.0]), ["0k", "20k", "100k"]);
        let compact = NumberFormat::parse("$k").unwrap().for_ticks(500_000_000.0, 2_500_000_000.0);
        assert_eq!(ticks(&compact, &[0.0, 500_000_000.0, 2_500_000_000.0]), ["$0.0B", "$0.5B", "$2.5B"]);
        // An explicit precision is the most ticks take: a whole step apart, none.
        let fixed = NumberFormat::parse(".2f").unwrap().for_ticks(10.0, 50.0);
        assert_eq!(ticks(&fixed, &[10.0]), ["10"]);
        let tenths = NumberFormat::parse("$,.2f").unwrap().for_ticks(0.1, 0.5);
        assert_eq!(ticks(&tenths, &[0.0, 0.1]), ["$0.0", "$0.1"]);
        let capped = NumberFormat::parse(".1%").unwrap().for_ticks(0.0005, 0.002);
        assert_eq!(ticks(&capped, &[0.0005]), ["0.1%"]);
        let plain = NumberFormat::plain().for_ticks(0.1, 0.3);
        assert_eq!(ticks(&plain, &[0.1 + 0.2]), ["0.3"]);
    }

    #[test]
    fn bad_formats_say_what_they_expected() {
        for (spec, says) in [(".f", "precision"), ("x", "not a number type"), (",.2fx", "not a number type")] {
            let err = NumberFormat::parse(spec).unwrap_err().to_string();
            assert!(err.contains(says), "{spec}: {err}");
        }
        assert!(DateFormat::parse("%Q").unwrap_err().to_string().contains("not a date directive"));
        assert!(DateFormat::parse("%").is_err());
    }

    #[test]
    fn civil_dates_round_trip_through_seconds() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (1969, 12, 31), (2025, 3, 1), (1600, 3, 1), (2400, 12, 31)] {
            let t = DateTime::ymd(y, m, d).unwrap();
            let c = t.civil();
            assert_eq!((c.year, c.month, c.day), (y, m, d));
        }
        assert_eq!(DateTime::ymd(1970, 1, 1).unwrap().0, 0);
        assert_eq!(DateTime::ymd(1970, 1, 1).unwrap().weekday(), 4, "a Thursday");
        assert_eq!(DateTime::ymd(2025, 3, 1).unwrap().day_of_year(), 60);
        assert!(DateTime::ymd(2025, 2, 29).is_none());
        assert!(DateTime::ymd(2025, 4, 31).is_none());
    }

    #[test]
    fn dates_format_and_read_back() {
        let en = Locale::of(None);
        let t = DateTime::from_civil(Civil { year: 2025, month: 3, day: 5, hour: 14, minute: 7, second: 9 }).unwrap();
        let f = |spec: &str| DateFormat::parse(spec).unwrap().format(t, en);
        assert_eq!(f("%Y-%m-%d"), "2025-03-05");
        assert_eq!(f("%b %-d, %Y"), "Mar 5, 2025");
        assert_eq!(f("%B %e"), "March  5");
        assert_eq!(f("%a %A"), "Wed Wednesday");
        assert_eq!(f("Q%q %y"), "Q1 25");
        assert_eq!(f("%I:%M %p"), "02:07 PM");
        assert_eq!(f("%H:%M:%S"), "14:07:09");
        assert_eq!(f("%j"), "064");
        assert_eq!(f("100%%"), "100%");
        assert_eq!(DateFormat::parse("%B").unwrap().format(t, Locale::of(Some("de"))), "März");

        let read = |spec: &str, text: &str| DateFormat::parse(spec).unwrap().read(text, en);
        assert_eq!(read("%b %-d, %Y", "mar 5, 2025").unwrap(), DateTime::ymd(2025, 3, 5).unwrap());
        assert_eq!(read("Q%q %Y", "Q3 2025").unwrap(), DateTime::ymd(2025, 7, 1).unwrap());
        assert_eq!(read("%y", "69").unwrap(), DateTime::ymd(1969, 1, 1).unwrap());
        assert_eq!(read("%y", "68").unwrap(), DateTime::ymd(2068, 1, 1).unwrap());
        assert_eq!(read("%I %p", "12 AM").unwrap(), DateTime::ymd(1900, 1, 1).unwrap());
        assert_eq!(read("%I %p", "12 PM").unwrap().civil().hour, 12);
        assert!(read("%Y-%m-%d", "2025-02-30").unwrap_err().to_string().contains("no such date"));
        assert!(read("%Y", "2025x").unwrap_err().to_string().contains("left over"));
        assert_eq!(read_iso("2025-03").unwrap(), DateTime::ymd(2025, 3, 1).unwrap());
        assert_eq!(read_iso("2025-03-05T14:07:09Z").unwrap(), t);
        assert!(read_iso("March 2025").is_err());
    }
}

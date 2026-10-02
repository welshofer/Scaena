//! Expressions over a data row (`docs/spec/expr.md`): what a chart's `dataTransform`
//! filters and derives with. A small, typed, JavaScript-like language: an expression is
//! parsed once, checked against the table's column types, bound to its columns, and
//! evaluated per row with plain IEEE arithmetic, so every platform gets the same bits
//! (SPEC §13).

use crate::data::{ColumnType, Datum};
use std::fmt;

/// A value's type, as the checker sees it. `Null` is the literal `null`, which goes
/// with any type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Number,
    Text,
    Bool,
    Date,
    Null,
}

impl Type {
    pub fn of(column: ColumnType) -> Type {
        match column {
            ColumnType::Number => Type::Number,
            ColumnType::String => Type::Text,
            ColumnType::Boolean => Type::Bool,
            ColumnType::Date => Type::Date,
        }
    }

    /// The column type a value of this type is stored as; `null` alone is text.
    pub fn column(self) -> ColumnType {
        match self {
            Type::Number => ColumnType::Number,
            Type::Text | Type::Null => ColumnType::String,
            Type::Bool => ColumnType::Boolean,
            Type::Date => ColumnType::Date,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Type::Number => "a number",
            Type::Text => "text",
            Type::Bool => "true or false",
            Type::Date => "a date",
            Type::Null => "null",
        }
    }

    /// The type both `self` and `other` can be: the same type, or either with `null`.
    fn with(self, other: Type) -> Option<Type> {
        match (self, other) {
            (a, b) if a == b => Some(a),
            (Type::Null, t) | (t, Type::Null) => Some(t),
            _ => None,
        }
    }
}

/// Why an expression did not parse or check: where in its text (in characters), and
/// what went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub at: usize,
    pub message: String,
    /// It parsed, but reads a column the table does not have or uses one as the wrong
    /// type (E103, not E106).
    pub data: bool,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at {}: {}", self.at, self.message)
    }
}

impl std::error::Error for ExprError {}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Text(String),
    Name(String),
    /// A field in backticks: any column name.
    Field(String),
    Op(&'static str),
}

/// The source as tokens, each with where it starts.
fn tokens(src: &str) -> Result<Vec<(usize, Token)>, ExprError> {
    let chars: Vec<char> = src.chars().collect();
    let err = |at: usize, message: String| ExprError { at, message, data: false };
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let mut j = i + 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    i = j;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text: String = chars[start..i].iter().collect();
            let n = text.parse::<f64>().map_err(|_| err(start, format!("`{text}` is not a number")))?;
            out.push((start, Token::Number(n)));
            continue;
        }
        if c == '\'' || c == '"' {
            let mut text = String::new();
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err(err(start, "text that never closes its quote".into())),
                    Some(&q) if q == c => break,
                    Some('\\') => {
                        match chars.get(i + 1) {
                            Some(&e @ ('\\' | '\'' | '"')) => text.push(e),
                            Some('n') => text.push('\n'),
                            Some('t') => text.push('\t'),
                            _ => return Err(err(i, "a backslash escapes \\\\, \\', \\\", \\n, or \\t".into())),
                        }
                        i += 2;
                        continue;
                    }
                    Some(&ch) => text.push(ch),
                }
                i += 1;
            }
            i += 1;
            out.push((start, Token::Text(text)));
            continue;
        }
        if c == '`' {
            let close = chars[i + 1..].iter().position(|&ch| ch == '`');
            let Some(len) = close else { return Err(err(start, "a field name that never closes its backtick".into())) };
            out.push((start, Token::Field(chars[i + 1..i + 1 + len].iter().collect())));
            i += len + 2;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push((start, Token::Name(chars[start..i].iter().collect())));
            continue;
        }
        let two: String = chars[i..chars.len().min(i + 2)].iter().collect();
        let op = ["==", "!=", "<=", ">=", "&&", "||"].into_iter().find(|op| *op == two);
        if let Some(op) = op {
            out.push((start, Token::Op(op)));
            i += 2;
            continue;
        }
        let op =
            ["<", ">", "+", "-", "*", "/", "%", "!", "(", ")", "[", "]", ","].into_iter().find(|op| op.starts_with(c));
        match (op, c) {
            (Some(op), _) => out.push((start, Token::Op(op))),
            (None, '=') => return Err(err(start, "`=` alone: compare with `==`".into())),
            (None, '&' | '|') => return Err(err(start, format!("`{c}` alone: `&&` is and, `||` is or"))),
            (None, _) => return Err(err(start, format!("`{c}` is not part of an expression"))),
        }
        i += 1;
    }
    Ok(out)
}

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Text(String),
    Bool(bool),
    Null,
    /// A column, by name, and where it was written.
    Field(String, usize),
    Not(Box<Expr>),
    Negate(Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>, usize),
    In(Box<Expr>, Vec<Expr>, usize),
    Call(String, Vec<Expr>, usize),
}

struct Parser {
    tokens: Vec<(usize, Token)>,
    next: usize,
    end: usize,
}

impl Parser {
    fn at(&self) -> usize {
        self.tokens.get(self.next).map_or(self.end, |t| t.0)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.next).map(|t| &t.1)
    }

    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Token::Op(o)) if *o == op) {
            self.next += 1;
            return true;
        }
        false
    }

    fn fail<T>(&self, message: impl Into<String>) -> Result<T, ExprError> {
        Err(ExprError { at: self.at(), message: message.into(), data: false })
    }

    fn expect(&mut self, op: &str) -> Result<(), ExprError> {
        if self.eat(op) { Ok(()) } else { self.fail(format!("expected `{op}`")) }
    }

    fn binary(
        &mut self,
        ops: &[&'static str],
        next: fn(&mut Parser) -> Result<Expr, ExprError>,
    ) -> Result<Expr, ExprError> {
        let mut left = next(self)?;
        loop {
            let at = self.at();
            let Some(op) = ops.iter().copied().find(|op| self.eat(op)) else { return Ok(left) };
            left = Expr::Binary(op, Box::new(left), Box::new(next(self)?), at);
        }
    }

    fn or(&mut self) -> Result<Expr, ExprError> {
        self.binary(&["||"], Parser::and)
    }

    fn and(&mut self) -> Result<Expr, ExprError> {
        self.binary(&["&&"], Parser::not)
    }

    fn not(&mut self) -> Result<Expr, ExprError> {
        if self.eat("!") { Ok(Expr::Not(Box::new(self.not()?))) } else { self.compare() }
    }

    fn compare(&mut self) -> Result<Expr, ExprError> {
        let left = self.sum()?;
        let at = self.at();
        if matches!(self.peek(), Some(Token::Name(n)) if n == "in") {
            self.next += 1;
            self.expect("[")?;
            return Ok(Expr::In(Box::new(left), self.list("]")?, at));
        }
        match ["==", "!=", "<=", ">=", "<", ">"].into_iter().find(|op| self.eat(op)) {
            Some(op) => Ok(Expr::Binary(op, Box::new(left), Box::new(self.sum()?), at)),
            None => Ok(left),
        }
    }

    fn sum(&mut self) -> Result<Expr, ExprError> {
        self.binary(&["+", "-"], Parser::product)
    }

    fn product(&mut self) -> Result<Expr, ExprError> {
        self.binary(&["*", "/", "%"], Parser::unary)
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        if self.eat("-") { Ok(Expr::Negate(Box::new(self.unary()?))) } else { self.atom() }
    }

    /// Expressions up to `close`, comma-separated.
    fn list(&mut self, close: &str) -> Result<Vec<Expr>, ExprError> {
        let mut items = Vec::new();
        if self.eat(close) {
            return Ok(items);
        }
        loop {
            items.push(self.or()?);
            if self.eat(close) {
                return Ok(items);
            }
            self.expect(",")?;
        }
    }

    fn atom(&mut self) -> Result<Expr, ExprError> {
        let at = self.at();
        let Some((_, token)) = self.tokens.get(self.next).cloned() else { return self.fail("expected a value") };
        self.next += 1;
        Ok(match token {
            Token::Number(n) => Expr::Number(n),
            Token::Text(t) => Expr::Text(t),
            Token::Field(f) => Expr::Field(f, at),
            Token::Name(name) => match name.as_str() {
                "true" => Expr::Bool(true),
                "false" => Expr::Bool(false),
                "null" => Expr::Null,
                "in" => return Err(ExprError { at, message: "`in` needs a value before it".into(), data: false }),
                _ if self.eat("(") => Expr::Call(name, self.list(")")?, at),
                _ => Expr::Field(name, at),
            },
            Token::Op("(") => {
                let inner = self.or()?;
                self.expect(")")?;
                inner
            }
            Token::Op("[") => {
                return Err(ExprError {
                    at,
                    message: "a list goes after `in`: `region in ['NA', 'EU']`".into(),
                    data: false,
                });
            }
            Token::Op(op) => {
                self.next -= 1;
                return self.fail(format!("expected a value, not `{op}`"));
            }
        })
    }
}

/// Parse `src`.
pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let tokens = tokens(src)?;
    let mut p = Parser { tokens, next: 0, end: src.chars().count() };
    if p.tokens.is_empty() {
        return p.fail("an empty expression");
    }
    let e = p.or()?;
    if p.next < p.tokens.len() {
        return p.fail("expected the end of the expression, or an operator");
    }
    Ok(e)
}

/// The row functions an expression can call.
const FUNCTIONS: &str = "abs, round, floor, ceil, min, max, year, quarter, month, day, lower, upper, len, coalesce, if";

/// An expression checked against a table's columns, and the type it yields.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    expr: Expr,
    pub ty: Type,
}

impl Expr {
    /// Check this expression against columns `names` of `types`, and bind it to them.
    pub fn bind(&self, names: &[String], types: &[ColumnType]) -> Result<Bound, ExprError> {
        let ty = self.check(names, types)?;
        Ok(Bound { expr: self.clone(), ty })
    }

    fn check(&self, names: &[String], types: &[ColumnType]) -> Result<Type, ExprError> {
        let wrong = |at: usize, message: String| ExprError { at, message, data: true };
        let at = |e: &Expr| match e {
            Expr::Field(_, at) | Expr::Binary(.., at) | Expr::In(.., at) | Expr::Call(.., at) => *at,
            _ => 0,
        };
        Ok(match self {
            Expr::Number(_) => Type::Number,
            Expr::Text(_) => Type::Text,
            Expr::Bool(_) => Type::Bool,
            Expr::Null => Type::Null,
            Expr::Field(name, pos) => match names.iter().position(|n| n == name) {
                Some(c) => Type::of(types[c]),
                None => {
                    let have = names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ");
                    return Err(wrong(*pos, format!("no column `{name}`; there are {have}")));
                }
            },
            Expr::Not(e) => match e.check(names, types)? {
                Type::Bool | Type::Null => Type::Bool,
                t => return Err(wrong(at(e), format!("`!` takes true or false, not {}", t.name()))),
            },
            Expr::Negate(e) => match e.check(names, types)? {
                Type::Number | Type::Null => Type::Number,
                t => return Err(wrong(at(e), format!("`-` takes a number, not {}", t.name()))),
            },
            Expr::Binary(op, l, r, pos) => {
                let (a, b) = (l.check(names, types)?, r.check(names, types)?);
                let both = a.with(b);
                let bad = || wrong(*pos, format!("`{op}` cannot take {} and {}", a.name(), b.name()));
                match *op {
                    "+" => match both {
                        Some(t @ (Type::Number | Type::Text | Type::Null)) => t,
                        _ => return Err(bad()),
                    },
                    "-" | "*" | "/" | "%" => match both {
                        Some(Type::Number | Type::Null) => Type::Number,
                        _ => return Err(bad()),
                    },
                    "==" | "!=" => match both {
                        Some(_) => Type::Bool,
                        None => return Err(bad()),
                    },
                    "<" | "<=" | ">" | ">=" => match both {
                        Some(Type::Number | Type::Text | Type::Date | Type::Null) => Type::Bool,
                        _ => return Err(bad()),
                    },
                    _ => match both {
                        Some(Type::Bool | Type::Null) => Type::Bool,
                        _ => return Err(bad()),
                    },
                }
            }
            Expr::In(e, items, pos) => {
                let t = e.check(names, types)?;
                for item in items {
                    let u = item.check(names, types)?;
                    if t.with(u).is_none() {
                        return Err(wrong(*pos, format!("`in` compares {} with {}", t.name(), u.name())));
                    }
                }
                Type::Bool
            }
            Expr::Call(name, args, pos) => {
                let got: Vec<Type> = args.iter().map(|a| a.check(names, types)).collect::<Result<_, _>>()?;
                let arity = |n: std::ops::RangeInclusive<usize>, how: &str| -> Result<(), ExprError> {
                    if n.contains(&got.len()) { Ok(()) } else { Err(wrong(*pos, format!("`{name}` takes {how}"))) }
                };
                let takes = |want: Type| -> Result<(), ExprError> {
                    match got.iter().position(|t| t.with(want) != Some(want)) {
                        None => Ok(()),
                        Some(i) => {
                            Err(wrong(at(&args[i]), format!("`{name}` takes {}, not {}", want.name(), got[i].name())))
                        }
                    }
                };
                match name.as_str() {
                    "abs" | "floor" | "ceil" => {
                        arity(1..=1, "one number")?;
                        takes(Type::Number)?;
                        Type::Number
                    }
                    "round" => {
                        arity(1..=2, "a number and, if you like, how many decimals")?;
                        takes(Type::Number)?;
                        Type::Number
                    }
                    "min" | "max" => {
                        arity(1..=usize::MAX, "one or more numbers")?;
                        takes(Type::Number)?;
                        Type::Number
                    }
                    "year" | "quarter" | "month" | "day" => {
                        arity(1..=1, "one date")?;
                        takes(Type::Date)?;
                        Type::Number
                    }
                    "lower" | "upper" => {
                        arity(1..=1, "one text")?;
                        takes(Type::Text)?;
                        Type::Text
                    }
                    "len" => {
                        arity(1..=1, "one text")?;
                        takes(Type::Text)?;
                        Type::Number
                    }
                    "coalesce" => {
                        arity(1..=usize::MAX, "one or more values")?;
                        let all = got.iter().try_fold(Type::Null, |t, &u| t.with(u));
                        all.ok_or_else(|| wrong(*pos, "`coalesce` takes values of one type".into()))?
                    }
                    "if" => {
                        arity(3..=3, "a condition, a value if it holds, and one if not")?;
                        if got[0].with(Type::Bool).is_none() {
                            return Err(wrong(
                                at(&args[0]),
                                format!("`if` tests true or false, not {}", got[0].name()),
                            ));
                        }
                        got[1].with(got[2]).ok_or_else(|| wrong(*pos, "`if` gives values of one type".into()))?
                    }
                    _ => {
                        return Err(ExprError {
                            at: *pos,
                            message: format!("no function `{name}`; there are {FUNCTIONS}"),
                            data: false,
                        });
                    }
                }
            }
        })
    }
}

impl Bound {
    /// The expression's value on `row`.
    pub fn eval(&self, row: &[Datum], names: &[String]) -> Datum {
        eval(&self.expr, row, names)
    }
}

fn number(n: f64) -> Datum {
    if n.is_finite() { Datum::Number(n) } else { Datum::Null }
}

/// `true` only for `true`: `null` and anything else test false.
fn truth(d: &Datum) -> bool {
    matches!(d, Datum::Bool(true))
}

fn eval(e: &Expr, row: &[Datum], names: &[String]) -> Datum {
    match e {
        Expr::Number(n) => Datum::Number(*n),
        Expr::Text(t) => Datum::Text(t.clone()),
        Expr::Bool(b) => Datum::Bool(*b),
        Expr::Null => Datum::Null,
        Expr::Field(name, _) => names.iter().position(|n| n == name).map_or(Datum::Null, |c| row[c].clone()),
        Expr::Not(e) => Datum::Bool(!truth(&eval(e, row, names))),
        Expr::Negate(e) => match eval(e, row, names) {
            Datum::Number(n) => Datum::Number(-n),
            _ => Datum::Null,
        },
        Expr::Binary(op, l, r, _) => {
            let a = eval(l, row, names);
            if *op == "&&" && !truth(&a) {
                return Datum::Bool(false);
            }
            if *op == "||" && truth(&a) {
                return Datum::Bool(true);
            }
            let b = eval(r, row, names);
            binary(op, a, b)
        }
        Expr::In(e, items, _) => {
            let v = eval(e, row, names);
            Datum::Bool(!matches!(v, Datum::Null) && items.iter().any(|i| eval(i, row, names) == v))
        }
        Expr::Call(name, args, _) => {
            let mut values = args.iter().map(|a| eval(a, row, names));
            call(name, &mut values)
        }
    }
}

fn binary(op: &str, a: Datum, b: Datum) -> Datum {
    use Datum::{Bool, Date, Null, Number, Text};
    match (op, a, b) {
        ("&&" | "||", _, b) => Bool(truth(&b)),
        ("==", a, b) => Bool(a == b),
        ("!=", a, b) => Bool(a != b),
        (_, Null, _) | (_, _, Null) => match op {
            "<" | "<=" | ">" | ">=" => Bool(false),
            _ => Null,
        },
        ("+", Number(x), Number(y)) => number(x + y),
        ("+", Text(x), Text(y)) => Text(x + &y),
        ("-", Number(x), Number(y)) => number(x - y),
        ("*", Number(x), Number(y)) => number(x * y),
        // Dividing by zero gives no value, not infinity.
        ("/", Number(x), Number(y)) => number(x / y),
        ("%", Number(x), Number(y)) => number(x % y),
        (op, a, b) => {
            let order = match (&a, &b) {
                (Number(x), Number(y)) => x.partial_cmp(y),
                (Text(x), Text(y)) => Some(x.cmp(y)),
                (Date(x), Date(y)) => Some(x.0.cmp(&y.0)),
                _ => None,
            };
            let Some(order) = order else { return Null };
            Bool(match op {
                "<" => order.is_lt(),
                "<=" => order.is_le(),
                ">" => order.is_gt(),
                _ => order.is_ge(),
            })
        }
    }
}

/// 10ⁿ for the decimals `round` keeps, exactly.
const TENS: [f64; 16] = [1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15];

fn call(name: &str, args: &mut dyn Iterator<Item = Datum>) -> Datum {
    let values: Vec<Datum> = args.collect();
    let num = |i: usize| match values.get(i) {
        Some(Datum::Number(n)) => Some(*n),
        _ => None,
    };
    let date = || match values.first() {
        Some(Datum::Date(t)) => Some(t.civil()),
        _ => None,
    };
    let text = || match values.first() {
        Some(Datum::Text(t)) => Some(t.as_str()),
        _ => None,
    };
    let or_null = |n: Option<f64>| n.map_or(Datum::Null, number);
    match name {
        "abs" => or_null(num(0).map(f64::abs)),
        "floor" => or_null(num(0).map(f64::floor)),
        "ceil" => or_null(num(0).map(f64::ceil)),
        // Half away from zero, at `digits` decimals (0 to 15).
        "round" => {
            let digits = num(1).unwrap_or(0.0).clamp(0.0, 15.0) as usize;
            or_null(num(0).map(|x| (x * TENS[digits]).round() / TENS[digits]))
        }
        "min" | "max" => {
            let mut all = values.iter().filter_map(|v| match v {
                Datum::Number(n) => Some(*n),
                _ => None,
            });
            let first = all.next();
            or_null(first.map(|f| all.fold(f, |a, b| if name == "min" { a.min(b) } else { a.max(b) })))
        }
        "year" => or_null(date().map(|c| c.year as f64)),
        "quarter" => or_null(date().map(|c| f64::from((c.month - 1) / 3 + 1))),
        "month" => or_null(date().map(|c| f64::from(c.month))),
        "day" => or_null(date().map(|c| f64::from(c.day))),
        "lower" => text().map_or(Datum::Null, |t| Datum::Text(t.to_lowercase())),
        "upper" => text().map_or(Datum::Null, |t| Datum::Text(t.to_uppercase())),
        "len" => text().map_or(Datum::Null, |t| Datum::Number(t.chars().count() as f64)),
        "coalesce" => values.into_iter().find(|v| !matches!(v, Datum::Null)).unwrap_or(Datum::Null),
        "if" => {
            let mut it = values.into_iter();
            let (test, yes, no) = (it.next(), it.next(), it.next());
            if test.as_ref().is_some_and(truth) { yes.unwrap_or(Datum::Null) } else { no.unwrap_or(Datum::Null) }
        }
        _ => Datum::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::DateTime;

    fn table() -> (Vec<String>, Vec<ColumnType>, Vec<Datum>) {
        let names = ["region", "revenue", "cost", "on", "day", "Revenue ($M)"].map(String::from).to_vec();
        let types = vec![
            ColumnType::String,
            ColumnType::Number,
            ColumnType::Number,
            ColumnType::Boolean,
            ColumnType::Date,
            ColumnType::Number,
        ];
        let row = vec![
            Datum::Text("NA".into()),
            Datum::Number(120.0),
            Datum::Number(90.0),
            Datum::Bool(true),
            Datum::Date(DateTime::ymd(2025, 8, 14).unwrap()),
            Datum::Null,
        ];
        (names, types, row)
    }

    fn eval(src: &str) -> Datum {
        let (names, types, row) = table();
        let bound = parse(src).unwrap().bind(&names, &types).unwrap_or_else(|e| panic!("{src}: {e}"));
        bound.eval(&row, &names)
    }

    #[test]
    fn expressions_compute_compare_and_call() {
        assert_eq!(eval("region == 'NA' && revenue > 100"), Datum::Bool(true));
        assert_eq!(eval("(revenue - cost) / revenue * 100"), Datum::Number(25.0));
        assert_eq!(eval("region in ['EU', \"NA\"]"), Datum::Bool(true));
        assert_eq!(eval("!on || cost >= 100"), Datum::Bool(false));
        assert_eq!(eval("-revenue % 7"), Datum::Number(-1.0));
        assert_eq!(eval("round(2 / 3, 2)"), Datum::Number(0.67));
        assert_eq!(eval("round(-2.5)"), Datum::Number(-3.0), "half away from zero");
        assert_eq!(eval("max(cost, 100, revenue)"), Datum::Number(120.0));
        assert_eq!(eval("quarter(day) * 10 + month(day)"), Datum::Number(38.0));
        assert_eq!(eval("lower(region) + '-' + upper('x')"), Datum::Text("na-X".into()));
        assert_eq!(eval("if(revenue > cost, 'up', 'down')"), Datum::Text("up".into()));
        assert_eq!(eval("len('añb')"), Datum::Number(3.0), "characters, not bytes");
    }

    #[test]
    fn null_gives_no_value_and_tests_false() {
        assert_eq!(eval("`Revenue ($M)` * 2"), Datum::Null);
        assert_eq!(eval("`Revenue ($M)` > 1"), Datum::Bool(false));
        assert_eq!(eval("`Revenue ($M)` == null"), Datum::Bool(true));
        assert_eq!(eval("coalesce(`Revenue ($M)`, revenue)"), Datum::Number(120.0));
        assert_eq!(eval("revenue / 0"), Datum::Null, "dividing by zero gives no value");
        assert_eq!(eval("null in [null]"), Datum::Bool(false));
    }

    #[test]
    fn mistakes_say_where_and_what() {
        let (names, types, _) = table();
        let err = |src: &str| match parse(src) {
            Err(e) => e,
            Ok(e) => e.bind(&names, &types).unwrap_err(),
        };
        let e = err("region = 'NA'");
        assert_eq!((e.at, e.data), (7, false));
        assert!(e.message.contains("=="), "{e}");
        assert!(err("revenue +").message.contains("expected a value"));
        assert!(err("'open").message.contains("quote"));
        let e = err("revenu > 1");
        assert!(e.data && e.message.contains("no column `revenu`"), "{e}");
        let e = err("region + 1");
        assert!(e.data && e.message.contains("cannot take text and a number"), "{e}");
        assert!(err("sqrt(revenue)").message.contains("no function `sqrt`"));
        assert!(err("year(region)").message.contains("takes a date, not text"));
        assert!(err("revenue > 1 cost").message.contains("end of the expression"));
    }
}

//! A chart's `dataTransform` (SPEC §3.10): steps that filter, derive, sort, limit,
//! aggregate, fold, and pivot a source's table, in order, before the chart reads it.
//! Expressions are `docs/spec/expr.md`'s. Every step keeps rows in a fixed order and
//! sums in it, so a transform gives the same table on every platform (SPEC §13).

use crate::data::{ColumnType, Datum, Table};
use crate::expr::{self, Expr, Type};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Why a step did not run: which step, where in it (keys from the step down), what is
/// wrong, and whether the step is malformed (E106) or reads its table wrongly (E103).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("step {step}: {message}")]
pub struct TransformError {
    pub step: usize,
    pub at: Vec<String>,
    pub message: String,
    pub data: bool,
}

/// The steps, by name, each with the other keys it takes.
const STEPS: [(&str, &[&str]); 7] = [
    ("filter", &[]),
    ("derive", &[]),
    ("sort", &[]),
    ("limit", &[]),
    ("aggregate", &["groupby"]),
    ("fold", &["as"]),
    ("pivot", &["value", "groupby", "op"]),
];

/// What `aggregate` and `pivot` can compute over a group's values.
const OPS: &str = "count, distinct, sum, mean, median, min, max, first, last";

/// `table` after `steps`, in order.
pub fn apply(mut table: Table, steps: &[Value]) -> Result<Table, TransformError> {
    for (i, step) in steps.iter().enumerate() {
        table = Step { i }.run(step, table)?;
    }
    Ok(table)
}

struct Step {
    i: usize,
}

impl Step {
    fn malformed(&self, at: &[&str], message: impl Into<String>) -> TransformError {
        let at = at.iter().map(|s| s.to_string()).collect();
        TransformError { step: self.i, at, message: message.into(), data: false }
    }

    fn wrong(&self, at: &[&str], message: impl Into<String>) -> TransformError {
        TransformError { data: true, ..self.malformed(at, message) }
    }

    /// The column `name` of `table`, or why not.
    fn column(&self, table: &Table, name: &str, at: &[&str]) -> Result<usize, TransformError> {
        table.column(name).ok_or_else(|| {
            let have = table.columns.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ");
            self.wrong(at, format!("no column `{name}`; there are {have}"))
        })
    }

    /// `src`, an expression in the step at `at`, checked against `table`.
    fn bind(&self, table: &Table, src: &Value, at: &[&str]) -> Result<expr::Bound, TransformError> {
        let src = src.as_str().ok_or_else(|| self.malformed(at, "an expression, as text"))?;
        let parsed = expr::parse(src).map_err(|e| self.malformed(at, format!("`{src}` {e}")))?;
        parsed
            .bind(&table.columns, &table.types)
            .map_err(|e| TransformError { data: e.data, ..self.malformed(at, format!("`{src}` {e}")) })
    }

    /// Field names: one as text, or a list.
    fn fields<'v>(&self, v: &'v Value, at: &[&str]) -> Result<Vec<&'v str>, TransformError> {
        match v {
            Value::String(s) => Ok(vec![s.as_str()]),
            Value::Array(a) => {
                a.iter().map(|f| f.as_str().ok_or_else(|| self.malformed(at, "field names, as text"))).collect()
            }
            _ => Err(self.malformed(at, "a field name, or a list of them")),
        }
    }

    fn run(&self, step: &Value, table: Table) -> Result<Table, TransformError> {
        let Some(o) = step.as_object() else {
            return Err(self.malformed(&[], "a step is an object, such as `{ \"filter\": \"revenue > 0\" }`"));
        };
        let named: Vec<&(&str, &[&str])> = STEPS.iter().filter(|(k, _)| o.contains_key(*k)).collect();
        let [&(kind, extra)] = named[..] else {
            let all = STEPS.iter().map(|(k, _)| format!("`{k}`")).collect::<Vec<_>>().join(", ");
            return Err(self.malformed(&[], format!("a step names one of {all}")));
        };
        if let Some(key) = o.keys().find(|k| k.as_str() != kind && !extra.contains(&k.as_str())) {
            let takes = match extra {
                [] => "nothing else".to_string(),
                keys => keys.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(" and "),
            };
            return Err(self.malformed(&[key.as_str()], format!("`{kind}` takes {takes}")));
        }
        match kind {
            "filter" => self.filter(o, table),
            "derive" => self.derive(o, table),
            "sort" => self.sort(o, table),
            "limit" => self.limit(o, table),
            "aggregate" => self.aggregate(o, table),
            "fold" => self.fold(o, table),
            _ => self.pivot(o, table),
        }
    }

    /// `{ "filter": "region == 'NA'" }`: the rows where it is true.
    fn filter(&self, o: &Map<String, Value>, table: Table) -> Result<Table, TransformError> {
        let test = self.bind(&table, &o["filter"], &["filter"])?;
        if !matches!(test.ty, Type::Bool | Type::Null) {
            return Err(self.wrong(&["filter"], "`filter` keeps the rows where it is true, so it tests true or false"));
        }
        let Table { columns, types, rows } = table;
        let rows = rows.into_iter().filter(|r| test.eval(r, &columns) == Datum::Bool(true)).collect();
        Ok(Table { columns, types, rows })
    }

    /// `{ "derive": { "margin": "profit / revenue" } }`: a column per expression, in
    /// order, each able to read the ones before it; a name that is a column replaces it.
    fn derive(&self, o: &Map<String, Value>, mut table: Table) -> Result<Table, TransformError> {
        let Some(derived) = o["derive"].as_object() else {
            return Err(self.malformed(&["derive"], "new columns by name: `{ \"margin\": \"profit / revenue\" }`"));
        };
        for (name, src) in derived {
            let e = self.bind(&table, src, &["derive", name])?;
            let values: Vec<Datum> = table.rows.iter().map(|r| e.eval(r, &table.columns)).collect();
            let c = match table.column(name) {
                Some(c) => c,
                None => {
                    table.columns.push(name.clone());
                    table.types.push(ColumnType::String);
                    table.rows.iter_mut().for_each(|r| r.push(Datum::Null));
                    table.columns.len() - 1
                }
            };
            table.types[c] = e.ty.column();
            for (row, v) in table.rows.iter_mut().zip(values) {
                row[c] = v;
            }
        }
        Ok(table)
    }

    /// `{ "sort": ["region", "-revenue"] }`: by each field in turn, `-` for descending;
    /// rows that tie keep their order, and nulls go last either way.
    fn sort(&self, o: &Map<String, Value>, mut table: Table) -> Result<Table, TransformError> {
        let mut by = Vec::new();
        for field in self.fields(&o["sort"], &["sort"])? {
            let (descending, name) = match field.strip_prefix('-') {
                Some(name) => (true, name),
                None => (false, field),
            };
            by.push((self.column(&table, name, &["sort"])?, descending));
        }
        crate::sort::by(&mut table.rows, |a, b| {
            by.iter()
                .map(|&(c, descending)| match (&a[c], &b[c]) {
                    (Datum::Null, Datum::Null) => std::cmp::Ordering::Equal,
                    (Datum::Null, _) => std::cmp::Ordering::Greater,
                    (_, Datum::Null) => std::cmp::Ordering::Less,
                    (x, y) if descending => order(y, x),
                    (x, y) => order(x, y),
                })
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(table)
    }

    /// `{ "limit": 8 }`: the first rows.
    fn limit(&self, o: &Map<String, Value>, mut table: Table) -> Result<Table, TransformError> {
        let n = o["limit"].as_u64().ok_or_else(|| self.malformed(&["limit"], "how many rows to keep: 0 or more"))?;
        table.rows.truncate(usize::try_from(n).unwrap_or(usize::MAX));
        Ok(table)
    }

    /// `{ "aggregate": { "total": "sum(revenue)" }, "groupby": ["region"] }`: a row per
    /// group, in the order groups first appear, with its `groupby` fields and each
    /// aggregate. Without `groupby`, every row is one group.
    fn aggregate(&self, o: &Map<String, Value>, table: Table) -> Result<Table, TransformError> {
        let Some(specs) = o["aggregate"].as_object() else {
            return Err(self.malformed(&["aggregate"], "aggregates by name: `{ \"total\": \"sum(revenue)\" }`"));
        };
        let keys = self.groupby(o, &table)?;
        let mut outputs = Vec::new();
        for (name, spec) in specs {
            let at = ["aggregate", name.as_str()];
            let src = spec.as_str().ok_or_else(|| self.malformed(&at, "an aggregate, such as `sum(revenue)`"))?;
            let call = expr::parse(src).map_err(|e| self.malformed(&at, format!("`{src}` {e}")))?;
            let (op, field) = match &call {
                Expr::Call(op, args, _) => match &args[..] {
                    [] => (op.as_str(), None),
                    [Expr::Field(f, _)] => (op.as_str(), Some(f.as_str())),
                    _ => {
                        return Err(
                            self.malformed(&at, format!("`{src}`: an aggregate reads one field, `{op}(field)`"))
                        );
                    }
                },
                _ => return Err(self.malformed(&at, format!("`{src}`: an aggregate is `op(field)`; ops: {OPS}"))),
            };
            let column = field.map(|f| self.column(&table, f, &at)).transpose()?;
            let ty = self.output(op, column.map(|c| table.types[c]), &at)?;
            outputs.push((name.clone(), op.to_string(), column, ty));
        }
        let mut groups = groups(&table, &keys);
        // Every row is one group, even with no rows: a count of none is 0.
        if keys.is_empty() && groups.is_empty() {
            groups.push(Vec::new());
        }
        let mut out = Table {
            columns: keys.iter().map(|&k| table.columns[k].clone()).collect(),
            types: keys.iter().map(|&k| table.types[k]).collect(),
            rows: Vec::with_capacity(groups.len()),
        };
        for (name, _, _, ty) in &outputs {
            out.columns.push(name.clone());
            out.types.push(*ty);
        }
        for members in groups {
            let mut row: Vec<Datum> = keys.iter().map(|&k| table.rows[members[0]][k].clone()).collect();
            for (_, op, column, _) in &outputs {
                let values = members.iter().map(|&r| column.map_or(&Datum::Null, |c| &table.rows[r][c]));
                row.push(compute(op, values, members.len(), column.is_none()));
            }
            out.rows.push(row);
        }
        Ok(out)
    }

    /// `{ "fold": ["2024", "2025"], "as": ["year", "revenue"] }`: wide to long. Each row
    /// becomes one per folded column, with the column's name (`as[0]`, default `key`)
    /// and its value (`as[1]`, default `value`) beside the columns not folded.
    fn fold(&self, o: &Map<String, Value>, table: Table) -> Result<Table, TransformError> {
        let folded = self.fields(&o["fold"], &["fold"])?;
        let folded: Vec<usize> = folded.iter().map(|f| self.column(&table, f, &["fold"])).collect::<Result<_, _>>()?;
        if folded.is_empty() {
            return Err(self.malformed(&["fold"], "the columns to fold, one or more"));
        }
        let names = match o.get("as") {
            None => vec!["key", "value"],
            Some(v) => self.fields(v, &["as"])?,
        };
        let [key, value] = names[..] else {
            return Err(self.malformed(&["as"], "two names: the key's and the value's"));
        };
        let ty = folded.iter().skip(1).try_fold(table.types[folded[0]], |t, &c| (table.types[c] == t).then_some(t));
        let Some(ty) = ty else {
            return Err(self.wrong(&["fold"], "folded columns hold one type, so their values can share a column"));
        };
        let kept: Vec<usize> = (0..table.columns.len()).filter(|c| !folded.contains(c)).collect();
        if let Some(&c) = kept.iter().find(|&&c| table.columns[c] == key || table.columns[c] == value) {
            return Err(self.malformed(&["as"], format!("`{}` is already a column", table.columns[c])));
        }
        let mut columns: Vec<String> = kept.iter().map(|&c| table.columns[c].clone()).collect();
        let mut types: Vec<ColumnType> = kept.iter().map(|&c| table.types[c]).collect();
        columns.extend([key.to_string(), value.to_string()]);
        types.extend([ColumnType::String, ty]);
        let mut rows = Vec::with_capacity(table.rows.len() * folded.len());
        for row in &table.rows {
            for &c in &folded {
                let mut out: Vec<Datum> = kept.iter().map(|&k| row[k].clone()).collect();
                out.extend([Datum::Text(table.columns[c].clone()), row[c].clone()]);
                rows.push(out);
            }
        }
        Ok(Table { columns, types, rows })
    }

    /// `{ "pivot": "year", "value": "revenue", "groupby": ["region"] }`: long to wide. A
    /// row per group with a column per `pivot` value, in the order they first appear,
    /// holding `op` (default `sum` for numbers, else `first`) over the group's values.
    fn pivot(&self, o: &Map<String, Value>, table: Table) -> Result<Table, TransformError> {
        let name = |key: &str| o.get(key).and_then(Value::as_str);
        let by = name("pivot").ok_or_else(|| self.malformed(&["pivot"], "the field whose values become columns"))?;
        let by = self.column(&table, by, &["pivot"])?;
        let value = name("value").ok_or_else(|| self.malformed(&["value"], "`pivot` needs the `value` field"))?;
        let value = self.column(&table, value, &["value"])?;
        let op = match o.get("op") {
            None if table.types[value] == ColumnType::Number => "sum",
            None => "first",
            Some(v) => v.as_str().ok_or_else(|| self.malformed(&["op"], format!("one of {OPS}")))?,
        };
        let ty = self.output(op, Some(table.types[value]), &["op"])?;
        let keys = self.groupby(o, &table)?;
        // The new columns: the pivot field's values, in the order they first appear.
        let mut heads: Vec<String> = Vec::new();
        for row in &table.rows {
            let head = row[by].label();
            if !matches!(row[by], Datum::Null) && !heads.contains(&head) {
                heads.push(head);
            }
        }
        if let Some(h) = heads.iter().find(|h| keys.iter().any(|&k| table.columns[k] == **h)) {
            return Err(self.wrong(&["pivot"], format!("the value `{h}` would be a column twice")));
        }
        let mut out = Table {
            columns: keys.iter().map(|&k| table.columns[k].clone()).chain(heads.iter().cloned()).collect(),
            types: keys.iter().map(|&k| table.types[k]).chain(heads.iter().map(|_| ty)).collect(),
            rows: Vec::new(),
        };
        for members in groups(&table, &keys) {
            let mut row: Vec<Datum> = keys.iter().map(|&k| table.rows[members[0]][k].clone()).collect();
            for head in &heads {
                let cell: Vec<usize> =
                    members.iter().copied().filter(|&r| table.rows[r][by].label() == *head).collect();
                row.push(match cell.is_empty() {
                    true => Datum::Null,
                    false => compute(op, cell.iter().map(|&r| &table.rows[r][value]), cell.len(), false),
                });
            }
            out.rows.push(row);
        }
        Ok(out)
    }

    /// A step's `groupby` columns, if it has any.
    fn groupby(&self, o: &Map<String, Value>, table: &Table) -> Result<Vec<usize>, TransformError> {
        let Some(v) = o.get("groupby") else { return Ok(Vec::new()) };
        self.fields(v, &["groupby"])?.into_iter().map(|f| self.column(table, f, &["groupby"])).collect()
    }

    /// What `op` gives over a column of `input` (`None` for `count()`), or why it cannot.
    fn output(&self, op: &str, input: Option<ColumnType>, at: &[&str]) -> Result<ColumnType, TransformError> {
        Ok(match (op, input) {
            ("count", _) => ColumnType::Number,
            ("distinct", Some(_)) => ColumnType::Number,
            ("sum" | "mean" | "median", Some(ColumnType::Number)) => ColumnType::Number,
            ("sum" | "mean" | "median", Some(t)) => {
                return Err(self.wrong(at, format!("`{op}` adds up numbers; this field holds {}s", t.name())));
            }
            ("min" | "max", Some(t @ (ColumnType::Number | ColumnType::Date | ColumnType::String))) => t,
            ("min" | "max", Some(_)) => {
                return Err(self.wrong(at, format!("`{op}` compares numbers, dates, or text")));
            }
            ("first" | "last", Some(t)) => t,
            (op, None) if OPS.split(", ").any(|o| o == op) => {
                return Err(self.malformed(at, format!("`{op}` reads a field: `{op}(field)`")));
            }
            (op, _) => return Err(self.malformed(at, format!("no aggregate `{op}`; there are {OPS}"))),
        })
    }
}

/// Two values of one column in order: numbers and dates by value, text by code point,
/// false before true.
fn order(a: &Datum, b: &Datum) -> std::cmp::Ordering {
    match (a, b) {
        (Datum::Number(x), Datum::Number(y)) => x.total_cmp(y),
        (Datum::Text(x), Datum::Text(y)) => x.cmp(y),
        (Datum::Date(x), Datum::Date(y)) => x.0.cmp(&y.0),
        (Datum::Bool(x), Datum::Bool(y)) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    }
}

/// The rows of each group of `table` by `keys`, groups in the order they first appear;
/// one group of every row without keys.
fn groups(table: &Table, keys: &[usize]) -> Vec<Vec<usize>> {
    // A group's identity: its key values, exactly (a number by its bits).
    let id = |row: &[Datum]| -> String {
        let mut out = String::new();
        for &k in keys {
            match &row[k] {
                Datum::Number(n) => out.push_str(&format!("n{:x}", n.to_bits())),
                Datum::Text(t) => out.push_str(&format!("t{t}")),
                Datum::Bool(b) => out.push_str(if *b { "b1" } else { "b0" }),
                Datum::Date(d) => out.push_str(&format!("d{}", d.0)),
                Datum::Null => out.push('z'),
            }
            out.push('\u{0}');
        }
        out
    };
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<Vec<usize>> = Vec::new();
    for (r, row) in table.rows.iter().enumerate() {
        let group = *at.entry(id(row)).or_insert_with(|| {
            out.push(Vec::new());
            out.len() - 1
        });
        out[group].push(r);
    }
    out
}

/// `op` over a group's `values` (`rows` of them; `count()` counts rows), nulls aside.
fn compute<'d>(op: &str, values: impl Iterator<Item = &'d Datum>, rows: usize, all: bool) -> Datum {
    let present: Vec<&Datum> = values.filter(|v| !matches!(v, Datum::Null)).collect();
    let numbers = || present.iter().filter_map(|v| if let Datum::Number(n) = v { Some(*n) } else { None });
    let some = |n: f64| if n.is_finite() { Datum::Number(n) } else { Datum::Null };
    match op {
        "count" if all => Datum::Number(rows as f64),
        "count" => Datum::Number(present.len() as f64),
        "distinct" => {
            let mut seen: Vec<&Datum> = Vec::new();
            for v in &present {
                if !seen.contains(v) {
                    seen.push(v);
                }
            }
            Datum::Number(seen.len() as f64)
        }
        "sum" => some(numbers().fold(0.0, |a, b| a + b)),
        "mean" if present.is_empty() => Datum::Null,
        "mean" => some(numbers().fold(0.0, |a, b| a + b) / present.len() as f64),
        "median" => {
            let mut sorted: Vec<f64> = numbers().collect();
            crate::sort::by(&mut sorted, f64::total_cmp);
            match sorted.len() {
                0 => Datum::Null,
                n if n % 2 == 1 => Datum::Number(sorted[n / 2]),
                n => some((sorted[n / 2 - 1] + sorted[n / 2]) / 2.0),
            }
        }
        "min" => present.iter().copied().min_by(|a, b| order(a, b)).cloned().unwrap_or(Datum::Null),
        "max" => present.iter().copied().max_by(|a, b| order(a, b)).cloned().unwrap_or(Datum::Null),
        "first" => present.first().map_or(Datum::Null, |v| (*v).clone()),
        _ => present.last().map_or(Datum::Null, |v| (*v).clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sales() -> Table {
        let row = |region: &str, year: f64, revenue: Option<f64>| {
            vec![Datum::Text(region.into()), Datum::Number(year), revenue.map_or(Datum::Null, Datum::Number)]
        };
        Table {
            columns: vec!["region".into(), "year".into(), "revenue".into()],
            types: vec![ColumnType::String, ColumnType::Number, ColumnType::Number],
            rows: vec![
                row("NA", 2024.0, Some(10.0)),
                row("EU", 2024.0, Some(7.0)),
                row("NA", 2025.0, Some(14.0)),
                row("EU", 2025.0, None),
                row("APAC", 2025.0, Some(9.0)),
            ],
        }
    }

    fn run(steps: Value) -> Table {
        apply(sales(), steps.as_array().unwrap()).unwrap_or_else(|e| panic!("{e}"))
    }

    fn column(t: &Table, name: &str) -> Vec<Datum> {
        let c = t.column(name).unwrap();
        t.rows.iter().map(|r| r[c].clone()).collect()
    }

    fn texts(t: &Table, name: &str) -> Vec<String> {
        column(t, name).iter().map(Datum::label).collect()
    }

    #[test]
    fn filter_derive_sort_and_limit_run_in_order() {
        let t = run(json!([
            { "filter": "year == 2025" },
            { "derive": { "k": "revenue / 1000", "big": "k > 0.01" } },
            { "sort": "-revenue" },
            { "limit": 2 }
        ]));
        assert_eq!(texts(&t, "region"), ["NA", "APAC"], "EU's null sorts last");
        assert_eq!(column(&t, "big"), [Datum::Bool(true), Datum::Bool(false)]);
        assert_eq!(t.types[t.column("big").unwrap()], ColumnType::Boolean);
        // Ties keep their order; text sorts by code point.
        let t = run(json!([{ "sort": ["year", "region"] }]));
        assert_eq!(texts(&t, "region"), ["EU", "NA", "APAC", "EU", "NA"]);
    }

    #[test]
    fn aggregate_and_pivot_group_in_order_of_first_appearance() {
        let t = run(json!([{ "aggregate": { "total": "sum(revenue)", "n": "count()", "years": "distinct(year)",
                                             "best": "max(revenue)", "mid": "median(revenue)" },
                             "groupby": "region" }]));
        assert_eq!(t.columns, ["region", "total", "n", "years", "best", "mid"]);
        assert_eq!(texts(&t, "region"), ["NA", "EU", "APAC"]);
        assert_eq!(texts(&t, "total"), ["24", "7", "9"]);
        assert_eq!(texts(&t, "n"), ["2", "2", "1"], "count() counts rows, nulls too");
        assert_eq!(texts(&t, "mid"), ["12", "7", "9"]);
        let wide = run(json!([{ "pivot": "year", "value": "revenue", "groupby": ["region"] }]));
        assert_eq!(wide.columns, ["region", "2024", "2025"]);
        assert_eq!(column(&wide, "2025"), [Datum::Number(14.0), Datum::Number(0.0), Datum::Number(9.0)]);
        assert_eq!(column(&wide, "2024")[2], Datum::Null, "APAC has no 2024");
        // And back: fold is pivot's inverse.
        let long =
            apply(wide, json!([{ "fold": ["2024", "2025"], "as": ["year", "revenue"] }]).as_array().unwrap()).unwrap();
        assert_eq!(long.columns, ["region", "year", "revenue"]);
        assert_eq!(texts(&long, "year")[..2], ["2024", "2025"]);
        assert_eq!(long.rows.len(), 6);
    }

    #[test]
    fn a_bad_step_says_which_and_where() {
        let err = |steps: Value| apply(sales(), steps.as_array().unwrap()).unwrap_err();
        let e = err(json!([{ "limit": 2 }, { "filter": "revenue >" }]));
        assert_eq!((e.step, e.at.clone(), e.data), (1, vec!["filter".to_string()], false));
        let e = err(json!([{ "derive": { "m": "profit / revenue" } }]));
        assert_eq!((e.at.clone(), e.data), (vec!["derive".to_string(), "m".to_string()], true));
        assert!(e.message.contains("no column `profit`"), "{e}");
        assert!(err(json!([{ "filter": "revenue" }])).message.contains("true or false"));
        assert!(err(json!([{ "sort": "-profit" }])).data);
        assert!(err(json!([{ "aggregate": { "t": "sum(region)" } }])).message.contains("adds up numbers"));
        assert!(err(json!([{ "aggregate": { "t": "total(revenue)" } }])).message.contains("no aggregate"));
        assert!(err(json!([{ "filter": "true", "limit": 3 }])).message.contains("names one of"));
        assert!(err(json!([{ "limit": 3, "groupby": "x" }])).message.contains("takes nothing else"));
        assert!(err(json!([{ "fold": ["region", "year"] }])).message.contains("one type"));
    }
}

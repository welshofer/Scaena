//! Tables (SPEC §3.3): a data source's rows, through its `dataTransform`, set in the
//! theme's table styles once per snapshot. Every cell is laid-out text keyed by its row
//! and its column, so a transition moves a row that persists to where it now stands and
//! fades rows and columns in and out ([`crate::sample`]).

use crate::EngineError;
use crate::charts::{CategoryFormat, Ctx, Rule, typeset_minus};
use crate::data::{self, ColumnType, Datum};
use crate::text::{TextLayout, TextSpec};
use crate::theme::Numeric;
use scaena_core::displaylist::{Color, Point};
use scaena_core::document::Props;
use scaena_core::format::{Locale, MINUS};
use serde_json::Value;

/// A table laid out for one snapshot, relative to its cell.
#[derive(Debug, Clone, PartialEq)]
pub struct TableLayout {
    /// The header row's cells, keyed by column; none without a header.
    pub header: Vec<Cell>,
    /// The body's cells, row by row, keyed by row and column.
    pub cells: Vec<Cell>,
    /// The rule under the header.
    pub rule: Option<Rule>,
    /// The rules between rows, each keyed by the row above it.
    pub row_rules: Vec<(String, Rule)>,
}

/// One cell's text, where it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Its row's key; empty in the header.
    pub row: String,
    /// Its column's field.
    pub column: String,
    /// The text box's top-left corner, relative to the table.
    pub origin: Point,
    /// The point the text aligns to: on its baseline, at the column's start, middle, or
    /// end. Text that changes keeps to it as it cross-fades.
    pub anchor: Point,
    pub text: TextLayout,
}

/// One column as the table sets it.
struct Column {
    index: usize,
    field: String,
    title: String,
    format: Option<CategoryFormat>,
    /// How much of the spare width falls before the text: 0 start, ½ center, 1 end.
    align: f32,
    numeric: bool,
}

/// Lay out a table node's resolved props for a cell `size` wide and high.
pub fn compile(cx: &mut Ctx, props: &Props, size: [f32; 2]) -> Result<TableLayout, EngineError> {
    let source = props.get("data").and_then(Value::as_str).and_then(|d| d.strip_prefix('@'));
    let source = source.ok_or_else(|| EngineError::Layout("table has no `data`".into()))?;
    let table = data::transform(data::load(cx.deck, cx.data, source)?, props.get("dataTransform"))?;
    let locale = Locale::of(cx.deck.meta.as_ref().and_then(|m| m.lang.as_deref()));
    let col = |name: &str| table.column(name).ok_or_else(|| EngineError::Data(format!("no column `{name}`")));

    // Columns: as listed, else every column of the data.
    let mut columns: Vec<Column> = Vec::new();
    match props.get("columns").and_then(Value::as_array) {
        Some(listed) => {
            for (k, c) in listed.iter().enumerate() {
                let field = c.get("field").and_then(Value::as_str).unwrap_or_default();
                let index = col(field)?;
                let format = (c.get("format").and_then(Value::as_str))
                    .map(|f| CategoryFormat::parse(f, table.types[index], field, &format!("columns[{k}]")))
                    .transpose()?;
                let numeric = table.types[index] == ColumnType::Number;
                let align = match c.get("align").and_then(Value::as_str) {
                    Some("start") => 0.0,
                    Some("center") => 0.5,
                    Some("end") => 1.0,
                    _ if numeric => 1.0,
                    _ => 0.0,
                };
                let title = c.get("title").and_then(Value::as_str).unwrap_or(field).to_string();
                columns.push(Column { index, field: field.to_string(), title, format, align, numeric });
            }
        }
        None => {
            for (index, field) in table.columns.iter().enumerate() {
                let numeric = table.types[index] == ColumnType::Number;
                let align = if numeric { 1.0 } else { 0.0 };
                columns.push(Column {
                    index,
                    field: field.clone(),
                    title: field.clone(),
                    format: None,
                    align,
                    numeric,
                });
            }
        }
    }
    if columns.is_empty() {
        return Err(EngineError::Data("table has no columns".into()));
    }
    // Rows by key: the `key` field's value, else the first column's.
    let key = match props.get("key").and_then(Value::as_str) {
        Some(k) => col(k)?,
        None => columns[0].index,
    };
    let mut keys: Vec<String> = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        let k = row[key].label();
        if keys.contains(&k) {
            return Err(EngineError::Data(format!(
                "row key `{k}` repeats; table keys must be unique (give the table a `key` field)"
            )));
        }
        keys.push(k);
    }

    // Theme: table styles, all tokens.
    let theme = cx.theme;
    let styles = theme.tables.as_ref();
    let gap = theme.tokens.space.unit as f32;
    let row_gap = styles.and_then(|t| t.row_gap).map_or(0.5 * gap, |g| g as f32);
    let role = |text: Option<&scaena_core::model::theme::TableText>, role: &str, color: Option<&str>| {
        let mut r = theme.text_role(text.and_then(|t| t.role.as_deref()).unwrap_or(role))?;
        if let Some(color) = text.and_then(|t| t.color.as_deref()).or(color) {
            r.color = Some(color.to_string());
        }
        Ok::<_, EngineError>(r)
    };
    let head_role = role(styles.and_then(|t| t.header.as_ref()), "label", Some("onSurfaceMuted"))?;
    let cell_role = role(styles.and_then(|t| t.cell.as_ref()), "body", None)?;
    // Columns stand an em of the cell text apart unless the theme says otherwise.
    let column_gap = styles.and_then(|t| t.column_gap).map_or(cell_role.size, |g| g as f32);
    let rule_of = |r: &scaena_core::model::theme::ChartRule| -> Result<Rule, EngineError> {
        let width = theme.stroke(r.stroke.as_deref().unwrap_or("hairline"))?;
        let Color([red, green, blue, a]) = theme.color(r.color.as_deref().unwrap_or("onSurfaceMuted"))?;
        let opacity = r.opacity.unwrap_or(1.0).clamp(0.0, 1.0);
        let color = Color([red, green, blue, (f64::from(a) * opacity).round() as u8]);
        Ok(Rule { from: [0.0, 0.0], to: [size[0], 0.0], width, color })
    };
    let default_rule = scaena_core::model::theme::ChartRule { role: None, stroke: None, color: None, opacity: None };
    let header_rule = rule_of(styles.and_then(|t| t.rule.as_ref()).unwrap_or(&default_rule))?;
    let row_rule = styles.and_then(|t| t.row_rule.as_ref()).map(rule_of).transpose()?;

    // Text: every cell set once, numbers in tabular lining figures, and the minus sign
    // the cell font sets (U+2212 if it has one).
    let mut set = |text: String, role: &crate::theme::TextRole, numeric: bool| -> Result<TextLayout, EngineError> {
        let spec =
            TextSpec { numeric: numeric.then_some(Numeric::TabularLining), ..TextSpec::plain(role.clone(), text) };
        cx.text.layout(cx.fonts, theme, &spec, f32::INFINITY)
    };
    let minus = match columns.iter().any(|c| c.numeric) {
        true => {
            let probe = set(format!("{MINUS}0"), &cell_role, true)?;
            if probe.runs.iter().flat_map(|r| &r.glyphs).any(|g| g.id == 0) { '-' } else { MINUS }
        }
        false => MINUS,
    };
    let show_header = props.get("header").and_then(Value::as_bool).unwrap_or(true);
    let header: Vec<Option<TextLayout>> = columns
        .iter()
        .map(|c| if show_header { set(c.title.clone(), &head_role, false).map(Some) } else { Ok(None) })
        .collect::<Result<_, _>>()?;
    let mut body: Vec<Vec<Option<TextLayout>>> = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        let mut cells = Vec::with_capacity(columns.len());
        for c in &columns {
            let d = &row[c.index];
            let text = match (&c.format, d) {
                (_, Datum::Null) => String::new(),
                (Some(f), d) => f.print(d, locale),
                (None, d) => d.label(),
            };
            let text = typeset_minus(text, minus);
            cells.push(if text.is_empty() { None } else { Some(set(text, &cell_role, c.numeric)?) });
        }
        body.push(cells);
    }

    // Widths: each column as wide as its widest text; the first column takes what the
    // cell has to spare, so the rest keep together at its far side.
    let mut widths: Vec<f32> = vec![0.0; columns.len()];
    for row in std::iter::once(&header).chain(&body) {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = w.max(cell.as_ref().map_or(0.0, |t| t.width));
        }
    }
    let need = widths.iter().sum::<f32>() + column_gap * (columns.len() - 1) as f32;
    if need > size[0] {
        return Err(EngineError::Layout(format!(
            "table needs {need:.0} cu across for its {} columns, and its cell is {:.0}: show fewer columns, or give it more room",
            columns.len(),
            size[0]
        )));
    }
    widths[0] += size[0] - need;
    let lefts: Vec<f32> = widths
        .iter()
        .scan(0.0, |x, w| {
            let at = *x;
            *x += w + column_gap;
            Some(at)
        })
        .collect();

    // Rows: each its tallest text with `row_gap` above and below, the cells' first
    // baselines on one line.
    let mut out = TableLayout { header: Vec::new(), cells: Vec::new(), rule: None, row_rules: Vec::new() };
    let mut y = 0.0_f32;
    let place = |cells: Vec<Option<TextLayout>>, row: &str, y: &mut f32| -> Vec<Cell> {
        let baseline = cells.iter().flatten().filter_map(|t| t.lines.first()).map(|l| l.baseline).fold(0.0, f32::max);
        let height = cells.iter().flatten().map(|t| t.height).fold(0.0_f32, f32::max);
        let mut placed = Vec::new();
        for ((c, text), (left, width)) in columns.iter().zip(cells).zip(lefts.iter().zip(&widths)) {
            let Some(text) = text else { continue };
            let first = text.lines.first().map_or(0.0, |l| l.baseline);
            let origin = [left + c.align * (width - text.width), *y + row_gap + baseline - first];
            let anchor = [left + c.align * width, *y + row_gap + baseline];
            placed.push(Cell { row: row.to_string(), column: c.field.clone(), origin, anchor, text });
        }
        *y += row_gap + height + row_gap;
        placed
    };
    if show_header {
        out.header = place(header, "", &mut y);
        out.rule = Some(Rule { from: [0.0, y], to: [size[0], y], ..header_rule });
    }
    let rows = body.len();
    for (r, (cells, key)) in body.into_iter().zip(&keys).enumerate() {
        out.cells.extend(place(cells, key, &mut y));
        if let Some(rule) = &row_rule
            && r + 1 < rows
        {
            out.row_rules.push((key.clone(), Rule { from: [0.0, y], to: [size[0], y], ..rule.clone() }));
        }
    }
    if y > size[1] + 0.5 {
        let head = out.rule.as_ref().map_or(0.0, |r| r.from[1]);
        let pitch = (y - head) / rows.max(1) as f32;
        let fits = ((size[1] - head) / pitch).floor().max(0.0);
        return Err(EngineError::Layout(format!(
            "table shows {rows} rows in {y:.0} cu, and its cell is {:.0} high: keep about {fits:.0} with `dataTransform` ({{ \"limit\": {fits:.0} }}), or give it more room",
            size[1]
        )));
    }
    Ok(out)
}

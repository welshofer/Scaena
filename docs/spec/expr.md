# Data expressions

A chart's `dataTransform` (SPEC §3.10) filters rows and derives columns with expressions over one row at a time: `region == 'NA' && revenue > 0`, `round(profit / revenue * 100, 1)`. The language is small, typed, and reads like JavaScript, so an expression from a Vega spec or a d3 filter mostly works here. An expression is parsed once, checked against the table's column types before any row is read, and evaluated with plain IEEE arithmetic, so it gives the same value on every platform (§13). `scaena-core::expr` implements this page and `scaena-core::transform` the steps that use it.

## Values

| Type | Written | From a column typed |
|---|---|---|
| number | `12`, `0.5`, `.5`, `1e6` | `number` |
| text | `'NA'` or `"NA"`; `\\`, `\'`, `\"`, `\n`, `\t` escape | `string` |
| true or false | `true`, `false` | `boolean` |
| date | (no literal; read from a column) | `date` |
| null | `null` | an empty cell |

A column is named bare when it is a word (`revenue`, `région`, `q_3`), and in backticks otherwise: `` `Revenue ($M)` ``. `true`, `false`, `null`, and `in` are not column names.

## Operators

From the loosest binding to the tightest:

| Operators | Takes | Gives |
|---|---|---|
| `\|\|` | true or false | true when either side is |
| `&&` | true or false | true when both sides are |
| `!` | true or false | the opposite |
| `==` `!=` | two values of one type | whether they are equal |
| `<` `<=` `>` `>=` | two numbers, texts, or dates | their order; text by code point |
| `x in [a, b, …]` | a value and a list of its type | whether it is one of them |
| `+` | two numbers, or two texts | the sum, or the texts joined |
| `-` | two numbers | the difference |
| `*` `/` `%` | two numbers | the product, quotient, or remainder (sign of the left side) |
| `-x` | a number | its negative |

Parentheses group. `&&` and `||` stop early: `revenue > 0 || x / 0 > 1` never divides when the left side holds. `=` alone, `&`, and `|` are errors that say what to write instead.

## Functions

| Function | Takes | Gives |
|---|---|---|
| `abs(x)` `floor(x)` `ceil(x)` | a number | its absolute value, or it rounded down or up |
| `round(x)`, `round(x, d)` | a number, and decimals from 0 to 15 | it rounded to `d` decimals (0 by default), a tie away from zero |
| `min(a, …)` `max(a, …)` | one or more numbers | the least or the greatest |
| `year(d)` `quarter(d)` `month(d)` `day(d)` | a date | its year, quarter 1–4, month 1–12, or day of the month |
| `lower(s)` `upper(s)` | text | it in lower or upper case |
| `len(s)` | text | how many characters it has |
| `coalesce(a, b, …)` | values of one type | the first that is not null |
| `if(test, a, b)` | true or false, and two values of one type | `a` when the test holds, else `b` |

## Types are checked first

Every column an expression names must be in the table at that step, and every operator and function must get the types it takes, or the transform does not run. `validate` reports a column that is not there, or one used as the wrong type, as **E103** at the step, with the columns there are: `` `profit / rev` at 0: no column `profit`; there are `quarter`, `rev`, `when` ``. An expression that does not parse, or names a function that does not exist, is **E106**. Positions count characters from 0.

## Null

A cell with no value is null. Null keeps an expression from inventing a value:

- Arithmetic with null gives null: `revenue * 2` is null when `revenue` is.
- Dividing by zero gives null, not infinity, and so does any result too large to hold.
- `<`, `<=`, `>`, `>=` with null give false. `== null` and `!= null` test for it.
- `&&`, `||`, `!`, a `filter`, and an `if` read null as false.
- `x in […]` is false when `x` is null.
- `coalesce` picks the first value that is not null.

## Determinism

Numbers are 64-bit floats, and only `+ − × ÷ %`, comparisons, `abs`, `floor`, `ceil`, `round`, `min`, and `max` touch them: operations IEEE 754 defines exactly, so every platform gets the same bits. There is no `pow`, `log`, or trigonometry, whose results differ by platform. Text compares by Unicode code point, never by a locale's collation, and `lower` and `upper` use Unicode's default case mapping.

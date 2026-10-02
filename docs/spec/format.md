# Number and date formats

Charts and tables print numbers and dates with format strings: an encoding's `format` (SPEC §3.7), and a data source's `parse`, which reads dates (§3.10). The grammar is a subset of d3-format and d3-time-format, so a format seen in a d3 or Vega chart works here. There is one addition, the compact type `k`. Formatting is pure: the same value, format, and language give the same text on every platform (§13). `scaena-core::format` implements this page.

## Numbers

```
[sign][$][,][.precision][~][type]
```

| Part | Meaning |
|---|---|
| sign | `-` (the default): a minus sign for negatives only. `+`: a plus sign for zero and positives too. `(`: parentheses around negatives. A space: a space for zero and positives. |
| `$` | The language's currency symbol: `$1.5`, `1,5 €`. |
| `,` | Thousands grouped with the language's separator. |
| `.precision` | Digits after the decimal point for `f` and `%`; significant digits for the other types. The default is 6, or 12 with no type, as in d3. |
| `~` | Trailing zeros after the decimal point are dropped, and the point too when nothing follows it. |
| type | What prints; the table below. |

| Type | Prints | `1234.5` as `.3` + type |
|---|---|---|
| `f` | fixed point | `1234.500` |
| `d` | an integer; precision is ignored | `1235` |
| `%` | the value × 100 in fixed point, then the language's percent sign | `123450.000%` |
| `e` | exponent notation | `1.234e+3` |
| `r` | significant digits, in fixed point | `1230` |
| `g` | `e` when the exponent is below −6 or at least the precision, else `r` | `1.23e+3` |
| `s` | significant digits with an SI prefix: … µ m (none) k M G T … | `1.23k` |
| `k` | significant digits with the language's compact suffix: K, M, B, T in English | `1.23K` |
| none | like `g`, with `~` | `1234.5` |

- **`k` is ours.** d3's `s` writes billions as `G`, which reads wrong in business. `k` uses the language's short scale instead (CLDR's compact-short), so `$.2~k` prints `$1.2B`. Past trillions it stays in trillions: `2500T`.
- **Rounding.** A value rounds to the nearest digit it keeps, a tie away from zero. Rounding uses the value's exact binary value, so `1.005` at `.2f` is `1.00`, since `1.005` is stored as 1.00499999…. This matches JavaScript's `toFixed`, so the text matches d3's. A carry can move to the next prefix: `999950` at `.3s` is `1.00M`.
- **Signs.** The minus sign is U+2212 (−), as d3 sets it. Where the label's font has no U+2212, the engine sets a hyphen-minus (-). A negative value that rounds to zero prints no sign unless the format starts with `+`. `NaN` prints `NaN`, and infinity `∞`.
- **No format.** A number with no `format` prints with no type: `0.1 + 0.2` prints `0.3`, `24` prints `24`, and `1e21` prints `1e+21`.
- **Counting labels** (§3.7) print every frame in their encoding's format. With no format, they show as many decimal places as either end does.
- **Left out of d3's grammar:** fill, alignment, width, zero padding, `#`, and the types `b o x X c p n`. A slide aligns numbers by layout and tabular figures, not by padding.

## Dates

Directives, among literal text:

| Directive | Prints | 2025-03-05 14:07:09 |
|---|---|---|
| `%Y` | year | `2025` |
| `%y` | year without century | `25` |
| `%q` | quarter, 1–4 | `1` |
| `%m` | month, 01–12 | `03` |
| `%B` / `%b` | month name, full / short | `March` / `Mar` |
| `%d` | day, 01–31 | `05` |
| `%e` | day, padded with a space | ` 5` |
| `%j` | day of the year, 001–366 | `064` |
| `%A` / `%a` | weekday, full / short | `Wednesday` / `Wed` |
| `%H` | hour, 00–23 | `14` |
| `%I` | hour, 01–12 | `02` |
| `%p` | AM or PM | `PM` |
| `%M` | minute | `07` |
| `%S` | second | `09` |
| `%%` | a percent sign | `%` |

- **Padding.** After the `%`, `-` drops a number's padding, `_` pads with spaces, and `0` pads with zeros: `%b %-d, %Y` prints `Mar 5, 2025`.
- **Time zones.** Dates have none. A date is a civil date and time, so 2025-03-01T09:00 is nine o'clock wherever the deck plays.
- **Reading dates.** A column the source's `schema` types `date` reads with its `parse` format, keyed by column (`"parse": { "month": "%b %Y" }`). A format that reads a month, quarter, day, or weekday reads the year too (`%Y` or `%y`): one without would put every date in 1900, and is E103. Periods without a year (`Jan`, `Q3`) are text, on an ordinal axis in their order. A time of day alone (`%H:%M`) reads no date and needs no year. Without one, it reads ISO 8601: `2025`, `2025-03`, `2025-03-05`, or with `T14:07` or `T14:07:09`, and a trailing `Z` is ignored.
  - Names read in any case.
  - Fields the format does not name default to 1900-01-01 at midnight, as d3's do.
  - `%y` reads 00–68 as 2000–2068, and 69–99 as 1969–1999.
  - The whole text must match.

## Languages

The deck's `meta.lang` picks the language. A tag reads as its exact entry below, else the first entry for its language (`fr-CA` → `fr-FR`), else `en-US`. Adding a language is one table row in `scaena-core::format::LOCALES`, with a test.

| Tag | Decimal | Group | Currency | Percent | Compact |
|---|---|---|---|---|---|
| `en-US` | `.` | `,` | `$1` | `1%` | K M B T |
| `en-GB` | `.` | `,` | `£1` | `1%` | K M B T |
| `de-DE` | `,` | `.` | `1 €` | `1%` | Tsd. Mio. Mrd. Bio. |
| `fr-FR` | `,` | no-break space | `1 €` | `1 %` | k M Md Bn |
| `es-ES` | `,` | `.` | `1 €` | `1 %` | mil M mil M B |
| `nl-NL` | `,` | `.` | `€ 1` | `1%` | K mln. mld. bln. |

Month and weekday names come from the same entry. Separators and currency follow d3's locale files, and compact suffixes follow CLDR. The spaces in this table are no-break spaces (U+00A0).

## Validation

- A `format` or `parse` string that does not parse is **E106**.
- A `format` on a column that holds neither numbers nor dates is **E103**.
- A value that does not fit its column's schema type or `parse` format is **E103**, at its source.

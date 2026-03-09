---
name: Add NRQL advanced features
overview: "Extend the parser and AST to support six documented NRQL features: named function arguments, filter(agg, WHERE ...), rate(agg, time interval), funnel(attr, WHERE ...), FACET CASES(...), and EXTRAPOLATE."
todos: []
isProject: false
---

# Plan: Add support for 6 NRQL advanced features

## Current state

- **AST** ([src/ast.rs](src/ast.rs)): `SelectArg` is `Wildcard | Literal | Attribute | Function`. `FunctionCall` has `name` and `args: Vec<SelectArg>`. `FacetItem` is `Attr | Function`. `TimeseriesClause` is `Auto | Interval { n, unit }`. No named args, no WHERE-in-args, no time-interval arg type, no FACET CASES, no EXTRAPOLATE.
- **Parser** ([src/parser.rs](src/parser.rs)): `select_arg` parses `*`, literal, attribute_ref, or function_call. `function_call` parses `ident ( * | comma-separated select_arg )`. `facet_clause` parses `FACET facet_item ( , facet_item )`* then optional ORDER BY. `timeseries_clause` parses `TIMESERIES` then AUTO | interval | success. `condition` and `where_clause` exist and can be reused for "WHERE condition" in arguments.

## 1. Named arguments (e.g. `apdex(duration, t: 0.4)`)

**AST**

- Add `SelectArg::Named { name: String, value: Box<SelectArg> }` so function args can be named without changing `FunctionCall.args` type.

**Parser**

- Add `function_arg(i)` that tries in order: `ident ':' select_arg` (named), then existing `select_arg`. Use `function_arg` inside `function_call` in place of `select_arg` for the comma-separated list.

**Tests**

- Move "apdex named arg" from EXPECTED_FAILURES to success list; add fixture for `apdex(duration, t: 0.4)`.

---

## 2. filter(agg, WHERE condition) and 4. funnel(attr, WHERE ..., WHERE ...)

**AST**

- Add `SelectArg::WhereCondition(Condition)` reusing existing `Condition`.

**Parser**

- In `function_arg`, add alternative: `keyword("WHERE")` then `condition(i)` before other args. Try WHERE first so it is not parsed as an attribute.

**Tests**

- Move "filter with WHERE" and "funnel with WHERE" to success list; add fixtures.

---

## 3. rate(count(*), 5 minutes) — time interval as argument

**AST**

- Add `TimeInterval { n: u64, unit: TimeUnit }` in ast.rs (next to TimeExpr).
- Add `SelectArg::TimeInterval(TimeInterval)`.

**Parser**

- Add `time_interval_arg`: parse `number_str` then `time_unit` (no "ago"), return `SelectArg::TimeInterval(...)`. In `function_arg`, try time_interval before literal/attribute so "5 minutes" is one arg.

**Tests**

- Move "rate with interval" to success list; add fixture for `rate(count(*), 5 minutes)`.

---

## 5. FACET CASES(WHERE ... AS 'Label', ...)

**AST**

- Add `FacetCase { condition: Condition, alias: Option<String> }`.
- Add `FacetItem::Cases(Vec<FacetCase>)`.

**Parser**

- In `facet_clause`, after `FACET`, try `keyword("CASES")` then `delimited('(', separated_list0(facet_case), ')')` before the existing facet_item list. `facet_case`: `WHERE` + `condition` + optional `AS` + string literal.

**Tests**

- Move "FACET CASES" to success list; add fixture.

---

## 6. EXTRAPOLATE (after TIMESERIES)

**AST**

- Change `TimeseriesClause` to a struct with `kind: TimeseriesKind` (Auto | Interval) and `extrapolate: bool` (default false), or add `extrapolate: bool` to a new wrapper so existing variant stays and we add the flag.

**Parser**

- In `timeseries_clause`, after parsing AUTO | interval | bare, parse optional `preceded(w, keyword("EXTRAPOLATE"))` and set extrapolate flag in the returned value.

**Tests**

- Move "EXTRAPOLATE" to success list; add fixture for `TIMESERIES 1 minute EXTRAPOLATE`.

---

## Implementation order

1. **AST first** – Add all new types/variants (SelectArg: Named, WhereCondition, TimeInterval; TimeInterval; FacetCase; FacetItem::Cases; TimeseriesClause + extrapolate). Ensure serde keeps existing fixtures deserializing.
2. **Named args** – Parser + fixture + move example to success list.
3. **WHERE condition in args** – Parser + fixtures for filter and funnel.
4. **Time interval arg** – Parser + fixture for rate.
5. **FACET CASES** – Parser + fixture.
6. **EXTRAPOLATE** – Parser + fixture.
7. **Online examples** – Remove all six from EXPECTED_FAILURES in [tests/online_examples.rs](tests/online_examples.rs); run full test suite.

## Files to touch

- [src/ast.rs](src/ast.rs) – new types and enum variants
- [src/parser.rs](src/parser.rs) – function_arg, time_interval_arg, facet_case, facet_clause branch, timeseries_clause extrapolate
- [tests/online_examples.rs](tests/online_examples.rs) – move six examples to success list
- `tests/fixtures/examples/` – new .nrql + .json pairs for each feature


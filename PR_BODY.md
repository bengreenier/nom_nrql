## Summary

Adds six NRQL features (see plan in-repo): **named function arguments** (`apdex(duration, t: 0.4)`), **`WHERE` inside function args** (e.g. `filter`, `funnel`), **time interval args** (`rate(count(*), 5 minutes)`), **`FACET CASES(...)`**, and **`TIMESERIES ... EXTRAPOLATE`**, with matching AST and fixture coverage.

## Follow-up fixes (parser + serde)

- **TIMESERIES / EXTRAPOLATE:** Avoid streaming `w()` after the timeseries kind so `complete(optional_clauses)` does not stop early; optional `EXTRAPOLATE` uses `ws_complete` + `complete(keyword)`.
- **Named args:** `named_arg_label` excludes `:` so `t: 0.4` parses correctly.
- **Nested calls:** `select_arg` parses `function_call` before `attribute_ref` so `count(*)` inside `rate(count(*), ...)` is a call, not an attribute.
- **Streaming vs complete:** `tag_complete` for `*` and comparison operators; `char_complete` for `(` / `)` / `,` in arg lists; `complete(keyword("WHERE"))` for arg-level `WHERE`.
- **Serde:** `TimeseriesClause.extrapolate` defaults for older JSON fixtures; `SelectArg` untagged order puts `Function` / `Named` before `Attribute` so JSON round-trips match the parser.

## Commits

| Commit | Description |
|--------|-------------|
| `6a77ef5` | Add support for 6 NRQL advanced features |
| `658a7fe` | fix: parser and serde for advanced NRQL (timeseries, nested calls, fixtures) |
| `8f3e464` | chore: remove unused nom streaming tag import |

## Testing

`cargo test` — lib tests, `online_examples`, and `parse_fixtures` all pass.

//! Streaming NRQL parser using nom streaming combinators.

use crate::ast::*;
use crate::error::ParseError;
use crate::lexer;
use nom::branch::alt;
use tracing::instrument;
use nom::bytes::streaming::tag;
use nom::bytes::complete::tag as tag_complete;
use nom::character::streaming::char;
use nom::combinator::{complete, map, map_res, opt, peek, recognize, value};
use nom::multi::{many_m_n, separated_list0};
use nom::sequence::{delimited, pair, preceded, tuple};
use nom::Parser;
use nom::IResult;

type Res<'a, O> = IResult<&'a str, O, nom::error::Error<&'a str>>;

fn w(i: &str) -> Res<&str> {
    lexer::ws(i)
}

/// FROM clause: FROM EventType ( , EventType )*
#[instrument(skip(i), fields(len = i.len()))]
fn from_clause(i: &str) -> Res<FromClause> {
    let (i, _) = lexer::keyword("FROM").parse(i)?;
    let (i, _) = w(i)?;
    let (i, first) = lexer::attr_or_ident(i)?;
    // Use complete() so trailing newline after first event type yields Error (no comma) not Incomplete
    let (i, rest) = nom::multi::many0(preceded(
        complete(tuple((lexer::ws_complete, char(','), lexer::ws_complete))),
        lexer::attr_or_ident,
    )).parse(i)?;
    let mut event_types = vec![first];
    event_types.extend(rest);
    Ok((i, FromClause { event_types }))
}

/// Literal: string | number | true | false | null
#[instrument(skip(i), fields(len = i.len()))]
fn literal(i: &str) -> Res<Literal> {
    alt((
        map(lexer::string_literal, Literal::String),
        map(null_kw, |_| Literal::Null),
        map(lexer::keyword("true"), |_| Literal::Bool(true)),
        map(lexer::keyword("false"), |_| Literal::Bool(false)),
        map(number_literal, Literal::Number),
    )).parse(i)
}

fn null_kw(i: &str) -> Res<()> {
    let (i, _) = lexer::keyword("null").parse(i)?;
    Ok((i, ()))
}

fn number_literal(i: &str) -> Res<NumberLiteral> {
    let (i, s) = lexer::number_str(i)?;
    let s = s.trim();
    if let Ok(n) = s.parse::<i64>() {
        return Ok((i, NumberLiteral::Int(n)));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Ok((i, NumberLiteral::Float(f)));
    }
    Err(nom::Err::Error(nom::error::Error::new(i, nom::error::ErrorKind::MapRes)))
}

/// AttributeRef
fn attribute_ref(i: &str) -> Res<AttributeRef> {
    map(lexer::attr_or_ident, |name| AttributeRef { name }).parse(i)
}

/// SelectArg: * | literal | attribute | function_call (recursive)
fn select_arg(i: &str) -> Res<SelectArg> {
    let (i, _) = w(i)?;
    alt((
        value(SelectArg::Wildcard, preceded(w, tag("*"))),
        map(literal, SelectArg::Literal),
        map(attribute_ref, SelectArg::Attribute),
        map(function_call, SelectArg::Function),
    )).parse(i)
}

/// TimeInterval: number time_unit (no "ago")
fn time_interval_arg(i: &str) -> Res<SelectArg> {
    let (i, _) = w(i)?;
    map(
        pair(lexer::number_str, time_unit),
        |(s, unit)| {
            let n: u64 = s.trim().parse().unwrap_or(0);
            SelectArg::TimeInterval(TimeInterval { n, unit })
        },
    ).parse(i)
}

/// FunctionArg: named arg | WHERE condition | time interval | select_arg
fn function_arg(i: &str) -> Res<SelectArg> {
    let (i, _) = w(i)?;
    alt((
        // Named argument: ident ':' select_arg
        map(
            tuple((
                map(lexer::identifier, |s: &str| s.to_string()),
                preceded(w, char(':')),
                preceded(w, select_arg),
            )),
            |(name, _, value)| SelectArg::Named {
                name,
                value: Box::new(value),
            },
        ),
        // WHERE condition: WHERE condition
        map(
            preceded(lexer::keyword("WHERE"), preceded(w, condition)),
            SelectArg::WhereCondition,
        ),
        // Time interval: number time_unit (try before literal/attribute so "5 minutes" is one arg)
        time_interval_arg,
        // Fallback to regular select_arg
        select_arg,
    )).parse(i)
}

/// FunctionCall: Ident ( ( * | ArgList ) )
#[instrument(skip(i), fields(len = i.len()))]
fn function_call(i: &str) -> Res<FunctionCall> {
    let (i, _) = w(i)?;
    let (i, name) = map(lexer::identifier, |s: &str| s.to_string()).parse(i)?;
    let (i, _) = w(i)?;
    let (i, args) = delimited(
        char('('),
        alt((
            value(vec![SelectArg::Wildcard], preceded(w, tag("*"))),
            separated_list0(preceded(w, char(',')), function_arg),
        )),
        char(')'),
    ).parse(i)?;
    Ok((i, FunctionCall { name, args }))
}

/// SelectItem: * | ( AttributeRef | FunctionCall ) ( AS string )?
fn select_item(i: &str) -> Res<SelectItem> {
    let (i, _) = w(i)?;
    alt((
        value(SelectItem::Wildcard, preceded(w, tag("*"))),
        map(
            tuple((
                alt((
                    map(attribute_ref, SelectItem::Attr),
                    map(
                        tuple((
                            map(lexer::identifier, |s: &str| s.to_string()),
                            delimited(
                                char('('),
                                alt((
                                    value(vec![SelectArg::Wildcard], preceded(w, tag("*"))),
                                    separated_list0(preceded(w, char(',')), function_arg),
                                )),
                                char(')'),
                            ),
                            opt(preceded(
                                tuple((w, lexer::keyword("AS"), w)),
                                lexer::string_literal,
                            )),
                        )),
                        |(name, args, alias)| SelectItem::Function {
                            name,
                            args,
                            alias,
                        },
                    ),
                )),
                opt(preceded(
                    tuple((w, lexer::keyword("AS"), w)),
                    lexer::string_literal,
                )),
            )),
            |(mut item, alias_opt)| {
                if let SelectItem::Function { alias, .. } = &mut item {
                    *alias = alias_opt;
                }
                item
            },
        ),
    )).parse(i)
}

#[instrument(skip(i), fields(len = i.len()))]
fn select_item_inner(i: &str) -> Res<SelectItem> {
    let (i, _) = w(i)?;
    alt((
        value(SelectItem::Wildcard, preceded(w, tag("*"))),
        // Try function before attribute so "count(*)" is not parsed as attr "count"
        map(
            tuple((
                map(lexer::identifier, |s: &str| s.to_string()),
                delimited(
                    char('('),
                    alt((
                        value(vec![SelectArg::Wildcard], preceded(w, tag("*"))),
                        separated_list0(preceded(w, char(',')), function_arg),
                    )),
                    char(')'),
                ),
                opt(preceded(
                    tuple((w, lexer::keyword("AS"), w)),
                    lexer::string_literal,
                )),
            )),
            |(name, args, alias)| SelectItem::Function {
                name,
                args,
                alias,
            },
        ),
        map(attribute_ref, SelectItem::Attr),
    )).parse(i)
}

/// SelectClause: SELECT ( * | SelectList )
#[instrument(skip(i), fields(len = i.len()))]
fn select_clause(i: &str) -> Res<SelectClause> {
    let (i, _) = lexer::keyword("SELECT").parse(i)?;
    let (i, items) = preceded(
        lexer::ws_complete,
        alt((
            value(vec![SelectItem::Wildcard], preceded(lexer::ws_complete, tag_complete("*"))),
            separated_list0(
                complete(preceded(lexer::ws_complete, char(','))),
                complete(select_item_inner),
            ),
        )),
    ).parse(i)?;
    Ok((i, SelectClause { items }))
}

/// WHERE clause
#[instrument(skip(i), fields(len = i.len()))]
fn condition(i: &str) -> Res<Condition> {
    let (i, _) = w(i)?;
    let (i, attribute) = attribute_ref(i)?;
    let (i, _) = w(i)?;
    let (i, op) = comparison_op(i)?;
    let (i, values) = opt(preceded(w, value_list_or_single)).parse(i)?;
    Ok((
        i,
        Condition {
            attribute,
            op,
            values: values.flatten(),
        },
    ))
}

fn value_list_or_single(i: &str) -> Res<Option<Vec<Literal>>> {
    alt((
        map(
            delimited(
                preceded(w, char('(')),
                separated_list0(preceded(w, char(',')), preceded(w, literal)),
                preceded(w, char(')')),
            ),
            Some,
        ),
        map(literal, |l| Some(vec![l])),
    )).parse(i)
}

fn comparison_op(i: &str) -> Res<ComparisonOp> {
    let (i, _) = w(i)?;
    alt((
        value(ComparisonOp::Eq, tag("=")),
        value(ComparisonOp::Ne, tag("!=")),
        value(ComparisonOp::Ge, tag(">=")),
        value(ComparisonOp::Gt, tag(">")),
        value(ComparisonOp::Le, tag("<=")),
        value(ComparisonOp::Lt, tag("<")),
        value(
            ComparisonOp::IsNotNull,
            tuple((lexer::keyword("IS"), w, lexer::keyword("NOT"), w, lexer::keyword("NULL"))),
        ),
        value(
            ComparisonOp::IsNull,
            tuple((lexer::keyword("IS"), w, lexer::keyword("NULL"))),
        ),
        value(
            ComparisonOp::NotLike,
            tuple((lexer::keyword("NOT"), w, lexer::keyword("LIKE"))),
        ),
        value(ComparisonOp::Like, lexer::keyword("LIKE")),
        value(
            ComparisonOp::NotIn,
            tuple((lexer::keyword("NOT"), w, lexer::keyword("IN"))),
        ),
        value(ComparisonOp::In, lexer::keyword("IN")),
    )).parse(i)
}

#[instrument(skip(i), fields(len = i.len()))]
fn where_clause(i: &str) -> Res<WhereClause> {
    let (i, _) = lexer::keyword("WHERE").parse(i)?;
    let (i, first) = condition(i)?;
    let (i, rest) = nom::multi::many0(preceded(
        tuple((
            w,
            alt((
                lexer::keyword("AND"),
                lexer::keyword("OR"),
            )),
        )),
        condition,
    )).parse(i)?;
    let mut conditions = vec![first];
    conditions.extend(rest);
    Ok((i, WhereClause { conditions }))
}

#[instrument(skip(i), fields(len = i.len()))]
fn limit_clause(i: &str) -> Res<u64> {
    let (i, _) = lexer::keyword("LIMIT").parse(i)?;
    let (i, _) = w(i)?;
    let (i, s) = lexer::number_str(i)?;
    let n: u64 = s.trim().parse().map_err(|_| nom::Err::Error(nom::error::Error::new(i, nom::error::ErrorKind::MapRes)))?;
    Ok((i, n))
}

#[instrument(skip(i), fields(len = i.len()))]
fn offset_clause(i: &str) -> Res<u64> {
    let (i, _) = lexer::keyword("OFFSET").parse(i)?;
    let (i, _) = w(i)?;
    let (i, s) = lexer::number_str(i)?;
    let n: u64 = s.trim().parse().map_err(|_| nom::Err::Error(nom::error::Error::new(i, nom::error::ErrorKind::MapRes)))?;
    Ok((i, n))
}

fn time_unit(i: &str) -> Res<TimeUnit> {
    let (i, _) = w(i)?;
    // Plural before singular so "days" matches before "day"
    alt((
        value(TimeUnit::Millisecond, lexer::keyword("milliseconds")),
        value(TimeUnit::Millisecond, lexer::keyword("millisecond")),
        value(TimeUnit::Second, lexer::keyword("seconds")),
        value(TimeUnit::Second, lexer::keyword("second")),
        value(TimeUnit::Minute, lexer::keyword("minutes")),
        value(TimeUnit::Minute, lexer::keyword("minute")),
        value(TimeUnit::Hour, lexer::keyword("hours")),
        value(TimeUnit::Hour, lexer::keyword("hour")),
        value(TimeUnit::Day, lexer::keyword("days")),
        value(TimeUnit::Day, lexer::keyword("day")),
        value(TimeUnit::Week, lexer::keyword("weeks")),
        value(TimeUnit::Week, lexer::keyword("week")),
        value(TimeUnit::Month, lexer::keyword("months")),
        value(TimeUnit::Month, lexer::keyword("month")),
        value(TimeUnit::Quarter, lexer::keyword("quarters")),
        value(TimeUnit::Quarter, lexer::keyword("quarter")),
        value(TimeUnit::Year, lexer::keyword("years")),
        value(TimeUnit::Year, lexer::keyword("year")),
    )).parse(i)
}

/// Epoch milliseconds are 13 digits (until year 2286). Only try UnixMillis when we see 13+ digits
/// so "1700000000000 UNTIL" parses correctly and "1 day ago" is not consumed by this branch.
fn unix_millis_13plus(i: &str) -> Res<TimeExpr> {
    map_res(
        recognize(many_m_n(
            13,
            64,
            nom::character::streaming::satisfy(|c| c.is_ascii_digit()),
        )),
        |s: &str| s.parse::<u64>().map(|n| TimeExpr::UnixMillis { value: n }),
    )
    .parse(i)
}

#[instrument(skip(i), fields(len = i.len()))]
fn time_expr_fixed(i: &str) -> Res<TimeExpr> {
    let (i, _) = w(i)?;
    alt((
        value(TimeExpr::Now, lexer::keyword("NOW")),
        unix_millis_13plus,
        map(
            tuple((
                lexer::number_str,
                time_unit,
                preceded(w, lexer::keyword("ago")),
            )),
            |(s, unit, _)| {
                let n: u64 = s.trim().parse().unwrap_or(0);
                TimeExpr::Relative { n, unit }
            },
        ),
        map(lexer::string_literal, |s: String| TimeExpr::Absolute { value: s }),
        map(
            nom::combinator::map_res(lexer::number_str, |s: &str| s.trim().parse::<u64>()),
            |n| TimeExpr::UnixMillis { value: n },
        ),
    )).parse(i)
}

#[instrument(skip(i), fields(len = i.len()))]
fn since_clause(i: &str) -> Res<TimeExpr> {
    let (i, _) = lexer::keyword("SINCE").parse(i)?;
    time_expr_fixed(i)
}

#[instrument(skip(i), fields(len = i.len()))]
fn until_clause(i: &str) -> Res<TimeExpr> {
    let (i, _) = lexer::keyword("UNTIL").parse(i)?;
    time_expr_fixed(i)
}

#[instrument(skip(i), fields(len = i.len()))]
fn timeseries_clause(i: &str) -> Res<TimeseriesClause> {
    let (i, _) = lexer::keyword("TIMESERIES").parse(i)?;
    let (i, _) = w(i)?;
    // Bare TIMESERIES (no AUTO or interval) is valid in NRQL and means auto bucketing. Try interval before success so "TIMESERIES 1 hour" parses.
    let (i, kind) = alt((
        value(TimeseriesKind::Auto, lexer::keyword("AUTO")),
        map(
            pair(lexer::number_str, time_unit),
            |(s, unit)| {
                let n: u64 = s.trim().parse().unwrap_or(0);
                TimeseriesKind::Interval { n, unit }
            },
        ),
        value(TimeseriesKind::Auto, nom::combinator::success(())),
    )).parse(i)?;
    let (i, _) = w(i)?;
    let (i, extrapolate) = opt(preceded(w, lexer::keyword("EXTRAPOLATE"))).parse(i)?;
    Ok((
        i,
        TimeseriesClause {
            kind,
            extrapolate: extrapolate.is_some(),
        },
    ))
}

#[instrument(skip(i), fields(len = i.len()))]
fn facet_case(i: &str) -> Res<FacetCase> {
    let (i, _) = w(i)?;
    let (i, _) = lexer::keyword("WHERE").parse(i)?;
    let (i, condition) = preceded(w, condition).parse(i)?;
    let (i, alias) = opt(preceded(
        tuple((w, lexer::keyword("AS"), w)),
        lexer::string_literal,
    )).parse(i)?;
    Ok((
        i,
        FacetCase {
            condition,
            alias,
        },
    ))
}

#[instrument(skip(i), fields(len = i.len()))]
fn facet_item(i: &str) -> Res<FacetItem> {
    let (i, _) = w(i)?;
    // Try function before attribute so FACET buckets(duration, 400, 10) parses correctly
    alt((
        map(function_call, FacetItem::Function),
        map(attribute_ref, FacetItem::Attr),
    )).parse(i)
}

#[instrument(skip(i), fields(len = i.len()))]
fn order_by_item(i: &str) -> Res<OrderByItem> {
    let (i, _) = w(i)?;
    // Try function before attribute so ORDER BY count(*) DESC parses correctly
    let (i, attr_or_fn) = alt((
        map(function_call, EitherAttrOrFunction::Function),
        map(attribute_ref, EitherAttrOrFunction::Attr),
    )).parse(i)?;
    let (i, direction) = opt(preceded(w, alt((
        value(OrderDirection::Asc, lexer::keyword("ASC")),
        value(OrderDirection::Desc, lexer::keyword("DESC")),
    )))).parse(i)?;
    Ok((
        i,
        OrderByItem {
            attribute_or_function: attr_or_fn,
            direction,
        },
    ))
}

#[instrument(skip(i), fields(len = i.len()))]
fn order_by_clause(i: &str) -> Res<OrderByClause> {
    let (i, _) = lexer::keyword("ORDER").parse(i)?;
    let (i, _) = w(i)?;
    let (i, _) = lexer::keyword("BY").parse(i)?;
    let (i, items) = separated_list0(preceded(w, char(',')), order_by_item).parse(i)?;
    let (i, limit) = opt(preceded(w, limit_clause)).parse(i)?;
    Ok((i, OrderByClause { items, limit }))
}

#[instrument(skip(i), fields(len = i.len()))]
fn facet_clause(i: &str) -> Res<FacetClause> {
    let (i, _) = lexer::keyword("FACET").parse(i)?;
    let (i, _) = w(i)?;
    // Try FACET CASES(...) before regular facet items
    let (i, attributes) = alt((
        map(
            preceded(
                lexer::keyword("CASES"),
                delimited(
                    preceded(w, char('(')),
                    separated_list0(preceded(w, char(',')), facet_case),
                    preceded(w, char(')')),
                ),
            ),
            |cases| vec![FacetItem::Cases(cases)],
        ),
        separated_list0(preceded(w, char(',')), facet_item),
    )).parse(i)?;
    let (i, order_by) = opt(preceded(w, order_by_clause)).parse(i)?;
    Ok((
        i,
        FacetClause {
            attributes,
            order_by,
        },
    ))
}

#[instrument(skip(i), fields(len = i.len()))]
fn with_timezone_clause(i: &str) -> Res<String> {
    let (i, _) = lexer::keyword("WITH").parse(i)?;
    let (i, _) = w(i)?;
    let (i, _) = lexer::keyword("TIMEZONE").parse(i)?;
    let (i, _) = w(i)?;
    alt((
        lexer::string_literal,
        map(lexer::identifier, |s: &str| s.to_string()),
    )).parse(i)
}

#[instrument(skip(i), fields(len = i.len()))]
fn compare_with_clause(i: &str) -> Res<TimeExpr> {
    let (i, _) = lexer::keyword("COMPARE").parse(i)?;
    let (i, _) = w(i)?;
    let (i, _) = lexer::keyword("WITH").parse(i)?;
    time_expr_fixed(i)
}

enum QueryMod {
    Where(WhereClause),
    Facet(FacetClause),
    Limit(u64),
    Offset(u64),
    Since(TimeExpr),
    Until(TimeExpr),
    Timeseries(TimeseriesClause),
    OrderBy(OrderByClause),
    WithTimezone(String),
    CompareWith(TimeExpr),
}

#[instrument(skip(i), fields(len = i.len()))]
fn optional_clauses(i: &str) -> Res<Vec<QueryMod>> {
    // Use ws_complete so trailing newline/whitespace doesn't trigger Incomplete from streaming ws
    nom::multi::many0(preceded(lexer::ws_complete, complete(optional_clause_inner))).parse(i)
}

#[instrument(skip(i), fields(len = i.len()))]
fn optional_clause_inner(i: &str) -> Res<QueryMod> {
    alt((
        map(where_clause, QueryMod::Where),
        map(facet_clause, QueryMod::Facet),
        map(limit_clause, QueryMod::Limit),
        map(offset_clause, QueryMod::Offset),
        map(since_clause, QueryMod::Since),
        map(until_clause, QueryMod::Until),
        map(timeseries_clause, QueryMod::Timeseries),
        map(order_by_clause, QueryMod::OrderBy),
        map(with_timezone_clause, QueryMod::WithTimezone),
        map(compare_with_clause, QueryMod::CompareWith),
    )).parse(i)
}

/// Top-level: ( SELECT FROM | FROM SELECT ) OptClauses*
#[instrument(skip(i), fields(remaining_len = i.len()))]
fn query_streaming(i: &str) -> Res<Query> {
    let (i, _) = lexer::ws_complete(i)?;
    // Try FROM-first when input starts with "FROM" so "FROM X SELECT count(*)" isn't parsed as SELECT FROM, count(*).
    let (i, (select, from)) = alt((
        map(
            preceded(
                peek(lexer::keyword("FROM")),
                (complete(from_clause), preceded(lexer::ws_complete, complete(select_clause))),
            ),
            |(f, s)| (s, f),
        ),
        map(
            (complete(select_clause), preceded(lexer::ws_complete, complete(from_clause))),
            |(s, f)| (s, f),
        ),
    )).parse(i)?;
    let (i, mods) = optional_clauses(i)?;
    let mut r#where = None;
    let mut facet = None;
    let mut limit = None;
    let mut offset = None;
    let mut since = None;
    let mut until = None;
    let mut timeseries = None;
    let mut order_by = None;
    let mut with_timezone = None;
    let mut compare_with = None;
    for m in mods {
        match m {
            QueryMod::Where(w) => r#where = Some(w),
            QueryMod::Facet(f) => facet = Some(f),
            QueryMod::Limit(n) => limit = Some(n),
            QueryMod::Offset(n) => offset = Some(n),
            QueryMod::Since(t) => since = Some(t),
            QueryMod::Until(t) => until = Some(t),
            QueryMod::Timeseries(t) => timeseries = Some(t),
            QueryMod::OrderBy(o) => order_by = Some(o),
            QueryMod::WithTimezone(z) => with_timezone = Some(z),
            QueryMod::CompareWith(t) => compare_with = Some(t),
        }
    }
    Ok((
        i,
        Query {
            select,
            from,
            r#where,
            facet,
            limit,
            offset,
            since,
            until,
            timeseries,
            order_by,
            with_timezone,
            compare_with,
        },
    ))
}

/// Parse NRQL query (streaming). Call with full input for one-shot parse.
/// Appends a newline so streaming parsers see a clear end-of-input boundary.
/// Uses complete() so that Incomplete is turned into Error for full-buffer parsing.
#[instrument]
pub fn parse_nrql(input: &str) -> Result<Query, ParseError> {
    let input = format!("{}\n", input.trim_end());
    let res = complete(query_streaming).parse(&input);
    match res {
        Ok(("", q)) => Ok(q),
        Ok((rest, q)) => {
            let rest_trimmed = rest.trim();
            if rest_trimmed.is_empty() {
                Ok(q)
            } else {
                Err(ParseError::new(
                    format!("unconsumed input: {:?}", rest_trimmed),
                    Some(input.len() - rest.len()),
                ))
            }
        }
        Err(nom::Err::Incomplete(_)) => Err(ParseError::new(
            "incomplete input (streaming: more data needed)",
            None,
        )),
        Err(nom::Err::Error(e)) => Err(ParseError::new(
            format!("parse error: {:?}", e.code),
            Some(input.len().saturating_sub(e.input.len())),
        )),
        Err(nom::Err::Failure(e)) => Err(ParseError::new(
            format!("parse failure: {:?}", e.code),
            Some(input.len().saturating_sub(e.input.len())),
        )),
    }
}

//! Streaming lexer primitives for NRQL: comments, whitespace, identifiers, literals.
//! All parsers use nom's streaming modules so they return Incomplete when more input is needed.

use nom::IResult;
use nom::Parser;
use nom::branch::alt;
use nom::bytes::streaming::{tag, take_until};
use nom::character::streaming::{char, multispace0, multispace1, satisfy};
use nom::combinator::{map, opt, recognize};
use nom::multi::many_m_n;
use nom::sequence::{delimited, pair, preceded};
use tracing::instrument;

/// Skip optional whitespace (streaming). Bounded so we don't return Incomplete at EOI.
#[instrument(skip(i), fields(len = i.len()))]
pub fn ws(i: &str) -> IResult<&str, &str> {
    recognize(many_m_n(0, 4096, satisfy(char::is_whitespace))).parse(i)
}

/// Skip optional whitespace (complete mode). Use at entry when parsing a full buffer to avoid Incomplete at start.
#[instrument(skip(i), fields(len = i.len()))]
pub fn ws_complete(i: &str) -> IResult<&str, &str> {
    nom::character::complete::multispace0(i)
}

/// Skip one or more whitespace (streaming).
pub fn ws1(i: &str) -> IResult<&str, &str> {
    multispace1(i)
}

/// Skip a line comment: // ... or -- ...
fn line_comment(i: &str) -> IResult<&str, ()> {
    let (i, _) = alt((
        preceded(tag("//"), take_until("\n")),
        preceded(tag("--"), take_until("\n")),
    ))
    .parse(i)?;
    Ok((i, ()))
}

/// Skip a block comment: /* ... */
fn block_comment(i: &str) -> IResult<&str, ()> {
    let (i, _) = delimited(tag("/*"), take_until("*/"), tag("*/")).parse(i)?;
    Ok((i, ()))
}

/// Skip a single comment (line or block). Returns Incomplete if at end of buffer.
pub fn comment(i: &str) -> IResult<&str, ()> {
    alt((line_comment, block_comment)).parse(i)
}

/// Skip zero or more comments and whitespace (streaming).
/// May return Incomplete if in the middle of a comment.
pub fn skip_comments_and_ws(i: &str) -> IResult<&str, ()> {
    let mut input = i;
    loop {
        let (rest, _) = multispace0(input)?;
        input = rest;
        match opt(comment).parse(input) {
            Ok((rest, Some(_))) => input = rest,
            Ok((_rest, None)) => return Ok((input, ())),
            Err(e) => return Err(e),
        }
    }
}

/// Parse an identifier: letter then alphanumeric, _, :, or .
/// Does not handle backticks (use backtick_ident for that).
fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.'
}

fn ident_first_char(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Unquoted identifier (at least one character, first must be letter or _).
/// Uses many_m_n so streaming parser has an upper bound and won't return Incomplete at EOI.
#[instrument(skip(i), fields(len = i.len()))]
pub fn identifier(i: &str) -> IResult<&str, &str> {
    recognize(pair(
        satisfy(ident_first_char),
        many_m_n(0, 512, satisfy(ident_char)),
    ))
    .parse(i)
}

/// Backtick-quoted identifier: `...` (content can contain spaces).
pub fn backtick_ident(i: &str) -> IResult<&str, &str> {
    delimited(char('`'), take_until("`"), char('`')).parse(i)
}

/// Attribute ref or event type: either identifier or backtick_ident.
#[instrument(skip(i), fields(len = i.len()))]
pub fn attr_or_ident(i: &str) -> IResult<&str, String> {
    let (i, s) = alt((
        map(backtick_ident, |s: &str| s.to_string()),
        map(identifier, |s: &str| s.to_string()),
    ))
    .parse(i)?;
    Ok((i, s))
}

/// Single-quoted string literal. Escapes: '' for single quote (per common SQL).
#[instrument(skip(i), fields(len = i.len()))]
pub fn string_literal(i: &str) -> IResult<&str, String> {
    let (i, _) = char('\'').parse(i)?;
    let mut s = String::new();
    let mut input = i;
    loop {
        let (rest, part) = take_until("'").parse(input)?;
        s.push_str(part);
        input = rest;
        if input.is_empty() {
            return Err(nom::Err::Incomplete(nom::Needed::new(1)));
        }
        let (rest, c) = nom::character::streaming::anychar.parse(input)?;
        if c == '\'' {
            if rest.starts_with('\'') {
                s.push('\'');
                input = rest.get(1..).unwrap_or("");
            } else if rest.is_empty() && part.is_empty() {
                s.push('\'');
                return Ok((rest, s));
            } else {
                return Ok((rest, s));
            }
        } else {
            s.push(c);
            input = rest;
        }
    }
}

fn number_char(c: char) -> bool {
    c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '-' || c == '+'
}

/// Returns the longest slice (1..=64 chars) that looks like a number.
#[instrument(skip(i), fields(len = i.len()))]
pub fn number_str(i: &str) -> IResult<&str, &str> {
    recognize(many_m_n(1, 64, satisfy(number_char))).parse(i)
}

/// Case-insensitive keyword: consumes the given keyword and returns ().
pub fn keyword(kw: &'static str) -> impl Fn(&str) -> IResult<&str, ()> {
    move |i: &str| {
        let (i, _) = nom::bytes::streaming::tag_no_case(kw).parse(i)?;
        Ok((i, ()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn test_identifier() {
        assert_eq!(identifier("appId "), Ok((" ", "appId")));
        assert_eq!(
            identifier("request.headers.x "),
            Ok((" ", "request.headers.x"))
        );
    }

    #[traced_test]
    #[test]
    fn test_string_literal() {
        assert_eq!(string_literal("'hello'"), Ok(("", "hello".to_string())));
        assert_eq!(string_literal("''"), Ok(("", "'".to_string())));
    }
}

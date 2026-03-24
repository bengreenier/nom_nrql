//! AST types for NRQL queries.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// A complete NRQL query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Query {
    pub select: SelectClause,
    pub from: FromClause,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#where: Option<WhereClause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facet: Option<FacetClause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<TimeExpr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<TimeExpr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeseries: Option<TimeseriesClause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<OrderByClause>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare_with: Option<TimeExpr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectClause {
    pub items: Vec<SelectItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum SelectItem {
    Wildcard,
    Attr(AttributeRef),
    Function {
        name: String,
        args: Vec<SelectArg>,
        #[serde(skip_serializing_if = "Option::is_none")]
        alias: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SelectArg {
    #[serde(alias = "null")]
    Wildcard,
    Literal(Literal),
    /// Before Attribute: untagged serde must prefer `{ name, args }` as FunctionCall, not AttributeRef with ignored `args`.
    Function(FunctionCall),
    /// Before Attribute: `{ name, value }` must not deserialize as AttributeRef (unknown fields ignored).
    Named {
        name: String,
        #[serde(rename = "value")]
        value: Box<SelectArg>,
    },
    WhereCondition(Condition),
    TimeInterval(TimeInterval),
    Attribute(AttributeRef),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: Vec<SelectArg>,
}

/// Attribute reference: identifier or backtick-quoted name (e.g. `appId`, `` `Logged-in user` ``).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeRef {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FromClause {
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhereClause {
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub attribute: AttributeRef,
    pub op: ComparisonOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<Literal>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum ComparisonOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    In,
    NotIn,
    Like,
    NotLike,
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FacetClause {
    pub attributes: Vec<FacetItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<OrderByClause>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FacetItem {
    Attr(AttributeRef),
    Function(FunctionCall),
    Cases(Vec<FacetCase>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FacetCase {
    pub condition: Condition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderByClause {
    pub items: Vec<OrderByItem>,
    /// Optional LIMIT (e.g. FACET ... ORDER BY x LIMIT 5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderByItem {
    pub attribute_or_function: EitherAttrOrFunction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<OrderDirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EitherAttrOrFunction {
    Attr(AttributeRef),
    Function(FunctionCall),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "variant")]
pub enum TimeExpr {
    Relative { n: u64, unit: TimeUnit },
    Absolute { value: String },
    UnixMillis { value: u64 },
    Now,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeInterval {
    pub n: u64,
    pub unit: TimeUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Literal {
    String(String),
    Number(NumberLiteral),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumberLiteral {
    Int(i64),
    Float(f64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeseriesClause {
    #[serde(flatten)]
    pub kind: TimeseriesKind,
    #[serde(default, skip_serializing_if = "is_false")]
    pub extrapolate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "variant")]
pub enum TimeseriesKind {
    Auto,
    Interval { n: u64, unit: TimeUnit },
}

fn is_false(b: &bool) -> bool {
    !b
}

//! NRQL (New Relic Query Language) streaming parser built with nom.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;

pub use ast::*;
pub use error::ParseError;
pub use parser::parse_nrql;

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn parse_minimal_from_select() {
        let q = parse_nrql("FROM Transaction SELECT *").unwrap();
        assert_eq!(q.from.event_types, ["Transaction"]);
        assert!(matches!(&q.select.items[0], SelectItem::Wildcard));
    }
}

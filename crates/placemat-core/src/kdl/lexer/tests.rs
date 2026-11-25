//! The forked lexer: kdlite's defects are errors, and the deny lints stay in place.

use super::{Error, Event, Parser, Value};
use std::borrow::Cow;

fn events(text: &str) -> Result<Vec<Event<'_>>, Error> {
    Parser::new(text).collect()
}

#[test]
fn short_raw_string_closers_are_errors_not_panics() {
    for text in ["a #\"x\"", "a ##\"x\"#", "a #\"\"\"\nx\n"] {
        assert!(events(text).is_err(), "{text:?} parsed");
    }
}

#[test]
fn the_fork_keeps_its_deny_lints() {
    let source = include_str!("../lexer.rs");
    for lint in [
        "indexing_slicing",
        "string_slice",
        "dbg_macro",
        "print_stdout",
        "print_stderr",
        "arithmetic_side_effects",
    ] {
        assert!(
            source.contains(&format!("clippy::{lint}")),
            "lexer.rs lost deny({lint})"
        );
    }
}

fn first_value(text: &str) -> Result<Value<'_>, Error> {
    for event in Parser::new(text) {
        if let Event::Entry { value, .. } = event? {
            return Ok(value);
        }
    }
    Err(Error::UnexpectedEof(text.len()))
}

#[test]
fn events_carry_byte_offsets() {
    let want = vec![
        Event::Node {
            ty: None,
            name: Cow::Borrowed("a"),
            start: 0,
        },
        Event::Entry {
            key: None,
            ty: None,
            value: Value::Number("1"),
            start: 2,
        },
        Event::Entry {
            key: Some(Cow::Borrowed("k")),
            ty: None,
            value: Value::String(Cow::Borrowed("v")),
            start: 4,
        },
        Event::Begin { offset: 10 },
        Event::Node {
            ty: None,
            name: Cow::Borrowed("b"),
            start: 14,
        },
        Event::End { offset: 16 },
    ];
    assert_eq!(events("a 1 k=\"v\" {\n  b\n}").unwrap(), want);
}

#[test]
fn an_argument_starts_after_its_type_annotation() {
    let want = Event::Entry {
        key: None,
        ty: Some(Cow::Borrowed("u")),
        value: Value::Number("1"),
        start: 5,
    };
    assert_eq!(events("n (u)1").unwrap().last(), Some(&want));
}

#[test]
fn eof_errors_carry_the_text_length() {
    assert_eq!(events("a #\"x\"").unwrap_err(), Error::UnexpectedEof(6));
    assert_eq!(events("a ##\"x\"#").unwrap_err(), Error::UnexpectedEof(8));
    let unterminated = "a #\"\"\"\nx\n";
    assert_eq!(
        events(unterminated).unwrap_err(),
        Error::UnexpectedEof(unterminated.len())
    );
}

#[test]
fn numbers_and_float_keywords_keep_their_source_text() {
    let wide = "9".repeat(50);
    let hex = format!("0x{}", "f".repeat(34));
    for text in [
        wide.as_str(),
        hex.as_str(),
        "#inf",
        "#-inf",
        "#nan",
        "+0b1_0",
        "1_000",
    ] {
        assert_eq!(
            first_value(&format!("n {text}")),
            Ok(Value::Number(text)),
            "{text}"
        );
    }
    assert_eq!(first_value("n #true"), Ok(Value::Bool(true)));
    assert_eq!(first_value("n #null"), Ok(Value::Null));
}

#[test]
fn number_syntax_follows_the_grammar() {
    for text in [
        "0", "-0", "+7", "1_000", "1_", "1__0", "1.5", "1.5_0", "1e5", "1E-5", "1.5e+5", "1e1_0",
        "0x1_F", "0xff", "-0o7", "+0b1_0",
    ] {
        assert_eq!(
            first_value(&format!("n {text}")),
            Ok(Value::Number(text)),
            "{text} should be valid"
        );
    }
    for text in [
        "1.", "1.e5", ".5", "-.5", "1e", "1e+", "0x", "0x_1", "1.2.3", "1e5e3", "0b12", "1._5",
        "1-2", "0o8", "0X10", "1abc", "1e5.0",
    ] {
        assert_eq!(
            first_value(&format!("n {text}")),
            Err(Error::InvalidNumber(2)),
            "{text} should be invalid"
        );
    }
    assert_eq!(
        first_value("n +-1"),
        Ok(Value::String(Cow::Borrowed("+-1"))),
        "a sign then a non-digit is an identifier"
    );
}

#[test]
fn a_second_child_block_is_reported_at_its_brace() {
    assert_eq!(events("a {} {}").unwrap_err(), Error::MultipleChildren(5));
    assert_eq!(events("a {}   {}").unwrap_err(), Error::MultipleChildren(7));
}

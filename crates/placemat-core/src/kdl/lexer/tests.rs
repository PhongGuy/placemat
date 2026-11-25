//! The forked lexer: kdlite's defects are errors, and the deny lints stay in place.

use super::{Error, Event, Parser};

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

//! Forms where kdl 6.7.1 and kdlite disagreed, each settled by KDL 2.0.0's grammar.
use placemat_core::kdl::{ReadLimits, Value, parse};

fn names(text: &str) -> Vec<String> {
    let nodes =
        parse(text, &ReadLimits::default()).unwrap_or_else(|e| panic!("{text:?} failed: {e}"));
    nodes.iter().map(|n| n.name.clone()).collect()
}

fn refused(text: &str) {
    assert!(
        parse(text, &ReadLimits::default()).is_err(),
        "{text:?} parsed"
    );
}

#[test]
fn children_need_a_space_before_the_brace() {
    refused("a{}");
    refused("a{b}");
    assert_eq!(names("a {b}"), ["a"]);
}

#[test]
fn banned_code_points_are_refused_inside_comments() {
    for text in [
        "/* \u{1} */ n",
        "/* \u{200E} */ n",
        "// \u{200E}\nn",
        "/* \u{FEFF} */ n",
        "// \u{7F}\nn",
    ] {
        refused(text);
    }
}

#[test]
fn raw_strings_follow_the_quote_rules() {
    refused("n ###\"\"\"##\"###");
    refused("n #\"\"\"one line\"\"\"#");
    assert_eq!(names("n ###\"\"#\"##\"###"), ["n"]);
}

#[test]
fn a_slashdashed_node_may_end_with_a_semicolon() {
    assert!(names("/-n;").is_empty(), "nothing left");
    assert_eq!(names("/-n;m"), ["m"]);
}

#[test]
fn a_slashdashed_node_takes_all_its_blocks_with_it() {
    assert_eq!(names("/-n /-{} {x}\nm"), ["m"]);
    assert_eq!(names("/-n {a} /-{b}\nm"), ["m"]);
    assert_eq!(names("n /-{} {x}").len(), 1);
}

#[test]
fn hex_needs_a_lowercase_prefix_and_wide_integers_survive() {
    refused("n 0X10");
    let wide = format!("n 0x{}", "f".repeat(34));
    assert_eq!(names(&wide), ["n"]);
}

#[test]
fn delete_is_refused_inside_strings() {
    refused("n \"a\u{7F}b\"");
    refused("no\u{7F}de");
}

#[test]
fn consecutive_slashdashed_nodes_with_blocks_are_valid() {
    assert!(names("/-n {a}\n/-m {b}").is_empty(), "nothing left");
    assert_eq!(names("/-n {a}\n/-m {b}\nk"), ["k"]);
    assert_eq!(names("p { /-n {a}; /-m {b} }"), ["p"]);
    assert!(names("/-n {}; /-n {}; /-n {}").is_empty(), "nothing left");
}

#[test]
fn entries_may_not_follow_a_child_block() {
    refused("n {a} x");
    refused("n {a} k=1");
    refused("/-n {a} /-x");
    refused("n {a} /-x");
}

#[test]
fn bare_float_keywords_are_not_identifiers() {
    for text in ["n inf", "n -inf", "n nan", "-inf", "n k=-inf", "n (t)-inf"] {
        refused(text);
    }
    // Only the exact words are excluded; a sign or suffix makes an ordinary identifier.
    assert_eq!(names("n +inf -nan +nan -infx"), ["n"]);
}

#[test]
fn a_line_continuation_may_end_on_crlf() {
    // `escline := '\\' ws* (single-line-comment | newline | eof)`, and CRLF is one newline.
    for text in ["n \\\r\n  k=1", "n \\ // note\r\n  k=1", "n \\\r  k=1"] {
        let nodes = parse(text, &ReadLimits::default()).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        assert_eq!(nodes.len(), 1, "{text:?}");
        assert_eq!(nodes[0].entries.len(), 1, "{text:?}");
    }
}

#[test]
fn skipped_nodes_keep_the_one_real_block_rule() {
    refused("/-n { x {a} {b} }");
    refused("n /-{ x {a} {b} }");
    refused("/-a { b { x {a} {b} } }");
    refused("n /-{ x /-{y} {z} {w} }");
    refused("n /-{a} {b} {c}");
    refused("n {a} /-{b} {c}");
}

#[test]
fn slashdashed_text_must_still_be_valid() {
    for text in [
        "n /-\"unterminated",
        "n /-k=",
        "/-n \"x",
        "p { /-n 0X1 }",
        "n /-{ x \"y }",
    ] {
        refused(text);
    }
}

#[test]
fn slashdashed_entries_drop_only_themselves() {
    let nodes = parse("n /-1 a", &ReadLimits::default()).unwrap();
    assert_eq!(nodes[0].entries.len(), 1);
    assert_eq!(nodes[0].entries[0].value, Value::String("a".into()));
    let nodes = parse("n /-k=1 k=2", &ReadLimits::default()).unwrap();
    assert_eq!(nodes[0].entries.len(), 1);
    assert_eq!(nodes[0].entries[0].key.as_deref(), Some("k"));
    assert_eq!(nodes[0].entries[0].value, Value::Number("2".into()));
}

#[test]
fn slashdashed_nodes_and_blocks_before_a_real_one_are_valid() {
    assert_eq!(names("/-n\nm"), ["m"]);
    assert_eq!(names("p { /-n; m }"), ["p"]);
    assert_eq!(names("n /-{ x } {y}"), ["n"]);
    assert!(names("/-n { x /-{ y } {z} }").is_empty(), "nothing left");
    assert!(names("/-n { x {a} /-{b} }").is_empty(), "nothing left");
    assert_eq!(names("n {a} /-{b}"), ["n"]);
    assert_eq!(names("n {a} /-{b} /-{c}"), ["n"]);
}

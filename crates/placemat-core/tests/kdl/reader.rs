//! The public tree: structure, verbatim numbers, offsets, limits and silence on stderr.

use placemat_core::kdl::{Node, ReadError, ReadErrorKind, ReadLimits, Value, line_col, parse};

fn read(text: &str) -> Vec<Node> {
    parse(text, &ReadLimits::default()).unwrap_or_else(|e| panic!("{text:?} failed: {e}"))
}

fn fail(text: &str, limits: &ReadLimits) -> (ReadErrorKind, u32) {
    match parse(text, limits) {
        Ok(nodes) => panic!("{text:?} parsed: {nodes:?}"),
        Err(ReadError { kind, offset, .. }) => (kind, offset),
    }
}

fn s(text: &str) -> Value {
    Value::String(text.to_owned())
}

fn n(text: &str) -> Value {
    Value::Number(text.to_owned())
}

#[test]
fn nodes_keep_entries_in_order_with_types_and_children() {
    let nodes =
        read("(t)parent a k=v \"b c\" k=(u)w {\n  child 1 2.5 #true #null\n}\nempty {}\nbare");
    let p = &nodes[0];
    assert_eq!((p.ty.as_deref(), p.name.as_str()), (Some("t"), "parent"));
    let entries: Vec<(Option<&str>, Option<&str>, &Value)> = p
        .entries
        .iter()
        .map(|e| (e.key.as_deref(), e.ty.as_deref(), &e.value))
        .collect();
    assert_eq!(
        entries,
        vec![
            (None, None, &s("a")),
            (Some("k"), None, &s("v")),
            (None, None, &s("b c")),
            (Some("k"), Some("u"), &s("w"))
        ]
    );
    let values: Vec<&Value> = p.children.as_ref().unwrap()[0]
        .entries
        .iter()
        .map(|e| &e.value)
        .collect();
    assert_eq!(
        values,
        [&n("1"), &n("2.5"), &Value::Bool(true), &Value::Null]
    );
    assert_eq!(nodes[1].children, Some(vec![]));
    assert_eq!(nodes[2].children, None);
}

#[test]
fn numbers_reach_the_tree_as_written() {
    let (wide, hex) = ("9".repeat(50), format!("0x{}", "f".repeat(34)));
    let nodes = read(&format!("n {wide} {hex} #inf 1_000"));
    let values: Vec<&Value> = nodes[0].entries.iter().map(|e| &e.value).collect();
    assert_eq!(values, [&n(&wide), &n(&hex), &n("#inf"), &n("1_000")]);
}

#[test]
fn starts_map_to_lines_and_columns_after_bom_crlf_and_multibyte_text() {
    let text = "\u{FEFF}a 1\r\n\"é b\" k=2 #\"r\"#=3\r\n  c {\r\n    dé\r\n  }";
    let nodes = read(text);
    let at = |offset: u32| line_col(text, offset);
    assert_eq!((nodes[0].start, at(nodes[0].start)), (3, (1, 2)));
    assert_eq!(
        (nodes[0].entries[0].start, at(nodes[0].entries[0].start)),
        (5, (1, 4))
    );
    let quoted = &nodes[1];
    assert_eq!(
        (quoted.name.as_str(), quoted.start, at(quoted.start)),
        ("é b", 8, (2, 1))
    );
    assert_eq!(
        (quoted.entries[0].start, at(quoted.entries[0].start)),
        (15, (2, 7))
    );
    assert_eq!(
        (
            quoted.entries[1].key.as_deref(),
            quoted.entries[1].start,
            at(19)
        ),
        (Some("r"), 19, (2, 11))
    );
    assert_eq!((nodes[2].start, at(nodes[2].start)), (30, (3, 3)));
    let inner = &nodes[2].children.as_ref().unwrap()[0];
    assert_eq!((inner.start, at(inner.start)), (39, (4, 5)));
}

#[test]
fn line_col_counts_kdl_newlines_and_chars() {
    assert_eq!(line_col("abc", 0), (1, 1));
    assert_eq!(line_col("é x", 3), (1, 3));
    assert_eq!(line_col("a\r\nb", 3), (2, 1));
    assert_eq!(line_col("a\u{85}b", 3), (2, 1));
    assert_eq!(line_col("ab", 99), (1, 3));
}

#[test]
fn syntax_and_eof_errors_carry_offsets() {
    let d = ReadLimits::default();
    assert_eq!(fail("a\n}", &d), (ReadErrorKind::Syntax, 2));
    assert_eq!(fail("a {\n b\n", &d), (ReadErrorKind::UnexpectedEof, 7));
    assert_eq!(fail("n \"abc", &d), (ReadErrorKind::UnexpectedEof, 6));
    assert_eq!(fail("a { } b", &d), (ReadErrorKind::Syntax, 6));
    assert_eq!(fail("a #\"x\"", &d), (ReadErrorKind::UnexpectedEof, 6));
    assert_eq!(fail("n true", &d).0, ReadErrorKind::Syntax);
}

#[test]
fn depth_is_refused_at_the_first_brace_past_the_limit() {
    let two = ReadLimits::default().with_max_depth(2);
    assert!(parse("a { b { c } }", &two).is_ok(), "depth 2 within 2");
    assert_eq!(
        fail("a { b { c { d } } }", &two),
        (ReadErrorKind::TooDeep, 10)
    );
    let zero = ReadLimits::default().with_max_depth(0);
    assert_eq!(fail("a {}", &zero), (ReadErrorKind::TooDeep, 2));
    assert!(
        parse("a /-{ b { c } }", &zero).is_ok(),
        "slashdashed blocks emit nothing"
    );
}

#[test]
fn node_entry_and_string_limits_stop_at_the_first_excess() {
    let nodes = ReadLimits::default().with_max_nodes(2);
    assert!(parse("a; b", &nodes).is_ok(), "two nodes");
    assert_eq!(fail("a; b; c", &nodes), (ReadErrorKind::TooManyNodes, 6));
    assert_eq!(fail("a { b; c }", &nodes), (ReadErrorKind::TooManyNodes, 7));
    let entries = ReadLimits::default().with_max_entries(2);
    assert!(parse("a 1 2", &entries).is_ok(), "two entries");
    assert_eq!(
        fail("a 1 2 3", &entries),
        (ReadErrorKind::TooManyEntries, 6)
    );
    assert_eq!(
        fail("a 1\nb 2 3", &entries),
        (ReadErrorKind::TooManyEntries, 8)
    );
    let strings = ReadLimits::default().with_max_string_bytes(3);
    assert!(parse("abc \"def\"", &strings).is_ok(), "three bytes");
    assert_eq!(fail("abcd", &strings), (ReadErrorKind::StringTooLong, 0));
    assert_eq!(
        fail("a k=\"éé\"", &strings),
        (ReadErrorKind::StringTooLong, 2)
    );
    assert_eq!(fail("a 12345", &strings), (ReadErrorKind::StringTooLong, 2));
    assert_eq!(
        fail("a long=1", &strings),
        (ReadErrorKind::StringTooLong, 2)
    );
}

#[test]
fn default_limits_and_error_text() {
    let d = ReadLimits::default();
    assert_eq!(
        (d.max_depth, d.max_nodes, d.max_entries, d.max_string_bytes),
        (64, 1_000_000, 10_000_000, 16 << 20)
    );
    assert_eq!(
        parse("a {", &d).unwrap_err().to_string(),
        "unexpected end of input at byte 3"
    );
}

#[test]
fn child_parses_a_multi_line_string() {
    let nodes = read("n \"\"\"\n    SECRET-LINE-7f3a\n    \"\"\"");
    assert_eq!(nodes[0].entries[0].value, s("SECRET-LINE-7f3a"));
}

// The child runs in its own process so its stderr can be read; kdlite printed it with dbg!.
#[test]
fn reading_a_multi_line_string_writes_nothing_to_stderr() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "reader::child_parses_a_multi_line_string",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("SECRET-LINE"),
        "the reader wrote user text to stderr: {stderr}"
    );
}

//! A seeded generator of valid KDL 2.0.0 documents that avoid the forms where parsers disagree.
use fastrand::Rng;

// No letters spelling a keyword (true, false, null, inf, nan), and no digit, sign or dot first.
const IDENT_FIRST: &[char] = &[
    'a', 'b', 'c', 'x', 'y', 'z', '_', 'é', 'ß', '~', '$', '<', '>', '|', '^', '@', '!', '?',
];
const IDENT_REST: &[char] = &['a', 'z', '0', '7', '-', '.', '_', 'é', '+', '\''];
// Every newline in the KDL 2.0.0 table, CRLF included.
const NEWLINES: &[&str] = &[
    "\n", "\r\n", "\r", "\u{85}", "\u{b}", "\u{c}", "\u{2028}", "\u{2029}",
];

fn pick<T: Copy>(rng: &mut Rng, items: &[T]) -> T {
    items[rng.usize(..items.len())]
}

fn repeat(rng: &mut Rng, max: usize, pieces: &[&str], out: &mut String) {
    for _ in 0..rng.usize(0..max) {
        out.push_str(pick(rng, pieces));
    }
}

fn newline(rng: &mut Rng) -> &'static str {
    if rng.bool() {
        "\n"
    } else {
        pick(rng, NEWLINES)
    }
}

fn ident(rng: &mut Rng) -> String {
    // Signed and dotted identifiers put a non-digit after the sign or dot, so they never read
    // as numbers; the first letter never starts a keyword, so `-inf` cannot appear.
    let mut s = String::from(pick(rng, &["", "", "", "", "-", "+", ".", "-.", "+."]));
    // A lone sign or dot (`-`, `+.`) is an identifier too, but a digit may not follow it.
    if !s.is_empty() && rng.usize(..4) == 0 {
        return s;
    }
    s.push(pick(rng, IDENT_FIRST));
    for _ in 0..rng.usize(0..6) {
        s.push(pick(rng, IDENT_REST));
    }
    s
}

fn quoted(rng: &mut Rng) -> String {
    let mut s = String::from("\"");
    repeat(
        rng,
        8,
        &[
            "a",
            "Z",
            " ",
            "é",
            "}",
            "{",
            "=",
            "#",
            "\\n",
            "\\t",
            "\\r",
            "\\b",
            "\\f",
            "\\\"",
            "\\\\",
            "\\u{e9}",
            "\\u{1F600}",
            "\\s",
            "\\   \n  ",
            "\\\u{2028}\t",
        ],
        &mut s,
    );
    s.push('"');
    s
}

fn raw(rng: &mut Rng) -> String {
    let hashes = "#".repeat(rng.usize(1..4));
    // With two or more hashes a body may hold `"#`, which a one-hash string would end on.
    let quote_hash: &[&str] = if hashes.len() > 1 { &["\"#"] } else { &[] };
    if rng.usize(..4) == 0 {
        let mut s = format!("{hashes}\"\"\"{}", newline(rng));
        for _ in 0..rng.usize(1..4) {
            s.push('a');
            repeat(rng, 5, &["a", " ", "\"", "\\", "\\n", "é"], &mut s);
            s.extend(quote_hash.iter().take(rng.usize(..2)).copied());
            s.push_str(newline(rng));
        }
        return format!("{s}\"\"\"{hashes}");
    }
    let mut body = String::from("a");
    repeat(rng, 8, &["a", " ", "\"", "\\", "}", "\\n", "é"], &mut body);
    body.extend(quote_hash.iter().take(rng.usize(..2)).copied());
    format!("{hashes}\"{body}\"{hashes}")
}

fn multiline(rng: &mut Rng) -> String {
    let indent = pick(rng, &["", "  ", "\t"]);
    let mut s = format!("\"\"\"{}", newline(rng));
    for _ in 0..rng.usize(1..4) {
        s.push_str(indent);
        s.push('a');
        repeat(
            rng,
            5,
            &["a", " ", "é", "}", "\\n", "\\\"", "\\r", "\\b", "\\f"],
            &mut s,
        );
        s.push_str(newline(rng));
    }
    s.push_str(indent);
    s.push_str("\"\"\"");
    s
}

fn integer(rng: &mut Rng) -> String {
    // Up to the full i128 range, except i128::MIN, which kdl refuses.
    let v = match rng.usize(..8) {
        0 => i128::from(i64::MAX),
        1 => i128::from(i64::MIN),
        2 => i128::MAX,
        3 => i128::MIN + 1,
        4 => rng.i128(i128::MIN + 1..),
        _ => rng.i128(-100_000..100_000),
    };
    let sign = if v < 0 {
        "-"
    } else if rng.usize(..5) == 0 {
        "+"
    } else {
        ""
    };
    let m = v.unsigned_abs();
    let (prefix, digits) = match rng.usize(..4) {
        0 => ("0x", format!("{m:x}")),
        1 => ("0o", format!("{m:o}")),
        2 => ("0b", format!("{m:b}")),
        _ => ("", m.to_string()),
    };
    let digits = if digits.len() > 1 && rng.bool() {
        format!("{}_{}", &digits[..1], &digits[1..])
    } else {
        digits
    };
    format!("{sign}{prefix}{digits}")
}

fn float(rng: &mut Rng) -> String {
    let (sign, int, frac, exp) = (
        pick(rng, &["", "-", "+"]),
        rng.u32(..10_000),
        rng.u32(..1000),
        rng.u32(..30),
    );
    // Underscores may follow any digit of an integer part, fraction or exponent.
    let int = if int > 9 && rng.bool() {
        format!("{int}_")
    } else {
        int.to_string()
    };
    let frac = if rng.usize(..4) == 0 {
        format!("{frac}_0")
    } else {
        frac.to_string()
    };
    let e = format!("{}{}", pick(rng, &["e", "E"]), pick(rng, &["", "+", "-"]));
    match rng.usize(..3) {
        0 => format!("{sign}{int}.{frac}"),
        1 => format!("{sign}{int}.{frac}{e}{exp}"),
        _ => format!("{sign}{int}{e}{exp}"),
    }
}

fn value(rng: &mut Rng) -> String {
    match rng.usize(..9) {
        0 => ident(rng),
        1 => quoted(rng),
        2 => raw(rng),
        3 => multiline(rng),
        4 | 5 => integer(rng),
        6 => float(rng),
        7 => pick(rng, &["#true", "#false", "#null"]).to_owned(),
        _ => pick(rng, &["#inf", "#-inf", "#nan"]).to_owned(),
    }
}

fn ty(rng: &mut Rng) -> String {
    if rng.usize(..5) != 0 {
        return String::new();
    }
    let name = match rng.usize(..4) {
        0 => quoted(rng),
        1 => raw(rng),
        _ => ident(rng),
    };
    // Space may sit inside the parentheses and between the annotation and what it types.
    let pad = ["", " ", "\t", " /* t */ "];
    let (open, close, after) = (pick(rng, &pad), pick(rng, &pad), pick(rng, &pad));
    format!("({open}{name}{close}){after}")
}

fn entry(rng: &mut Rng) -> String {
    let value = format!("{}{}", ty(rng), value(rng));
    if rng.bool() {
        return value;
    }
    // A few fixed keys so duplicate properties occur.
    let key = if rng.usize(..4) == 0 {
        quoted(rng)
    } else {
        pick(rng, &["k", "v", "w", "é", "-k", ".k", "+.k"]).to_owned()
    };
    format!("{key}={value}")
}

fn node(rng: &mut Rng, depth: usize, out: &mut String) {
    let slashdashed = rng.usize(..10) == 0;
    if slashdashed {
        out.push_str("/-");
    }
    out.push_str(&ty(rng));
    out.push_str(&if rng.usize(..4) == 0 {
        quoted(rng)
    } else {
        ident(rng)
    });
    for _ in 0..rng.usize(0..5) {
        match rng.usize(..8) {
            0 => {
                out.push_str(" \\");
                out.push_str(newline(rng));
                out.push_str("  ");
            }
            1 => {
                out.push_str(" \\ // note");
                out.push_str(newline(rng));
                out.push_str("  ");
            }
            _ => out.push_str(pick(
                rng,
                &[" ", "\t", "\u{3000}", " /* c */ ", " /* a /* b */ c */ "],
            )),
        }
        if rng.usize(..8) == 0 {
            out.push_str("/-");
        }
        out.push_str(&entry(rng));
    }
    if depth < 4 && rng.usize(..3) == 0 {
        if !slashdashed && rng.usize(..5) == 0 {
            out.push_str(" /-");
            block(rng, depth, out);
        }
        out.push(' ');
        block(rng, depth, out);
    }
    // kdl refuses `;` right after a slashdashed node's block, so slashdashed nodes end on a newline.
    match rng.usize(..4) {
        0 if !slashdashed => out.push(';'),
        1 if !slashdashed => {
            out.push(';');
            out.push_str(newline(rng));
        }
        2 => {
            out.push_str(" // tail");
            out.push_str(newline(rng));
        }
        _ => out.push_str(newline(rng)),
    }
}

fn block(rng: &mut Rng, depth: usize, out: &mut String) {
    out.push('{');
    out.push_str(newline(rng));
    for _ in 0..rng.usize(0..4) {
        node(rng, depth + 1, out);
    }
    out.push('}');
}

pub fn document(rng: &mut Rng) -> String {
    let mut out = String::new();
    for _ in 0..rng.usize(1..8) {
        if rng.usize(..10) == 0 {
            out.push_str("/* a /* nested */ comment */");
            out.push_str(newline(rng));
        }
        node(rng, 0, &mut out);
    }
    out
}

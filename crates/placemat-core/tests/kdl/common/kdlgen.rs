//! A seeded generator of valid KDL 2.0.0 documents that avoid the forms where parsers disagree.
use fastrand::Rng;

// No letters spelling a keyword (true, false, null, inf, nan), and no digit, sign or dot first.
const IDENT_FIRST: &[char] = &[
    'a', 'b', 'c', 'x', 'y', 'z', '_', 'é', 'ß', '~', '$', '<', '>', '|', '^', '@', '!', '?',
];
const IDENT_REST: &[char] = &['a', 'z', '0', '7', '-', '.', '_', 'é', '+', '\''];

fn pick<T: Copy>(rng: &mut Rng, items: &[T]) -> T {
    items[rng.usize(..items.len())]
}

fn repeat(rng: &mut Rng, max: usize, pieces: &[&str], out: &mut String) {
    for _ in 0..rng.usize(0..max) {
        out.push_str(pick(rng, pieces));
    }
}

fn ident(rng: &mut Rng) -> String {
    let mut s = String::from(pick(rng, IDENT_FIRST));
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
            "\\\"",
            "\\\\",
            "\\u{e9}",
            "\\u{1F600}",
            "\\s",
            "\\   \n  ",
        ],
        &mut s,
    );
    s.push('"');
    s
}

fn raw(rng: &mut Rng) -> String {
    let mut body = String::from("a");
    repeat(rng, 8, &["a", " ", "\"", "\\", "}", "\\n", "é"], &mut body);
    format!("#\"{body}\"#")
}

fn multiline(rng: &mut Rng) -> String {
    let indent = pick(rng, &["", "  ", "\t"]);
    let mut s = String::from("\"\"\"\n");
    for _ in 0..rng.usize(1..4) {
        s.push_str(indent);
        s.push('a');
        repeat(rng, 5, &["a", " ", "é", "}", "\\n", "\\\""], &mut s);
        s.push('\n');
    }
    s.push_str(indent);
    s.push_str("\"\"\"");
    s
}

fn integer(rng: &mut Rng) -> String {
    let v = match rng.usize(..6) {
        0 => i64::MAX,
        1 => i64::MIN,
        _ => rng.i64(-100_000..100_000),
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
        rng.u32(..1000),
        rng.u32(..1000),
        rng.u32(..30),
    );
    match rng.usize(..3) {
        0 => format!("{sign}{int}.{frac}"),
        1 => format!("{sign}{int}.{frac}e{exp}"),
        _ => format!("{sign}{int}E-{exp}"),
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
    if rng.usize(..5) == 0 {
        format!("({})", ident(rng))
    } else {
        String::new()
    }
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
        pick(rng, &["k", "v", "w", "é"]).to_owned()
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
        out.push_str(pick(
            rng,
            &[
                " ",
                "\t",
                "\u{3000}",
                " /* c */ ",
                " /* a /* b */ c */ ",
                " \\\n  ",
                " \\ // note\n  ",
            ],
        ));
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
    let ends: &[&str] = if slashdashed {
        &["\n", "\r\n", " // tail\n"]
    } else {
        &["\n", ";", "\r\n", " // tail\n", ";\n"]
    };
    out.push_str(pick(rng, ends));
}

fn block(rng: &mut Rng, depth: usize, out: &mut String) {
    out.push_str("{\n");
    for _ in 0..rng.usize(0..4) {
        node(rng, depth + 1, out);
    }
    out.push('}');
}

pub fn document(rng: &mut Rng) -> String {
    let mut out = String::new();
    for _ in 0..rng.usize(1..8) {
        if rng.usize(..10) == 0 {
            out.push_str("/* a /* nested */ comment */\n");
        }
        node(rng, 0, &mut out);
    }
    out
}

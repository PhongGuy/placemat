//! A comparison form of a KDL tree: no offsets, numbers canonicalised the way kdl stores them.
use kdl::{KdlDocument, KdlNode, KdlValue};
use placemat_core::kdl::{Node, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum PValue {
    String(String),
    Number(String),
    Bool(bool),
    Null,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PEntry {
    pub key: Option<String>,
    pub ty: Option<String>,
    pub value: PValue,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PNode {
    pub ty: Option<String>,
    pub name: String,
    pub entries: Vec<PEntry>,
    pub children: Option<Vec<PNode>>,
}

fn float_text(f: f64) -> String {
    if f.is_nan() {
        "nan".to_owned()
    } else if f.is_infinite() {
        (if f > 0.0 { "inf" } else { "-inf" }).to_owned()
    } else {
        format!("{f:e}")
    }
}

// Integers as decimal i128 (wider ones tagged so they never equal kdl), floats via float_text.
pub fn canonical_number(text: &str) -> String {
    match text {
        "#inf" => return "inf".into(),
        "#-inf" => return "-inf".into(),
        "#nan" => return "nan".into(),
        _ => {}
    }
    let clean: String = text.chars().filter(|&c| c != '_').collect();
    let (negative, body) = match clean.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, clean.strip_prefix('+').unwrap_or(&clean)),
    };
    let radix = [("0x", 16), ("0o", 8), ("0b", 2)]
        .into_iter()
        .find_map(|(p, r)| body.strip_prefix(p).map(|d| (d, r)));
    let (digits, radix) = match radix {
        Some(found) => found,
        None if body.contains(['.', 'e', 'E']) => {
            return float_text(
                clean
                    .trim_start_matches('+')
                    .parse()
                    .unwrap_or_else(|_| panic!("not a float: {text}")),
            );
        }
        None => (body, 10),
    };
    let signed = if negative {
        format!("-{digits}")
    } else {
        digits.to_owned()
    };
    i128::from_str_radix(&signed, radix)
        .map_or_else(|_| format!("beyond-i128:{text}"), |v| v.to_string())
}

pub fn from_ours(nodes: &[Node]) -> Vec<PNode> {
    nodes
        .iter()
        .map(|n| PNode {
            ty: n.ty.clone(),
            name: n.name.clone(),
            entries: n
                .entries
                .iter()
                .map(|e| PEntry {
                    key: e.key.clone(),
                    ty: e.ty.clone(),
                    value: our_value(&e.value),
                })
                .collect(),
            children: n.children.as_deref().map(from_ours),
        })
        .collect()
}

fn our_value(value: &Value) -> PValue {
    match value {
        Value::String(s) => PValue::String(s.clone()),
        Value::Number(t) => PValue::Number(canonical_number(t)),
        Value::Bool(b) => PValue::Bool(*b),
        Value::Null => PValue::Null,
        _ => unreachable!("Value gained a variant; teach the comparison form about it"),
    }
}

pub fn from_kdl(doc: &KdlDocument) -> Vec<PNode> {
    doc.nodes().iter().map(kdl_node).collect()
}

fn kdl_node(n: &KdlNode) -> PNode {
    let id = |i: &kdl::KdlIdentifier| i.value().to_owned();
    PNode {
        ty: n.ty().map(id),
        name: id(n.name()),
        entries: n
            .entries()
            .iter()
            .map(|e| PEntry {
                key: e.name().map(id),
                ty: e.ty().map(id),
                value: kdl_value(e.value()),
            })
            .collect(),
        children: n.children().map(from_kdl),
    }
}

fn kdl_value(value: &KdlValue) -> PValue {
    match value {
        KdlValue::String(s) => PValue::String(s.clone()),
        KdlValue::Integer(i) => PValue::Number(i.to_string()),
        KdlValue::Float(f) => PValue::Number(float_text(*f)),
        KdlValue::Bool(b) => PValue::Bool(*b),
        KdlValue::Null => PValue::Null,
    }
}

// kdl-rs's expected-output rules: arguments in order, then properties with the rightmost
// duplicate kept and sorted by key, and an empty child block written as none.
pub fn normalise(nodes: Vec<PNode>) -> Vec<PNode> {
    nodes
        .into_iter()
        .map(|node| {
            let (mut entries, props): (Vec<PEntry>, Vec<PEntry>) =
                node.entries.into_iter().partition(|e| e.key.is_none());
            let mut kept: Vec<PEntry> = Vec::new();
            for prop in props {
                kept.retain(|p| p.key != prop.key);
                kept.push(prop);
            }
            kept.sort_by(|a, b| a.key.cmp(&b.key));
            entries.extend(kept);
            PNode {
                entries,
                children: node.children.map(normalise).filter(|c| !c.is_empty()),
                ..node
            }
        })
        .collect()
}

//! Load-time repair: later duplicate ids are renamed and arrow ends bound to a missing id are
//! freed, both deterministically so every open of the same bytes yields the same board.

use std::collections::{BTreeMap, BTreeSet};

use super::load::{End, Mapped, Warning};
use super::zkey::BASE62;
use super::{ArrowEnd, COORD_MAX, ElementId, Kind, Point};

/// Repairs the file-order mapping in place and returns its warnings: every `DuplicateId` in
/// file order, then every `DanglingBinding` in file order, `From` before `To`. `unknown_keys`
/// are the unknown top-level nodes' keys with their position among the top-level nodes, the
/// same positions `Mapped::index` counts.
pub(crate) fn repair(mapped: &mut [Mapped], unknown_keys: &[(String, usize)]) -> Vec<Warning> {
    let mut warnings = rename_duplicates(mapped, unknown_keys);
    warnings.extend(free_dangling_ends(mapped, unknown_keys));
    warnings
}

/// Walks every id claim in file order; the first claimant keeps an id and a later element
/// repeating it is renamed. Unknown nodes are never renamed, they only claim.
fn rename_duplicates(mapped: &mut [Mapped], unknown_keys: &[(String, usize)]) -> Vec<Warning> {
    // A rename must not collide with any id in the file, including ones that appear later.
    let mut taken: BTreeSet<String> = unknown_keys.iter().map(|(k, _)| k.clone()).collect();
    taken.extend(mapped.iter().map(|m| m.element.id.as_str().to_owned()));
    let mut claimed = BTreeSet::new();
    let mut keys = unknown_keys.iter().peekable();
    let mut warnings = Vec::new();
    for m in mapped.iter_mut() {
        while let Some((key, _)) = keys.next_if(|(_, at)| *at < m.index) {
            claimed.insert(key.clone());
        }
        let original = m.element.id.clone();
        if claimed.insert(original.as_str().to_owned()) {
            continue;
        }
        let renamed = (1_u64..)
            .filter_map(|n| ElementId::new(&renamed_id(original.as_str(), n)))
            .find(|id| !taken.contains(id.as_str()));
        let Some(renamed) = renamed else { continue };
        taken.insert(renamed.as_str().to_owned());
        claimed.insert(renamed.as_str().to_owned());
        m.element.id = renamed.clone();
        warnings.push(Warning::DuplicateId { original, renamed });
    }
    warnings
}

/// The `n`th rename candidate for `id`: FNV-1a-64 over `id`, a NUL and `n` in decimal,
/// spelled in base62, most significant digit first.
fn renamed_id(id: &str, n: u64) -> String {
    let bytes = id.bytes().chain([0]).chain(n.to_string().into_bytes());
    let mut hash = bytes.fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    let mut digits: Vec<u8> = Vec::new();
    loop {
        digits.extend(usize::try_from(hash % 62).ok().and_then(|i| BASE62.get(i)));
        hash /= 62;
        if hash == 0 {
            break;
        }
    }
    digits.iter().rev().map(|&b| char::from(b)).collect()
}

/// Frees every arrow end bound to an id no element or unknown node holds, placing it at the
/// other end's original point. Every replacement is figured before any end changes, so an
/// arrow whose two ends both dangle collapses to the origin.
fn free_dangling_ends(mapped: &mut [Mapped], unknown_keys: &[(String, usize)]) -> Vec<Warning> {
    let by_id: BTreeMap<&str, &Kind> = mapped
        .iter()
        .map(|m| (m.element.id.as_str(), &m.element.kind))
        .collect();
    let keys: BTreeSet<&str> = unknown_keys.iter().map(|(k, _)| k.as_str()).collect();
    let live = |id: &ElementId| by_id.contains_key(id.as_str()) || keys.contains(id.as_str());
    let dangling = |end: &ArrowEnd| match end {
        ArrowEnd::Bound { target, .. } if !live(target) => Some(target.clone()),
        ArrowEnd::Bound { .. } | ArrowEnd::Free(_) => None,
    };
    let mut fixes = Vec::new();
    for (i, m) in mapped.iter().enumerate() {
        let Kind::Arrow(a) = &m.element.kind else {
            continue;
        };
        for (end, this, other) in [(End::From, &a.from, &a.to), (End::To, &a.to, &a.from)] {
            if let Some(target) = dangling(this) {
                let at = match dangling(other) {
                    Some(_) => Point::new(0, 0),
                    None => point_of(other, &by_id),
                };
                fixes.push((i, end, target, at));
            }
        }
    }
    let mut warnings = Vec::new();
    for (i, end, target, at) in fixes {
        let Some(m) = mapped.get_mut(i) else { continue };
        if let Kind::Arrow(a) = &mut m.element.kind {
            match end {
                End::From => a.from = ArrowEnd::Free(at),
                End::To => a.to = ArrowEnd::Free(at),
            }
        }
        warnings.push(Warning::DanglingBinding {
            arrow: m.element.id.clone(),
            end,
            target,
        });
    }
    warnings
}

/// Where a live end sits: a free end's point; a boxed target's anchor on its box, floored; a
/// line, freedraw or text target's origin; the origin for an arrow or unknown-node target.
/// Clamped into the coordinate range, since a box's far edge can lie past it.
fn point_of(end: &ArrowEnd, by_id: &BTreeMap<&str, &Kind>) -> Point {
    let (x, y) = match end {
        ArrowEnd::Free(p) => (i64::from(p.x), i64::from(p.y)),
        ArrowEnd::Bound { target, anchor } => match by_id.get(target.as_str()) {
            Some(Kind::Rect(b) | Kind::Ellipse(b) | Kind::Diamond(b)) => {
                let along = |at: i32, size: i32, f: u16| {
                    i64::from(at) + (i64::from(size) * i64::from(f)).div_euclid(10_000)
                };
                (
                    along(b.at.x, b.size.w, anchor.x),
                    along(b.at.y, b.size.h, anchor.y),
                )
            }
            Some(Kind::Line(p) | Kind::Freedraw(p)) => (p.at.x.into(), p.at.y.into()),
            Some(Kind::Text(t)) => (t.at.x.into(), t.at.y.into()),
            Some(Kind::Arrow(_)) | None => (0, 0),
        },
    };
    let clamp = |v: i64| {
        let max = i64::from(COORD_MAX);
        i32::try_from(v.clamp(-max, max)).unwrap_or_default()
    };
    Point::new(clamp(x), clamp(y))
}

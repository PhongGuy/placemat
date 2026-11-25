//! Paint order and the generated ids and seeds.

use std::collections::BTreeSet;

use placemat_core::board::{Board, Rng, Seed};

use crate::util::{rect, z};

#[test]
fn paint_order_sorts_by_z_then_id() {
    let mut b = Board::new();
    for (i, k) in [("c", "a2"), ("b", "a1"), ("a", "a2"), ("d", "Z")] {
        let mut e = rect(i, 0, 0);
        e.z = z(k);
        b.insert(e).unwrap();
    }
    let order: Vec<&str> = b.paint_order().map(|e| e.id.as_str()).collect();
    assert_eq!(order, ["d", "b", "a", "c"]);
}

#[test]
fn seeded_rngs_repeat_and_ids_are_8_base62_chars() {
    let b = Board::new();
    let (mut r1, mut r2) = (Rng::seeded(7), Rng::seeded(7));
    let mut high_seed = false;
    for _ in 0..100 {
        let (a, c) = (r1.element_id(&b), r2.element_id(&b));
        assert_eq!(a, c);
        assert_eq!(a.as_str().len(), 8, "{a}");
        assert!(
            a.as_str().bytes().all(|ch| ch.is_ascii_alphanumeric()),
            "{a}"
        );
        let (s1, s2) = (r1.seed(), r2.seed());
        assert_eq!(s1, s2);
        assert!(s1.get() <= Seed::MAX, "{s1:?}");
        high_seed |= s1.get() > Seed::MAX / 2;
    }
    assert!(
        high_seed,
        "100 seeds never reached the upper half of the range"
    );
}

#[test]
fn generated_ids_skip_ids_already_on_the_board() {
    let first = Rng::seeded(11).element_id(&Board::new());
    let mut b = Board::new();
    b.insert(rect(first.as_str(), 0, 0)).unwrap();
    let again = Rng::seeded(11).element_id(&b);
    assert_ne!(again, first);
    assert_eq!(again.as_str().len(), 8);
}

#[test]
fn entropy_rngs_differ() {
    let b = Board::new();
    let ids: BTreeSet<_> = (0..50)
        .map(|_| Rng::from_entropy().element_id(&b))
        .collect();
    assert!(ids.len() > 1, "50 entropy-seeded rngs gave one id");
}

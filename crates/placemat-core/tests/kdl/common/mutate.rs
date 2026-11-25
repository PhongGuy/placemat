//! Seeded char-level mutations of KDL text; working on chars keeps every result valid UTF-8.
use fastrand::Rng;

const PIECES: &[char] = &[
    '{', '}', '"', '#', '/', '\\', '-', '*', '=', ';', '(', ')', '\n', '\r', ' ', '0', 'x', '.',
    'e', '+', '_', 'a', 'é', '\u{7F}', '\u{FEFF}', '\u{85}',
];

pub fn mutate(rng: &mut Rng, text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    for _ in 0..rng.usize(1..4) {
        let at = rng.usize(..=chars.len());
        let piece = PIECES[rng.usize(..PIECES.len())];
        match rng.usize(..3) {
            0 => chars.insert(at, piece),
            1 if at < chars.len() => {
                chars.remove(at);
            }
            _ if at < chars.len() => chars[at] = piece,
            _ => chars.push(piece),
        }
    }
    chars.into_iter().collect()
}

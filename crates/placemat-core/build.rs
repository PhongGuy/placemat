//! Refuses a release build whose compiled-in update keys are the placeholder or empty, so no
//! shipped binary can trust nothing. The release workflow sets PLACEMAT_RELEASE_BUILD=1.

const KEYS: &str = "../../assets/update/keys.txt";
const PLACEHOLDER: &str = "PLACEMAT-UPDATE-KEY-PLACEHOLDER";

fn main() -> Result<(), String> {
    println!("cargo:rerun-if-changed={KEYS}");
    println!("cargo:rerun-if-env-changed=PLACEMAT_RELEASE_BUILD");
    if std::env::var("PLACEMAT_RELEASE_BUILD").as_deref() != Ok("1") {
        return Ok(());
    }
    let text = std::fs::read_to_string(KEYS).map_err(|e| format!("reading {KEYS}: {e}"))?;
    let keys: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    if keys.contains(&PLACEHOLDER) {
        return Err(format!(
            "{KEYS} still holds the placeholder key; see RELEASING.md, Keys"
        ));
    }
    if keys.is_empty() {
        return Err(format!(
            "{KEYS} holds no update key; see RELEASING.md, Keys"
        ));
    }
    Ok(())
}

//! The words of the update notice.

use super::Notice;
use crate::viewer::notice::clean;

/// The notice's sentence for `notice`, with the version cleaned of newlines and hidden code
/// points. A how-to reason supplies its own clause; no reason means the release's format
/// cannot replace this install.
pub fn notice_text(notice: &Notice<'_>) -> String {
    match notice {
        Notice::Offered(v) => format!("Placemat {} is available.", clean(v)),
        Notice::Downloading(v) => format!("Downloading Placemat {}\u{2026}", clean(v)),
        Notice::Ready(v) => format!("Placemat {} installs when you quit.", clean(v)),
        Notice::DownloadFailed(v) => format!("Could not download Placemat {}.", clean(v)),
        Notice::HowTo {
            version,
            reason: Some(reason),
        } => format!("Placemat {} is available, but {reason}.", clean(version)),
        Notice::HowTo {
            version,
            reason: None,
        } => format!(
            "Placemat {} is available; download it from the release page.",
            clean(version)
        ),
    }
}

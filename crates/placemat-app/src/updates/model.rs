//! The update notice's states as plain Rust, generic over the download payload `A` and the
//! staged file `S` so tests can drive it without core's opaque types.

use std::mem;

use placemat_core::update::{CheckError, Format, InstallKind, Reason};

/// A release a check offered, paired with what Install downloads for it.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer<A> {
    version: String,
    format: Format,
    payload: A,
}

impl<A> Offer<A> {
    /// An offer of `version`, packaged as `format`, downloaded through `payload`.
    pub fn new(version: impl Into<String>, format: Format, payload: A) -> Self {
        Self {
            version: version.into(),
            format,
            payload,
        }
    }

    /// The offered version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// How the offered release is packaged.
    pub fn format(&self) -> Format {
        self.format
    }
}

/// What the update notice shows, borrowed from the model.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice<'a> {
    /// This version can be installed.
    Offered(&'a str),
    /// This version is downloading.
    Downloading(&'a str),
    /// This version is staged and installs at quit.
    Ready(&'a str),
    /// This version failed to download.
    DownloadFailed(&'a str),
    /// This version exists but the user installs it by hand.
    HowTo {
        /// The offered version.
        version: &'a str,
        /// Why the updater leaves this install alone, or `None` when the release's format
        /// cannot replace it.
        reason: Option<Reason>,
    },
}

/// A staged install for quit to swap in.
#[non_exhaustive]
#[derive(Debug)]
pub struct Armed<S> {
    /// The staged replacement.
    pub staged: S,
    /// Whether the user asked to relaunch after the install.
    pub restart: bool,
}

/// Where the notice stands. Offered covers the how-to too: the install kind decides which
/// one it shows, so a refused offer never downloads.
#[derive(Debug)]
enum State<A, S> {
    Idle,
    Offered(Offer<A>),
    Downloading(Offer<A>),
    Ready {
        version: String,
        staged: S,
        restart: bool,
    },
    DownloadFailed(String),
}

/// The update notice's state for one session.
#[derive(Debug)]
pub struct UpdateModel<A, S> {
    kind: InstallKind,
    state: State<A, S>,
    dismissed: bool,
}

impl<A, S> UpdateModel<A, S> {
    /// A model for an app installed as `kind`, showing nothing until a check finds a release.
    pub fn new(kind: InstallKind) -> Self {
        Self {
            kind,
            state: State::Idle,
            dismissed: false,
        }
    }

    /// How the running app is installed.
    pub fn kind(&self) -> &InstallKind {
        &self.kind
    }

    /// Whether a tick should check the feed: not while downloading, once staged, or after
    /// the user dismissed the notice.
    pub fn should_check(&self) -> bool {
        !self.dismissed && !matches!(self.state, State::Downloading(_) | State::Ready { .. })
    }

    /// Records a check result. Only a found release changes anything, and only while the
    /// notice is idle or still offering, so a newer release replaces the shown offer.
    pub fn checked(&mut self, result: Result<Option<Offer<A>>, CheckError>) {
        let Ok(Some(offer)) = result else { return };
        if !self.dismissed && matches!(self.state, State::Idle | State::Offered(_)) {
            self.state = State::Offered(offer);
        }
    }

    /// What the notice shows, or `None` while idle or dismissed.
    pub fn notice(&self) -> Option<Notice<'_>> {
        if self.dismissed {
            return None;
        }
        match &self.state {
            State::Idle => None,
            State::Offered(offer) if self.kind.accepts(offer.format) => {
                Some(Notice::Offered(&offer.version))
            }
            State::Offered(offer) => Some(Notice::HowTo {
                version: &offer.version,
                reason: self.reason(),
            }),
            State::Downloading(offer) => Some(Notice::Downloading(&offer.version)),
            State::Ready { version, .. } => Some(Notice::Ready(version)),
            State::DownloadFailed(version) => Some(Notice::DownloadFailed(version)),
        }
    }

    /// Hides the notice for the rest of the session; a staged install stays armed.
    pub fn later(&mut self) {
        self.dismissed = true;
    }

    /// Moves an installable offer to Downloading and returns its payload; `None` when there
    /// is nothing this install may download.
    pub fn start_download(&mut self) -> Option<&A> {
        let State::Offered(offer) = &self.state else {
            return None;
        };
        if self.dismissed || !self.kind.accepts(offer.format) {
            return None;
        }
        if let State::Offered(offer) = mem::replace(&mut self.state, State::Idle) {
            self.state = State::Downloading(offer);
        }
        if let State::Downloading(offer) = &self.state {
            Some(&offer.payload)
        } else {
            None
        }
    }

    /// Records the download's outcome: the staged file arms the install, `None` shows the
    /// failure. Ignored unless downloading.
    pub fn downloaded(&mut self, staged: Option<S>) {
        let State::Downloading(offer) = &self.state else {
            return;
        };
        let version = offer.version.clone();
        self.state = match staged {
            Some(staged) => State::Ready {
                version,
                staged,
                restart: false,
            },
            None => State::DownloadFailed(version),
        };
    }

    /// Asks quit to relaunch after the install; false unless an update is staged.
    pub fn restart_now(&mut self) -> bool {
        if let State::Ready { restart, .. } = &mut self.state {
            *restart = true;
            true
        } else {
            false
        }
    }

    /// Hands the staged install to quit, once; `None` unless an update is staged.
    pub fn take_armed(&mut self) -> Option<Armed<S>> {
        if !matches!(self.state, State::Ready { .. }) {
            return None;
        }
        if let State::Ready {
            staged, restart, ..
        } = mem::replace(&mut self.state, State::Idle)
        {
            Some(Armed { staged, restart })
        } else {
            None
        }
    }

    /// The reason a not-owned install gives for leaving the update to the user.
    fn reason(&self) -> Option<Reason> {
        if let InstallKind::NotOwned(reason) = &self.kind {
            Some(*reason)
        } else {
            None
        }
    }
}

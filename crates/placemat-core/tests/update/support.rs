//! Shared helpers for the updater tests. Nothing here reaches the network, the real config or
//! the environment: keys are fixtures or generated per test, and feeds are loopback servers.

/// A well-formed minisign public key line (base64 of the key box) that signs no release.
pub const KEY_LINE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQwNzg2RTk3RUEyNjRCQzUKUldURlN5YnFsMjU0UUNFdy9pcDlBaWEyaGQ5WFd2L0tsckE2KzhQd0RFa0ZVMTJaVTllbDhMVngK";

/// The manifest target every test config uses, whatever the host.
pub const TARGET: &str = "app-linux-x86_64";

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// sha256("abc"), a well-formed digest for manifests whose artifact is never fetched.
pub const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

/// A throwaway minisign keypair, generated per test.
pub struct TestKey(minisign::KeyPair);

impl TestKey {
    pub fn generate() -> TestKey {
        TestKey(minisign::KeyPair::generate_unencrypted_keypair().unwrap())
    }

    /// The public key as a `keys.txt` line: base64 of the minisign key box.
    pub fn key_line(&self) -> String {
        STANDARD.encode(self.0.pk.to_box().unwrap().to_string())
    }

    /// A `latest.json.sig` body as the release writes it: base64 of a prehashed signature box.
    pub fn sign(&self, data: &[u8]) -> String {
        let trusted = Some("timestamp:0\tfile:latest.json");
        let sig = minisign::sign(
            Some(&self.0.pk),
            &self.0.sk,
            std::io::Cursor::new(data),
            trusted,
            None,
        );
        STANDARD.encode(sig.unwrap().to_string())
    }
}

use placemat_core::update::{CheckError, evaluate, verify_manifest};

/// Signs `manifest` with a throwaway key and evaluates the verified bytes, as `check` does.
pub fn signed_evaluate(
    manifest: &[u8],
    cfg: &UpdateConfig,
) -> Result<Option<Available>, CheckError> {
    let k = TestKey::generate();
    let sig = k.sign(manifest);
    let verified = verify_manifest(manifest, sig.as_bytes(), &[k.key_line()]).unwrap();
    assert_eq!(
        verified.as_bytes(),
        manifest,
        "verify keeps the exact buffer"
    );
    evaluate(verified, cfg)
}

/// `sig_file` with its algorithm re-tagged from prehashed `ED` to legacy `Ed`, every other
/// byte kept, so only the algorithm check can refuse it.
pub fn retag_legacy(sig_file: &str) -> String {
    let text = String::from_utf8(STANDARD.decode(sig_file.trim()).unwrap()).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut bin = STANDARD.decode(&lines[1]).unwrap();
    assert_eq!(&bin[..2], b"ED", "minisign 0.10 signs prehashed");
    bin[1] = b'd';
    lines[1] = STANDARD.encode(&bin);
    STANDARD.encode(format!("{}\n", lines.join("\n")))
}

/// A manifest offering `version` for TARGET, with unknown fields the rules must ignore.
pub fn manifest(version: &str, format: &str, url: &str, sha: &str) -> String {
    format!(
        r#"{{"version":"{version}","pub_date":"2026-11-01T12:00:00Z","extra":1,
          "targets":{{"{TARGET}":{{"url":"{url}","format":"{format}","size":3,"sha256":"{sha}","more":true}}}}}}"#
    )
}

use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the loopback server answers for one path.
#[derive(Clone)]
pub enum Reply {
    Body(Vec<u8>),
    Status(u16),
    Redirect(String),
    Slow(Duration, Vec<u8>),
    /// Close the connection without answering.
    Hangup,
}

/// One request as the server read it; header names are lowercased.
#[derive(Clone, Debug)]
pub struct Seen {
    pub path: String,
    pub query: Option<String>,
    pub headers: Vec<(String, String)>,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.headers.iter().map(|(n, _)| n.as_str()).collect();
        names.sort_unstable();
        names
    }
}

type Routes = Arc<Mutex<HashMap<String, Reply>>>;

/// An HTTP/1.1 server on 127.0.0.1 that answers each path with its `Reply` (404 when none is
/// set), one thread per connection, and records every request it reads.
pub struct Server {
    base: String,
    routes: Routes,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Server {
    pub fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let routes: Routes = Arc::default();
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let (r, s) = (Arc::clone(&routes), Arc::clone(&seen));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (r, s) = (Arc::clone(&r), Arc::clone(&s));
                std::thread::spawn(move || serve(&stream, &r, &s));
            }
        });
        Server { base, routes, seen }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn route(&self, path: &str, reply: Reply) {
        self.routes.lock().unwrap().insert(path.to_owned(), reply);
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn serve(stream: &TcpStream, routes: &Mutex<HashMap<String, Reply>>, seen: &Mutex<Vec<Seen>>) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let target = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_owned(), Some(q.to_owned())),
        None => (target, None),
    };
    let mut headers = Vec::new();
    // Read raw bytes so a non-UTF-8 line cannot end the header read, and keep a line without a
    // colon as a bare name, so every line the client sent shows in `Seen`.
    loop {
        let mut raw = Vec::new();
        if reader.read_until(b'\n', &mut raw).unwrap_or(0) == 0 {
            break;
        }
        let h = String::from_utf8_lossy(&raw);
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        match h.split_once(':') {
            Some((n, v)) => headers.push((n.trim().to_ascii_lowercase(), v.trim().to_owned())),
            None => headers.push((h.to_ascii_lowercase(), String::new())),
        }
    }
    seen.lock().unwrap().push(Seen {
        path: path.clone(),
        query,
        headers,
    });
    let reply = routes
        .lock()
        .unwrap()
        .get(&path)
        .cloned()
        .unwrap_or(Reply::Status(404));
    let mut out = stream;
    // A client that hangs up early makes the write fail, which is the client's business.
    let _ = match reply {
        Reply::Body(body) => respond(&mut out, 200, None, &body),
        Reply::Status(code) => respond(&mut out, code, None, b""),
        Reply::Redirect(to) => respond(&mut out, 302, Some(&to), b""),
        Reply::Slow(delay, body) => {
            std::thread::sleep(delay);
            respond(&mut out, 200, None, &body)
        }
        Reply::Hangup => Ok(()),
    };
}

fn respond(
    out: &mut impl Write,
    code: u16,
    location: Option<&str>,
    body: &[u8],
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {code} Test\r\ncontent-length: {}\r\nconnection: close\r\n",
        body.len()
    );
    if let Some(to) = location {
        head.push_str(&format!("location: {to}\r\n"));
    }
    head.push_str("\r\n");
    out.write_all(head.as_bytes())?;
    out.write_all(body)?;
    out.flush()
}

/// Serves `manifest` and its signature by `key` at `/latest.json` and `/latest.json.sig`.
pub fn publish(s: &Server, key: &TestKey, manifest: &str) {
    s.route("/latest.json", Reply::Body(manifest.as_bytes().to_vec()));
    s.route(
        "/latest.json.sig",
        Reply::Body(key.sign(manifest.as_bytes()).into_bytes()),
    );
}

/// A loopback port nothing listens on: bound, read, then closed.
pub fn dead_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

use placemat_core::update::{UpdateConfig, build_config_with};

/// A non-release config under the test feed override at `url`, trusting `key`.
pub fn feed_config(url: &str, key: &TestKey, current: &str) -> UpdateConfig {
    let keys = key.key_line();
    build_config_with(
        false,
        None,
        Some(url),
        Some(&keys),
        "",
        current,
        Some(TARGET),
    )
    .unwrap()
}

use placemat_core::update::{Available, Timeouts, check};
use std::path::Path;

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest as _;
    hex::encode(sha2::Sha256::digest(data))
}

/// Publishes 99.0.0 with a `format` entry at `<server>/artifact` listing `listed`'s size and
/// digest, and returns the offer, whose artifact download may take `total`.
pub fn offer_within(s: &Server, format: &str, listed: &[u8], total: Duration) -> Available {
    let k = TestKey::generate();
    let m = format!(
        r#"{{"version":"99.0.0","targets":{{"{TARGET}":{{"url":"{}","format":"{format}","size":{},"sha256":"{}"}}}}}}"#,
        s.url("/artifact"),
        listed.len(),
        sha256_hex(listed)
    );
    publish(s, &k, &m);
    let mut t = Timeouts::default();
    t.artifact = total;
    let cfg = feed_config(&s.url("/latest.json"), &k, "1.0.0").with_timeouts(t);
    check(&cfg).unwrap().expect("offered")
}

pub fn offer(s: &Server, format: &str, listed: &[u8]) -> Available {
    offer_within(s, format, listed, Duration::from_secs(15 * 60))
}

/// The sorted file names in `dir`, lock files left out.
pub fn leftovers(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| !n.ends_with(".lock"))
        .collect();
    names.sort();
    names
}

use placemat_core::update::{Claim, Staged, download};

/// Stages `body` in `dir` as the 99.0.0 `format` release, through a real check and download.
pub fn stage(dir: &Path, format: &str, body: &[u8]) -> Staged {
    let s = Server::start();
    s.route("/artifact", Reply::Body(body.to_vec()));
    let claim = Claim::new(dir).unwrap();
    download(&offer(&s, format, body), &claim, |_, _| {}).unwrap()
}

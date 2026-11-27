//! The loopback harness itself, driven by a raw socket so it is checked without the client it
//! will test.

use crate::support::{Reply, Server, TestKey, dead_port, publish};
use placemat_core::update::verify_manifest;
use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::{Duration, Instant};

// One HTTP/1.1 GET with `headers`, returning everything the server wrote before closing.
fn raw_get(s: &Server, target: &str, headers: &[&str]) -> String {
    let base = s.url("");
    let addr = base.trim_start_matches("http://");
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut request = format!("GET {target} HTTP/1.1\r\nHost: {addr}\r\n");
    for h in headers {
        request.push_str(h);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).unwrap();
    out
}

fn body(response: &str) -> &str {
    response.split_once("\r\n\r\n").unwrap().1
}

#[test]
fn server_answers_routes_and_404s_the_rest() {
    let s = Server::start();
    s.route("/latest.json", Reply::Body(b"{}".to_vec()));
    let ok = raw_get(&s, "/latest.json", &[]);
    assert!(ok.starts_with("HTTP/1.1 200 "), "{ok}");
    assert!(ok.contains("content-length: 2\r\n"), "{ok}");
    assert_eq!(body(&ok), "{}");
    let missing = raw_get(&s, "/missing", &[]);
    assert!(missing.starts_with("HTTP/1.1 404 "), "{missing}");
    s.route("/gone", Reply::Status(503));
    let gone = raw_get(&s, "/gone", &[]);
    assert!(gone.starts_with("HTTP/1.1 503 "), "{gone}");
}

#[test]
fn server_redirects_and_hangs_up() {
    let s = Server::start();
    s.route("/a", Reply::Redirect(s.url("/b")));
    let hop = raw_get(&s, "/a", &[]);
    assert!(hop.starts_with("HTTP/1.1 302 "), "{hop}");
    assert!(
        hop.contains(&format!("location: {}\r\n", s.url("/b"))),
        "{hop}"
    );
    s.route("/h", Reply::Hangup);
    assert_eq!(raw_get(&s, "/h", &[]), "");
}

#[test]
fn server_records_path_query_and_lowercased_headers() {
    let s = Server::start();
    raw_get(
        &s,
        "/latest.json?x=1",
        &["User-Agent: probe", "Accept: */*"],
    );
    let seen = s.seen();
    assert_eq!(seen.len(), 1, "{seen:?}");
    let r = &seen[0];
    assert_eq!(
        (r.path.as_str(), r.query.as_deref()),
        ("/latest.json", Some("x=1"))
    );
    assert_eq!(r.names(), ["accept", "host", "user-agent"]);
    assert_eq!(r.header("user-agent"), Some("probe"));
    assert_eq!(r.header("referer"), None);
}

#[test]
fn server_records_headers_without_a_colon_or_valid_utf8() {
    let s = Server::start();
    let addr = s.url("").trim_start_matches("http://").to_owned();
    let mut stream = TcpStream::connect(&addr).unwrap();
    let mut request = b"GET /h HTTP/1.1\r\nhost: h\r\nnocolon\r\nx-bin: ".to_vec();
    request.extend_from_slice(b"\xff\xfe\r\nx-after: 1\r\n\r\n");
    stream.write_all(&request).unwrap();
    stream.read_to_end(&mut Vec::new()).unwrap();
    let seen = s.seen();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0].names(), ["host", "nocolon", "x-after", "x-bin"]);
    assert_eq!(seen[0].header("x-after"), Some("1"));
}

#[test]
fn server_slow_reply_waits_before_answering() {
    let s = Server::start();
    s.route(
        "/slow",
        Reply::Slow(Duration::from_millis(300), b"x".to_vec()),
    );
    let start = Instant::now();
    let out = raw_get(&s, "/slow", &[]);
    assert!(
        start.elapsed() >= Duration::from_millis(300),
        "answered early"
    );
    assert_eq!(body(&out), "x");
}

#[test]
fn publish_serves_a_verifiable_signature() {
    let (s, k) = (Server::start(), TestKey::generate());
    publish(&s, &k, "{}");
    assert_eq!(body(&raw_get(&s, "/latest.json", &[])), "{}");
    let sig = raw_get(&s, "/latest.json.sig", &[]);
    assert_eq!(
        verify_manifest(b"{}", body(&sig).as_bytes(), &[k.key_line()]),
        Ok(())
    );
}

#[test]
fn dead_port_refuses_connections() {
    let got = TcpStream::connect(("127.0.0.1", dead_port()));
    assert!(got.is_err(), "something listens on the dead port: {got:?}");
}

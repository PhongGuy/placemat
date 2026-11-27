//! Shared helpers for the updater tests. Nothing here reaches the network, the real config or
//! the environment: keys are fixtures or generated per test, and feeds are loopback servers.

/// A well-formed minisign public key line (base64 of the key box) that signs no release.
pub const KEY_LINE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQwNzg2RTk3RUEyNjRCQzUKUldURlN5YnFsMjU0UUNFdy9pcDlBaWEyaGQ5WFd2L0tsckE2KzhQd0RFa0ZVMTJaVTllbDhMVngK";

/// The manifest target every test config uses, whatever the host.
pub const TARGET: &str = "app-linux-x86_64";

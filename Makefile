# One command surface for contributors and CI: `make run` opens Placemat, and `make check` is the
# full merge gate.
CARGO ?= cargo
# Build jobs for run and package, low enough to leave room for other work on the machine.
JOBS ?= 4
# Extra cargo flags for the lint and test targets; CI passes `LOCKED=--locked` so a stale
# Cargo.lock fails the gate, while a local `make check` may still refresh the lock.
LOCKED :=

.DEFAULT_GOAL := run
.PHONY: run check advisories fmt fmt-check lint test pilot-check pilot-perf icons-check notices-check script-test package clean workflows-lint

run:
	CARGO_BUILD_JOBS="$(JOBS)" $(CARGO) run -p placemat-app

check: advisories fmt-check lint test

# RustSec advisories and yanked crates in both lockfiles, policy in deny.toml. It fetches the
# advisory database each run, so it needs the network and fails loudly when the fetch fails.
advisories:
	$(CARGO) deny check advisories
	$(CARGO) deny $(PILOT) check advisories

fmt:
	$(CARGO) fmt
	$(CARGO) fmt $(PILOT)

fmt-check:
	$(CARGO) fmt --check

lint:
	$(CARGO) clippy $(LOCKED) --workspace --all-targets --all-features -- -Dwarnings

# Unit tests never open a window, so the display variables go.
test:
	env -u WAYLAND_DISPLAY -u DISPLAY $(CARGO) test $(LOCKED) --workspace

# The pilot package sits outside the workspace, with its own lock, so the gate never builds it.
PILOT := --manifest-path tools/placemat-pilot/Cargo.toml
pilot-check:
	$(CARGO) fmt --check $(PILOT)
	$(CARGO) clippy $(LOCKED) $(PILOT) --all-targets --all-features -- -Dwarnings
	$(CARGO) build $(LOCKED) -p placemat-app --bin placemat --profile timing
	env -u WAYLAND_DISPLAY -u DISPLAY $(CARGO) test $(LOCKED) $(PILOT)

# Frame cost of the board canvas on this machine's GPU, never in CI: the timing app for the
# session step, then the ignored perf scenario, which fails only on the frame gate.
pilot-perf:
	$(CARGO) build $(LOCKED) -p placemat-app --bin placemat --profile timing
	env -u WAYLAND_DISPLAY -u DISPLAY $(CARGO) test $(LOCKED) $(PILOT) --test perf -- --ignored --nocapture

icons-check:
	$(CARGO) xtask icons --check

# cargo-about runs --offline, so fetch first: a fresh CI checkout has no crate sources yet.
notices-check:
	$(CARGO) fetch --locked
	$(CARGO) xtask notices --check

# Every bash suite under scripts/tests, each in its own bash so one cannot leak state into the next.
script-test:
	@set -e; for t in scripts/tests/*-test.sh; do echo "== $$t"; bash "$$t"; done

# Linux .deb and AppImage, each verified, in target/dist/linux.
package:
	CARGO="$(CARGO)" CARGO_BUILD_JOBS="$(JOBS)" scripts/package-linux.sh

clean:
	$(CARGO) clean

workflows-lint:
	scripts/actionlint.sh

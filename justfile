default:
    @just --list

format: rust-format
    @:

lint: rust-lint shell-lint flake-check
    @:

test:
    cargo test --all

build:
    cargo build --release

rust-format:
    cargo fmt --all -- --check

rust-lint:
    cargo clippy --all-targets -- -D warnings

shell-lint:
    shfmt -i 2 -d scripts
    shellcheck scripts/*.sh

# Skipped where nix is absent, so `just ci` is runnable in a plain container.
flake-check:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v nix >/dev/null 2>&1; then
      nix flake check --no-build
    else
      echo "flake-check: nix not on PATH, skipping"
    fi

corpus:
    ./scripts/harvest-corpus.sh

ci: format lint test
    @:

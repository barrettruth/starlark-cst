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

flake-check:
    nix flake check --no-build

corpus:
    ./scripts/harvest-corpus.sh

ci: format lint test
    @:

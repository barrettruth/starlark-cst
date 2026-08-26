default:
    @just --list

format:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets -- -D warnings

test:
    cargo test --all

build:
    cargo build --release

corpus:
    ./scripts/harvest-corpus.sh

ci: format lint test
    @:

release version *args:
    nix develop .#ci --command ./scripts/release.sh {{version}} {{args}}

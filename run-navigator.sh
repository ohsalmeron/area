#!/bin/bash
# Quick script to run area-navigator

cd "$(dirname "$0")"

# Run from its own manifest path
cargo run --manifest-path crates/area-navigator/Cargo.toml "$@"


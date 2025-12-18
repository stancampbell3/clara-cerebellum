#!/bin/bash
# Build and install the CLIPS REPL

set -e

echo "Building CLIPS REPL..."
cargo build --bin clips-repl --release

echo "Installing to clips/binaries/..."
mkdir -p clips/binaries
cp target/release/clips-repl clips/binaries/clips-repl

echo ""
echo "✓ CLIPS REPL installed successfully!"
echo ""
echo "Run with: ./clips/binaries/clips-repl"
echo ""
echo "Features:"
echo "  - Full CLIPS language support"
echo "  - Callback to Rust tools via (clara-evaluate ...)"
echo "  - Interactive REPL with history"
echo "  - Built-in help and tool listing"
echo ""

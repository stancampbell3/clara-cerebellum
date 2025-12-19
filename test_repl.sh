#!/bin/bash
# Test script for CLIPS REPL with callbacks

echo "=== Testing Clara-CLIPS REPL ==="
echo ""

# Test 1: Basic CLIPS evaluation
echo "Test 1: Basic CLIPS math"
echo "(+ 1 2)" | ./clips/binaries/clips-repl 2>/dev/null | grep "CLIPS\[0\]>" -A1

echo ""

# Test 2: Echo tool callback
echo "Test 2: Callback to echo tool"
echo '(clara-evaluate "{\"tool\":\"echo\",\"arguments\":{\"message\":\"Testing callbacks!\"}}")' | ./clips/binaries/clips-repl 2>/dev/null | grep "CLIPS\[0\]>" -A1

echo ""

# Test 3: List tools
echo "Test 3: List available tools"
echo "tools" | ./clips/binaries/clips-repl 2>/dev/null | grep -A10 "Available tools:"

echo ""
echo "=== REPL is ready for interactive use ==="
echo "Run: ./clips/binaries/clips-repl"
echo "Or: cargo run --bin clips-repl"

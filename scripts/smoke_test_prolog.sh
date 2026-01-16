#!/bin/bash
# Smoke test script for LilDevils (Prolog) integration
#
# This script runs the Prolog integration tests to verify the full stack works.
# It requires SWI-Prolog to be built at SWIPL_HOME.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}  LilDevils (Prolog) Smoke Test Suite  ${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""

# Set environment variables
export SWIPL_HOME=${SWIPL_HOME:-"/mnt/vastness/home/stanc/Development/swipl/swipl-devel"}
export LD_LIBRARY_PATH="${SWIPL_HOME}/build/src:${LD_LIBRARY_PATH}"

echo -e "${YELLOW}Configuration:${NC}"
echo "  SWIPL_HOME: $SWIPL_HOME"
echo "  LD_LIBRARY_PATH: $LD_LIBRARY_PATH"
echo ""

# Check if SWI-Prolog library exists
if [ ! -f "${SWIPL_HOME}/build/src/libswipl.so" ]; then
    echo -e "${RED}ERROR: libswipl.so not found at ${SWIPL_HOME}/build/src${NC}"
    echo "Please set SWIPL_HOME to your SWI-Prolog build directory."
    exit 1
fi

echo -e "${GREEN}SWI-Prolog library found.${NC}"
echo ""

# Function to run tests
run_test() {
    local test_name=$1
    local test_filter=$2

    echo -e "${YELLOW}Running: $test_name${NC}"

    if cargo test -p $test_filter 2>&1 | tee /tmp/test_output.txt | grep -E "(PASSED|FAILED|ok|FAILED|error)"; then
        if grep -q "test result: ok" /tmp/test_output.txt; then
            echo -e "${GREEN}  PASSED${NC}"
            return 0
        else
            echo -e "${RED}  FAILED${NC}"
            return 1
        fi
    else
        echo -e "${RED}  FAILED (no output)${NC}"
        return 1
    fi
}

# Track results
PASSED=0
FAILED=0

echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}  Phase 1: clara-prolog Unit Tests     ${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""

if cargo test -p clara-prolog 2>&1; then
    echo -e "${GREEN}clara-prolog tests PASSED${NC}"
    ((PASSED++))
else
    echo -e "${RED}clara-prolog tests FAILED${NC}"
    ((FAILED++))
fi

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}  Phase 2: clara-session Prolog Tests  ${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""

if cargo test -p clara-session 2>&1; then
    echo -e "${GREEN}clara-session tests PASSED${NC}"
    ((PASSED++))
else
    echo -e "${RED}clara-session tests FAILED${NC}"
    ((FAILED++))
fi

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}  Phase 3: clara-api Devils Tests      ${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""

if cargo test -p clara-api 2>&1; then
    echo -e "${GREEN}clara-api tests PASSED${NC}"
    ((PASSED++))
else
    echo -e "${RED}clara-api tests FAILED${NC}"
    ((FAILED++))
fi

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}  Phase 4: Integration Smoke Tests     ${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""

# Run specific prolog integration tests
if cargo test --test prolog_integration_tests 2>&1; then
    echo -e "${GREEN}Prolog integration tests PASSED${NC}"
    ((PASSED++))
else
    echo -e "${RED}Prolog integration tests FAILED${NC}"
    ((FAILED++))
fi

echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}  Summary                               ${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""
echo -e "  Passed: ${GREEN}$PASSED${NC}"
echo -e "  Failed: ${RED}$FAILED${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All smoke tests PASSED!${NC}"
    echo ""
    echo "LilDevils (Prolog) integration is working correctly."
    exit 0
else
    echo -e "${RED}Some tests FAILED!${NC}"
    echo ""
    echo "Please check the output above for details."
    exit 1
fi

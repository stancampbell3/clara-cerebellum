#!/usr/bin/env bash
# Ground state for the Clara stack (capture / reset / restore / seed / verify / queue). See docs/ground_state.md.
exec python3 "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ground_state.py" "$@"

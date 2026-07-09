#!/usr/bin/env bash
# Live E2E for typed Ritual edges (docs/ritual_typed_edges.md, Verification).
#
# Prereqs (see the "Operational notes" section of that doc):
#   - Kafka broker on localhost:9094
#   - Dis (clara-api) on :8080, started with LD_LIBRARY_PATH set for libswipl
#   - one lildaemon FieryPit on :6666 with clara_mind_splinter (ollama) and
#     groq_evaluator (GROQ_API_KEY) available
#
# Usage: ./run_e2e.sh ["your query"]
# (Re-running the same query is fine: the evaluate cache is scoped per
# deduction since followup #2, so every Run re-evaluates.)
set -euo pipefail
cd "$(dirname "$0")"

BASE=${BASE:-http://localhost:6666}
E2E_USER=${E2E_USER:-typed_edges_e2e_bot}
E2E_PASS=${E2E_PASS:-typed-edges-e2e-pass-1}
QUERY=${1:-"Which chemical element has the symbol Fe? Answer in one word."}

# Account (409 if it already exists — fine) + token.
curl -s -o /dev/null -X POST "$BASE/auth/register" \
    -H 'Content-Type: application/json' \
    -d "{\"username\":\"$E2E_USER\",\"password\":\"$E2E_PASS\"}" || true
TOKEN=$(curl -s -X POST "$BASE/auth/token" \
    -d "username=$E2E_USER&password=$E2E_PASS" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["access_token"])')

# ponder_text_with_context/3 uses the process-global focused evaluator;
# without this the local side answers with the echo evaluator.
curl -s -o /dev/null -X POST "$BASE/evaluators/set" \
    -H 'Content-Type: application/json' \
    -d '{"evaluator": "clara_mind_splinter"}'

# Compose the config: inject the authored node source into the graph so
# reasoned_response.pl stays the single reviewable source of truth.
BODY=$(python3 - <<'PY'
import json
graph = json.load(open("graph_layout.json"))
graph["nodes"][0]["prologSource"] = open("reasoned_response.pl").read()
print(json.dumps({
    "name": "typed-edges-e2e",
    "evaluator": "clara_mind_splinter",
    "eval_timeout_s": 60.0,
    "kafka_bootstrap": "localhost:9094",
    "dis_url": "http://localhost:8080",
    "graph_layout": json.dumps(graph),
}))
PY
)

CID=$(curl -s -X POST "$BASE/ritual-configs" \
    -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
    -d "$BODY" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["ritual_config_id"])')
echo "ritual_config_id: $CID"

curl -s -X POST "$BASE/ritual-configs/$CID/activate" \
    -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

echo "running query: $QUERY"
curl -s -m 300 -X POST "$BASE/ritual-configs/$CID/run" \
    -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
    -d "$(python3 -c 'import json,sys; print(json.dumps({"query": sys.argv[1]}))' "$QUERY")" \
    | python3 -m json.tool

echo
echo "Cleanup reminder: deactivate the config when done —"
echo "  curl -X POST $BASE/ritual-configs/$CID/deactivate -H \"Authorization: Bearer \$TOKEN\""

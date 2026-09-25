# Manual test plan: the Clara analyst in the frontdesk demo

*Written 2026-09-25 for a hands-on session in the frontdesk POC (`http://<limbic>:8088`, ruleset **Clara (composed)**). It exercises what was built and fixed
on 2026-09-24/25: routing to the five analysts, the Edgequake baseline with citations, background deliberation/brainstorm, the Ego hand-off, UTF-8 text, the runaway
tool-loop fixes, and the Kafka reapers. Each step says what to type, what you should see, and where to look. Record results in the table at the end.*

## 0. Setup (do this first)

**Terminal 1: reset to a known baseline**

```bash
cd /mnt/moonpool/Development/clara-cerebellum
# ComfyUI must be off (it starves the GPU): nothing should be listed
pgrep -fa comfy; docker ps --format '{{.Names}}' | grep -i comfy

# clear the coupled bookkeeping left by earlier sessions (tags, topics, open queue rows), exporting it first
scripts/ground_state.sh queue reset-coupled --to /mnt/moonpool/clara-archives/queue_export_$(date +%Y%m%d_%H%M)
# delete Kafka topics no live ritual owns
python3 scripts/ground_state_kafka.py --dis http://127.0.0.1:8080 reset
# check the baseline; expect "ok": true, 14 checks
scripts/ground_state.sh verify --baseline /mnt/moonpool/clara-archives/baseline_v1 | head -5
```

If `verify` still fails on **document count** (research added documents), restore the workspace and verify again:

```bash
scripts/ground_state.sh restore --from /mnt/moonpool/clara-archives/baseline_v1 --components pg,bookkeeping
scripts/ground_state.sh verify --baseline /mnt/moonpool/clara-archives/baseline_v1 | head -5
```

**Terminal 2: watch the routes and the guards** (leave it running; this is how you see *which analyst answered*, since the UI does not show it)

```bash
docker logs -f --since 1m docker-lildaemon-1 2>&1 | grep --line-buffered -E \
  "routed to the|composed classify failed|tool loop stopped|Client disconnected|HUNG EVALUATION|Auto-cancel|participant reaper|Stopped a runaway"
```

**Terminal 3: watch the GPU** (it should go idle a little after each turn)

```bash
watch -n 5 "nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader; \
  docker exec docker-lildaemon-1 true && curl -s localhost:8080/ritual | python3 -c 'import sys,json;print(len(json.load(sys.stdin)[\"rituals\"]),\"rituals\")'"
```

**Browser:** open the frontdesk, start a **new** session, pick **Clara (composed)** from the ruleset dropdown *before* the first message. Note the time you start.

Expected timings, so you can tell slow from stuck: a plain turn 5 to 10 s; an Edgequake-grounded turn 25 to 35 s; the first turn after a restart may fail once (cold model): just retry. A deliberation or brainstorm acknowledgement is quick, the delivery comes minutes later.

---

## 1. Routing: does each kind of message reach the right analyst?

Terminal 2 prints `composed turn routed to the <analyst> analyst` for each message.

| # | Type | Expected route | Also expect |
|---|---|---|---|
| 1.1 | `good morning` | terse | short reply, no citations, no background work |
| 1.2 | `What is the capital of France?` | progressive | "Paris". May escalate (see note) |
| 1.3 | `Should we adopt DuckDB or SQLite for the session store?` | deliberative | an acknowledgement ("convening the assembly"); a decision arrives later |
| 1.4 | `Brainstorm names for a new ritual reaper` | id | an acknowledgement; ideas arrive later |
| 1.5 | `Write a small python application with three files` | **ego** | the Ego hand-off message; the gate is `deny_all`, so no file is created |
| 1.6 | `How do I create a project in Godot?` | **progressive** (a how-to question, not a build request) | a normal answer, nothing built |

**Note (1.2):** the "is pondering enough?" check is inconsistent even on trivial questions, so about half of these turns escalate to Edgequake, Groq or even a queued research task. That is a known finding, not a regression. Note which tier you seem to get (a slower reply with citations means Edgequake).

**Known routing gaps, expect these to route the "wrong" way** (they are recorded as strict expected-failures in the automated suite):

| Type | You will probably get | Would be better |
|---|---|---|
| `Tell me about rituals` | terse (short and question-free counts as chit-chat) | progressive |
| `hey, how are you?` | progressive (a question mark defeats the greeting rule) | terse |
| `How do I adopt a cat?` | deliberative (`adopt ` is a decision cue) | progressive |
| `How does the Ego gate decide between approve and escalate?` | deliberative (`decide between`) | progressive |

Try a few of your own phrasings of a build request ("Could you put together a small game for me?", "I need a script that renames files") and note which route they take: these are the cases a future fuzzy classifier would help with.

---

## 2. The Edgequake baseline: grounded answers with citations

The knowledge base holds 13 design documents. Ask (one per fresh turn):

| # | Type | Expect |
|---|---|---|
| 2.1 | `What is a Ritual in this system?` | a grounded answer; citations (dozens); mentions the ritual concept |
| 2.2 | `What does the ego gate do?` | grounded; mentions the gate and actions |
| 2.3 | `What does deadline_ms do on the deduce endpoint?` | grounded; mentions a wall-clock budget or `expired` |
| 2.4 | `What did we decide about the Kafka topic reaper?` | probably **not** in the baseline: a hedged answer, possibly a research task (see 5) |

For 2.1 to 2.3: the reply is slower (25 to 35 s) and the UI should show a citation count. If any of them answers instantly with no citations, the "pondering is enough" check accepted an ungrounded answer: note it.

---

## 3. Text integrity (the mojibake fix)

| # | Type | Expect |
|---|---|---|
| 3.1 | `Describe a sunrise in one sentence and use an em dash (—) and the word café in it, please?` | a clean em dash and an accented "é", **no** "â€"" or "Ã©" garbage |
| 3.2 | `Repeat this exactly: 日本 — naïve façade 🐙` | the characters come back unchanged (the model may paraphrase; you are checking for corruption, not obedience) |
| 3.3 | `Write a note about café prices` | routes to ego (the "write a note" cue); just confirm the message reaches the Ego intact in the log |

Any `Ã`, `â€`, or `Â` characters in a reply is a regression: copy the exact text.

---

## 4. Background work: deliberation and brainstorm

Stay in the same session for both.

| # | Do | Expect |
|---|---|---|
| 4.1 | Send `Should we keep the Kafka topic reaper on Dis or move it into each FieryPit?` | an immediate acknowledgement; **wait 2 to 6 minutes**: a deliberated decision should be pushed into the chat |
| 4.2 | Send `Brainstorm ways to make the routing smarter` | an acknowledgement; ideas delivered later |

While they run, keep sending short messages (`hello`): the assistant should stay responsive. Terminal 3 will show the GPU busy for the background legs and then idle when they finish.

If a delivery never arrives after ~10 minutes, note the session and the time. (Known issue: deleting a session fails its background rows; do not close the tab while waiting.)

---

## 5. Research task creation (an unknown topic)

Ask something the baseline cannot answer and the local model cannot invent, for example `Research the current stable release of the Godot engine and its headline features`.

Expect: a hedged best-effort answer that says it is researching, and (later) a follow-up with sourced detail. Check afterwards:

```bash
cd /mnt/moonpool/Development/clara-cerebellum
scripts/ground_state.sh queue status        # a "research" row should have appeared and moved to delivered/ready
```

This adds documents to the knowledge base: that is expected here, and step 9 cleans it up.

---

## 6. The runaway-loop fixes (the Godot incident)

**6.1 A build request no longer runs away.** Send the original request:

> `please create a small "hello universe" demo Godot gaming engine project. when run, it should display a window with the text "Hello Universe" centered in a 640x640 window in 3D text which rotates.`

Expect: route **ego**; a hand-off message; the GPU does **not** stay busy for 20 minutes; no `HUNG EVALUATION` lines in Terminal 2. Note what the Ego says (the gate is `deny_all`, so it should report that actions were refused).

**6.2 The loop guard (exploratory).** The plain chat evaluator still has its tools inside its own workspace. Try to provoke a repeat without a build cue:

> `Use your file tools to save the text "hi" into scratch.txt, then save the same text to scratch.txt again, and again, five times in total.`

Expect either a normal completion or, if the model repeats the identical call, Terminal 2 showing `tool loop stopped by the guard ... identical arguments more than 2 times` and a reply saying it stopped a runaway tool loop. Either way the GPU must go idle within a minute or two, not 20.

**6.3 Disconnect cancels work (terminal test).** A caller that gives up must not leave the model generating.

```bash
cd /mnt/moonpool/Development/lildaemon && .venv/bin/python - <<'EOF'
import httpx, time
u = "http://localhost:6666"
tok = httpx.post(f"{u}/auth/token", data={"username": "clara-functional", "password": "clara-functional-pw", "grant_type": "password"}).json()["access_token"]
h = {"Authorization": f"Bearer {tok}"}
httpx.post(f"{u}/evaluators/set", json={"evaluator": "clara_mind_splinter"}, headers=h)
try:   # a long generation, abandoned by the client after 3 seconds
    httpx.post(f"{u}/evaluate", json={"data": {"prompt": "Write a very long detailed essay of 3000 words about the Roman empire.", "model": "qwen-clara:latest"}}, headers=h, timeout=3)
except Exception as e:
    print("client gave up:", type(e).__name__)
time.sleep(6)
print("active evaluations:", httpx.get(f"{u}/evaluations/active", headers=h).json()["count"], "(expect 0)")
EOF
```

Expect Terminal 2 to print `Client disconnected during evaluation ... cancelled successfully`, `active evaluations: 0`, and the GPU to drop to idle. (Before the fix the evaluation kept running, and the count stayed at 1.)

---

## 7. Cleanup behaviour (the Kafka reapers)

| # | Do | Expect |
|---|---|---|
| 7.1 | Before you start: note the number of rituals in Terminal 3 and `docker exec docker-kafka-1 /opt/kafka/bin/kafka-topics.sh --bootstrap-server localhost:9092 --list \| grep -c ritual` | baseline counts |
| 7.2 | After sections 4 to 6, run the same two commands | more rituals/topics while sessions are active |
| 7.3 | Wait about 15 minutes after the last background work finished, then run them again | terminated rituals' topics are deleted by Dis's reaper (grace 10 minutes, sweep every 5); Dis logs `reaped topic ...` |
| 7.4 | `docker logs docker-clara-api-1 2>&1 \| grep "topic reaper" \| tail` | a `Ritual topic reaper: deleted N terminated ...` line |

---

## 8. Known issues to observe (record how they behave, do not expect them to pass)

| Issue | Try | Note |
|---|---|---|
| Date and time | `What day is it today?` | it may invent a day (there is a `get_datetime` tool it may not use): record what it says |
| Voice/persona | Compare the tone of a greeting, a factual answer, and a deliberation reply | the terse route is blunt; the shared-voice idea is deferred: note anything that jars |
| Context loss on switching | Ask a question, switch the ruleset dropdown to another analyst, then ask `What was my previous question?` | it may have lost the context window: note it |
| Cold model | Restart nothing; just note if the very first turn of the session fails | a retry should work |

---

## 9. Cleanup after the session

```bash
cd /mnt/moonpool/Development/clara-cerebellum
scripts/ground_state.sh queue reset-coupled --to /mnt/moonpool/clara-archives/queue_export_$(date +%Y%m%d_%H%M)
python3 scripts/ground_state_kafka.py --dis http://127.0.0.1:8080 reset
scripts/ground_state.sh restore --from /mnt/moonpool/clara-archives/baseline_v1 --components pg,bookkeeping   # if research added documents
scripts/ground_state.sh verify --baseline /mnt/moonpool/clara-archives/baseline_v1 | head -5                  # expect ok: true
nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader                                                   # expect ~0 %
```

---

## Appendix: seeing the full trace for a message

The frontdesk does not display the trace. To see the analyst, tier (`local` / `edgequake` / `groq` / `deferred` / `chat` / ...), citations and whether the composed path fell back, send the same text through the API:

```bash
cd /mnt/moonpool/Development/lildaemon && .venv/bin/python - <<'EOF'
import httpx
u = "http://localhost:6666"
tok = httpx.post(f"{u}/auth/token", data={"username": "clara-functional", "password": "clara-functional-pw", "grant_type": "password"}).json()["access_token"]
c = httpx.Client(base_url=u, headers={"Authorization": f"Bearer {tok}"}, timeout=240)
sid = c.post("/assistant/sessions").json()["session_id"]
c.put(f"/assistant/sessions/{sid}/ruleset", json={"ruleset_key": "clara"})
r = c.post(f"/assistant/sessions/{sid}/send", json={"text": input("message> ")}).json()
print({k: r[k] for k in ("analyst", "tier", "action_taken", "citation_count", "fallback")})
print(r["reply"][:400])
c.delete(f"/assistant/sessions/{sid}")
EOF
```

The automated equivalents live in `lildaemon/tests/functional/` (`CLARA_FUNCTIONAL=1 .venv/bin/python -m pytest -m clara_functional tests/functional`; see `clara_functional_tests.md`).

---

## Results

| Step | Pass / fail / odd | Notes (route seen, tier, timing, exact text of anything strange) |
|---|---|---|
| 1.1 to 1.6 | | |
| 2.1 to 2.4 | | |
| 3.1 to 3.3 | | |
| 4.1, 4.2 | | |
| 5 | | |
| 6.1 | | |
| 6.2 | | |
| 6.3 | | |
| 7 | | |
| 8 (each) | | |
| 9 cleanup verify | | |

Bring back: anything that failed or surprised you, with the session time so it can be matched to the logs. Feedback from Clara herself on this plan or her own answers is brainstorming input, not a decision.

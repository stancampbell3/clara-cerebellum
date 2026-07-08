# Kafka Monitor Utility

`scripts/kafka-monitor.sh`, in the standalone Kafka binary distribution at
`~/moonpool/tools/kafka_2.13-4.3.1/`, is a wrapper around the bundled
`kafka-console-consumer.sh` for quickly tailing topics on the local
`docker-kafka-1` container while developing the Feathers layer.

## Connecting to the broker

`docker-kafka-1` runs with two listeners:

```
KAFKA_ADVERTISED_LISTENERS=INTERNAL://kafka:9092,EXTERNAL://localhost:9094
```

`INTERNAL` (port 9092) is only reachable from other containers on the docker
network. From the host, use the `EXTERNAL` listener at **`localhost:9094`**
— this is the script's default, overridable with `-b`/`--bootstrap-server` or
the `KAFKA_BOOTSTRAP_SERVER` env var.

## Usage

```
scripts/kafka-monitor.sh --list                       # list topics
scripts/kafka-monitor.sh my-topic                      # tail from now
scripts/kafka-monitor.sh -f my-topic                   # from beginning
scripts/kafka-monitor.sh -k -f my-topic                # also show record keys
scripts/kafka-monitor.sh -g mygroup my-topic           # tracked consumer group
scripts/kafka-monitor.sh topic-a topic-b               # multiple topics at once
scripts/kafka-monitor.sh -f -k --json my-topic         # pretty-print JSON values
```

| Flag | Description |
|---|---|
| `-b, --bootstrap-server HOST:PORT` | broker address (default `localhost:9094`) |
| `-g, --group GROUP` | consumer group; offsets are tracked instead of always reading from the end |
| `-f, --from-beginning` | read from the start of the topic |
| `-k, --show-key` | print the record key alongside the value |
| `-j, --json` | pretty-print message values that are JSON |
| `-l, --list` | list available topics and exit |
| `-h, --help` | show usage |

Multiple topic names build a regex alternation and pass it to
`kafka-console-consumer.sh --include`, so several Feathers topics can be
watched in one terminal.

## How `--json` works

Kafka's console consumer prints one line per record, joining the
timestamp/key/value fields with a `key.separator`. When `--json` is passed,
the script:

1. Sets `key.separator` to an invisible control character (`\x1f`, ASCII Unit
   Separator) instead of the usual `" | "`, so the separator can never
   collide with characters that legitimately appear inside a JSON payload.
2. Pipes consumer output through a small `python3` filter that splits each
   line on that separator, treats the last field as the value, and — if the
   value parses as JSON — reformats it with `json.dumps(indent=2)`. The
   timestamp/key prefix is reassembled with a human-readable `" | "` for
   display.
3. Non-JSON values (or parse failures) are printed unchanged, so `--json` is
   safe to leave on even when a topic mixes JSON and plain-text messages.

Requires `python3` on `PATH`; the script exits with an error up front if
`--json` is requested and `python3` isn't found.

## Example

```
$ scripts/kafka-monitor.sh -f -k --json tail-feathers
Monitoring topic(s) [tail-feathers] on localhost:9094 (Ctrl-C to stop)...
CreateTime:1782883595811 | evaluator-42
{
  "type": "deduce",
  "roost": "roost-7",
  "payload": {
    "fact": "raven(dis, active)"
  }
}
```

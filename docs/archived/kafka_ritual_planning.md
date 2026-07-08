We're ready to start creating Rituals for our Evaluators.
A Ritual is our metaphor for coordinating work and communication among a group of Evaluators (lilDaemons).
It involves one or more Evaluators such as ClaraMindSplinterEvaluator, GroqEvaluator, etc.
In later phases, we'll add the framework for designing and implementing rule gated finite state automata fully specifying which Evaluator talks to whom, when, and under what conditions.

We'll be basing our communications framework off of Kafka (appropriate for a Dis-themed repo :0).

Let's use a Rust native kafka crate. We need distribution, replication, and an extensible raft or gossip protocol so let's go with rs kafka?

For our FieryPits (../lildaemon) which are implemented in Python we'll created producers and consumers to be made available to the GoatWrangler (if that's the right place).

clara-coire owns our local in memory and on disk duckdb database containing entries tracking our deductions' reasoning cycles (clara-cycle).

we might leverage the coire events and deduction updates to those tables to define the schemas and topics for kafka.

evaluator A running a deduction in clara-cerebrum's /deduce endpoint via clara-cycle's controller will use coire emit to write events to the topics.

evaluators participating in a ritual with evaluator A (B, C, etc.) will listen to topics corresponding to their
Ritual Id: <dis domain name>/<deduction topic>/<performance id> or something like that. performance id being unique to the Ritual and shared among the participating Evaluators.

of course we'll have to design a scaffolding for initializing the Ritual (setting up participating peer evaluators via Fiery Pit calls) and managing the producers, consumers, etc.

we can probably use one producer per clara-api/Dis server?

we'd likely tag the messages so consumers can drop expired/irrelevant messages with a timestamp and a label

we should leave room in our kafka message format for an envelope supporting encryption.

let's create a plan to introduce a new core feature to clara-cerebrum.  we're basically making coire-events (such as goals, clara_fy cache hits and queries, derived facts and rules, CLIPS fired events, etc.) available over Kafka topics to Evaluators participating in a Ritual.

clara-cerebrum : the clara-api, clara-coire, clara-cycle, etc. our symbolic logic engine server in Rust
../lildaemon : the Fiery Pit server (goat) where Evaluators run in Python

the sequence might look like:
On evaluator A living in Fiery Pit 1 and talking to the common Dis server:
/deduce request -> Dis (clara-api)
  -> Prolog engine runs, asserts a new fact
  -> CLIPS engine picks up the new fact on a production rule, firing a write to the Coire with an Offering (evaluate request) including a TTL and tagged with the Ritual Id
  -> Cycle continues.  There is an "evaluator" section of the clara-cycle controller.  this is where we'd look for messages received by our Kafka (walrus) consumer likely on a separate thread.  we'd unpack the Tephra, if it's a standard evaluation response Tephra we unpack the Hohi and push onto our own Coire.
  
On evaluator B, our consumer thread unpacks the Offering and treats it like a regular async call to its evaluate method.
We return the response by writing back to the topic corresponding to the Ritual Id.
  

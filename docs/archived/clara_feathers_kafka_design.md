Our main clara-api server is in Rust.
Individual FieryPits are currently in Python.
We'll run the kafka system independently of the Clara system, integrating it as a pluggable message layer where possible.

The following is an explanation of EXISTING CODE and implementation
----------------------------
project code locations are all within ~/Development:
- lildaemon : python, implements the FieryPit, Evaluators (lilDaemons/Devils), Fish, the Fiery Pit Protocol (REST)
	* see within that repo ./docs/fiery_pit_endpoints.md and the related special evaluate payload for "deduce" (format is shown in goat/repl/fishes/StickFish)
- clara-cerebrum/clara-api : rust, the main repo for the system running the Prolog, CLIPS endpoints, the clara-cycle deduction engine, and implementing the Demonic Voice protocol.
        * the endpoints in the clara-api are exercised by Evaluators (lilDaemon/Devils) to do their work.
- clara-cerebrum/clara-cycle
	* the deduction cycle; we may want to surface some metrics or management surrounding this
- clara-cerebrum/clara-coire
	* the in memory database is duckdb (on disk it at ./data/coire.duckdb) and maintains our "truth tableau" or current state of evaluation.
	  that state will be affected by assertions and retractions from within the clara-api session between its prolog and clips engines.
	  we will need to consider how to ensure that assertions and retractions of facts within coire can be published and consumed via the kafka "feathers" layer.
- kafka/kafka_2.13-4.2.0/
	* our kafka system, including the cluster id in the .env, no real data lives here yet.
----------------------------

Let's design a set of kafka topics and message types defining a protocol between FieryPits.

* Evaluators (lilDevils) live in FieryPits and will need to publish and subscribe to feathers (kafka messages)
* Communication directly with a FieryPit is defined by the Fiery Pit protocol as outlined in: 
  - ~/Development/lildaemon/docs/fiery_pit_endpoints.md
  - our protocol will be a kafka version of this REST protocol but with extended purpose and capability.
  - this layer will contain inter FieryPit messages of two types: 
        1. control messages associated with activities like starting a FieryPit session, setting an evaluator (lilDevil), setting a fish (StickFish for example), etc.
        2. data messages including standard FieryPit evaluate (Offering/Tephra) plus new fact and rule exchange with additional metadata (we'll leave room for cryptographic elements, etc.)
* With respect to the communication system at the Kafka messaging layer, we're dealing with Evaluator management and truth management messages.  Evaluators will be instantiated in one or more FieryPits and those Evaluators will need to find and send data messages to Evaluators participating in the same DOMAIN and in the same ROOST.  Domain is akin to an application tenant in a SaaS system.  Domains will need to be broken down into subdomains for by name routing.  We'll introduce more sophisticated routing later.  Roost is shared session membership.  Evaluators (lilDevils/lilDaemons) participating in the Kafka layer (Dis) are served by Ravens (the application agent which instantiates and handles communication through kafka).  A domain, zero or more subdomains, a roost, and a branch (per FieryPit instance) define a route.  We'll start out by spamming everybody but refine this later on.  We will group messages into two types when Ravens publish onto a topic.

* Ravens talk to each other on behalf of their lilDevil's with feathers. We'll have two types of feathers and likely two topics:
  - Flight Feathers - for control like session management
  - Tail Feathers - for "entail" heheh.  these are messages which wrap evaluate and deduce (accsessed via eval with a deduce payload) features.
  

For now, let's concentrate on defining the topics and messages for the Feathers.

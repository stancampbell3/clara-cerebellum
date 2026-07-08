We have, all projects are under ~/widebody/Development

* clara-api: implementing our API for supporting deduce queries from ClaraMindSplinter evaluator instances
* lildaemon: implements our FieryPit API and serves evaluators, fish, model interactions, etc.
* dagda's cobbler: a browser based GUI for interacting/developing evaluators, creating and testing Rituals, managing session logs and context, etc.

Documentation:
- clara-cerebellum/docs/
    * deduce_endpoint.md
    * rituals101.md
  clara-cerebellum/scripts/
    * smoke_test.sh

- lildaemon/docs/
    * fiery_pit_endpoints.md

- dagda/docs/
    * phase_d_checkpoint.md

Let's enter planning mode and extend the functionality of the system by:
- allowing entry and editing of Prolog and CLIPS source to be associated with a deduce invocation (wrapped in an Offering) made on an evaluator which supports this (child class of KindlingEvaluator).

- we'll *qualify* an initial incoming Offering to an deduction supporting evaluator (usually ClaraMindSplinter) so that the edge representing an event of Offering represents an Offering which contains a deduction request ("deduce" payload).

- there may be zero or more incoming Offerings for a Ritual

- there may be zero or more results of a Ritual of type Tephra which contain Tabu or Hohi's.

- when we draw an edge in the Ritual editor, say an incoming offering to the Ritual routed to a ClaraMindSplinter or between two nodes within a ritual, we are specifying a Prolog or CLIPS rule to be fired under some or no condition which places the Offering or Tephra on the Coire targetting one or more nodes in the Ritual and dynamic incoming declarations supporting asserting the incoming data.

* we'll plan on adding editor capabilities in Cobbler for the Prolog/CLIPS source.  later, we'll swap that out to CAWS code and derive the Prolog/CLIPS source via our Clara transduction utility.

* in the deduce request, there's room for references to Prolog and CLIPS code, but I'm not sure where that lives (if anywhere right now).

We are currently set up to either build Docker containers for the stack or start each individually via command line (using kafka in docker) to iterate more quickly on fixes.

To start the dockerized system, clara-cerebellum/scripts/docker-start.sh and docker-build.sh for compose.
For local we use clara-cerebellum/Dis script which checks and uses the dockerized kafka.  Logs will go in /tmp/dis.log.
It starts MCP servers as well.

For local FieryPit, we use light_fiery_pit.sh with logs in lildaemon/logs/bleat.log
For Cobbler, there's a script under dagda/scripts/cobbler.sh with logs in /tmp/cobbler.log

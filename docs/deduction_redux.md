Begun: claude --resume f097ee01-f88a-4c51-852d-089af0dce7f1

The edge qualification design is something we've punted on until this point.
We have been working to get an end to end system together centered on our deduction engine: Clara-api.
It's time to clarify how this whole thing is supposed to work, take stock of where we are in design and implementation, and do the final push just short of full CAWS language and a reference implementation.

* The deduction algorithm.
 - we're blending traditional expert system backward and forward chaining with LLM-mediated natural language expressed predicates which are collapse the truth function into a four valued logical system (true, false, unresolved, unknown).
 - the declarative logic controls the events which drive further reasoning either logical or LLM based
 - built in tools in Dis (clara_fy, ponder_text_with_context, etc.) use our custom LLM model to make determinations about the truth, satisfiability, confidence, suggest alternatives, etc. when reasoning.
 - evaluators provide custom access to the deduction as well as traditional prompt/chat and abstract the particular LLM or tool away from the composition of those nodes/evaluators and their communications.

* Dis coordinates activites of evaluators within Rituals and provides our custom LLM backed system for making choices about which path reasoning should take.  It also maintains traces of the reasoning process to further explainability of choices recommended or actions taken.

* The FieryPit provides a place for Evaluators to live.  Evaluators should never need to communicated amongst themselves.  They use asynchronos communication mediated by the Coire.

* The Cobbler provides developers a way of testing and configuring new custom evaluators, composing and testing Rituals, and navigating reasoning and chat session histories.

* The front desk poc demo shows one way we might integrate with a website (though it needs updating as well).  Basically, conversation is with any LLM but decisions and recommendations are delegated with context to a Clara backed system through a target predicate.  Resolving the predicate leads to a decision and possible recommendations.

During development:
Dev -> Cobbler -> Clara -> Evaluators, configurations, Rituals

It is envisioned that we train a specialized Clara LLM for consultation with developers in a "Talking Cure" phase.
During the Cure, Clara goes through a conversational exchange with the dev team and *Clara*fies the decision process under research.
The output of the Cure should be a system for making attributed and explainable decisions in the target domain and for a given subset of subject matter and decision questions and results.

Specifically, this could include:
* one or more Rituals each possibly composed of sub-Rituals detailing the evaluators involved, the data pathways, and including
* pairs of Prolog and CLIPS derived from the conversation (we're working towards making this deterministic via CAWS language) for the custom evaluator instances
* standard evaluators would include web search, web crawl, and research type actors and participate in the Rituals

Deployed system:
*decision system is packaged and deployed along with (for instance) a web site*
User -> website -> decision subsystem -> user developed procedures/rules/Rituals -> Decisions and recommendations with explanations

Within a deduce operation, we'll be invoking: evaluator code, prolog rules, clips rules, tools, sending and receiving messages over the coire with participating evaluator peers, results of asynchronous operations also participating as evaluators (say a long running research or web crawl).

Within a rule, we can invoke tools within Dis.  One backs clara_fy.  This is served by the custom Clara LLM which we'll tune and train to effective operate in both dev and runtime modes.

An example might be determining if a user is confused, a predicate used elsewhere (something like):

admit_them(Visitor, Context, Reason) :- \+visitor_confused(Visitor, Context), has_pass(Visitor).
visitor_confused(Visitor, Context) :- clara_fy("Is the user lost or confused?",Context,Tv), Tv == true.
suggest_help_kiosk(Visitor, Context) :- visitor_confused(Visitor, Context), assert(suggestion(seek_help_kiosk)).

Our transduction tool (we're using this in place of CAWS transpilation until the language is ready) would then add in the rabbit and rat dependencies, some housekeeping predicates, and detect the needed forward chain from visitor_confused being used as a condition adding a CLIPS rule to push the suggest_help_kiosk goal.

The LLM consulted in clara_fy is by Dis and is Clara LLM.

However, we could push off this decision (the visitors mental state) to a CounselorEvaluator (or the like).  We'd then need a mechanism for advertising the job to evaluate the visitor, and be ready to receive a response.

We'll use the Coire to push a message on a given topic.  Evaluators which advertise an interest (subscribe) to that topic then receive messages on it.  These may be point to point (within a Ritual) using diff ids, or broadcast, etc.

We could add a new tool call predicate such as: caws_squawk/3 something such as caws_squawk(MyParticipantId, Topic, [Tag1, Tag2, ...])
MyParticipantId only has to be unique to a ritual performance and allow routing of a message on the Coire.
Topic could be constructed such that it can help routing between Dis domains and by topic like : 'DisXYZ/Ritual123/PerformanceABC/PsychEvals/EvaluationNo999/DrHarveyEvaluator22'

It might be interesting to look at using something like Elastic Search to cluster messages like this.. (side note to look at their newly open sourced memory system for AI).

When one evaluator needs to cooperate with another, it passes messages on the Coire.  The other needs to have a CLIPS rule which fires on that event and pushes some goal on which that event impends.

An edge between two evaluators implies an originating push from the source and an asynchronouse event on the target.  Failed communications are handled by timing out an operation to false and letting convergence continue (the reply may eventually come back).

We can specify Offering/Tephra as a general type and Hohi/Tabu for result success/error but ideally we'd have them typed.  A useful analogy is the networks in Apache Nifi between systems like HL7 or Eric health systems data and processing nodes which route and cook the data before passing it along the workflow.

We'd have slots for incoming and outgoing messages.  For Evaluators these need to be at least Offering/Tephra though additional types would imply listening for or generating messages on topics corresponding to topics/channels on those types between those nodes.

Eventually, we'll add things like backpressure and replay but for now we can fail fast.

So, a full interaction for a deployed system might be something like:

user -> website -> chat -> some LLM maybe in Clara maybe not
                -> decision or recommendation required, website consults Clara and passes the query and context
                -> Clara invokes a Ritual performance and passes the query and context into the initial node (assumed to be a ClaraMindSpinter or other deduce capable evaluator)
                -> A deduction results in Prolog and CLIPS rules which invoke Clara LLM to decide on truth values and route messages to other participating nodes
                -> At least one FieryPit is participating and one or more evalators are instantiated by the performance from Dis.
                -> FieryPits don't talk to each other directly, neither do Evaluators
                -> Ritual converges to a decision with possible recommendations and a reasoning trace which can be persisted and explained

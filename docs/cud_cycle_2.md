Let's enter planning mode and revisit .../Development/clara-cerebellum/chewing_the_cud.md in light of our recent work with the lildaemon example .../Development/lildaemon/examples_ritual_snek_splinter.py

We have just finished an end to end test showing a Ritual composed of a splinter evaluator and a snek evaluator.  The snek evaluator accepts a message with a query then runs a web crawl, pushing evaluated results onto a kafka topic.

Now, we want to recreate a feature of an earlier version of the examples_ritual_snek_splinter.py example which included pushing evaluated results to Edgequake.
The earlier version of this example tried to instantiate splinter and snek instances locally rather than working with Rituals through a FieryPit.
We had earlier generated some python code for consuming from the query topics and pushing documents into the Edgequake graphical RAG system: see .../Development/lildaemon/EdgequakeIngestConsumer.py

Let's now develop a plan to build on our examples_ritual_snek_splinter.py with an examples_ritual_rumination_ingest.py.
* we'll have the example code create a new ritual involving the splinter, a snek, and a new evaluator EdgequakeIngestEvaluator.
* the ritual sets up a splinter for coordination and eventual deduction response handling, a snek for crawling the web in response to a query over an offering from the splinter, and an edgequakeingest for processing and pushing documents into the edgequake pipeline corresponding to a tenant and subject-oriented workspace.
* we'd like individual messages on the subject topic (sourced pages from snek) to be processed asynchronously by the edgequakeingest so that the ritual/deduction can converge after
    - snek has finished crawl and forwarded relevant pages to topic
    - edgequakeingest has seen all the pending messages (we may want to just key off of "i haven't seen a new page in a while")
    - splinter has received hohis from snek and edgequakeingest indicating success and summary

In our next iteration, we'll have this as a long running/repeatable job as a companion to another example ritual which uses .../Development/clara-cerebellum/clara-prolog/prolog-lib/the_cow.pl to answer queries including edgequake context.

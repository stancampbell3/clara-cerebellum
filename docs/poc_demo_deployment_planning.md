Let's enter planning mode.
We need to create one or more docker containers to be deployed to an AWS instance.
Our testing instance is a al2023-ami-2023.6.20250303.0-kernel-6.1-x86_64

The site also hosts our company website, the source of which is at ../seashell.  There are deployment scripts in the ../seashell/deploy directory to automate full and periodic updates.  You can use this information when setting up the configuration for the docker container (how it gets started, ports, etc) and the credentials and paths for the target instance.

We'll want to run clara-api, the two MCP servers, and our frontdesk poc demo.  Containerizing them should make them easier to deploy and manage.

Importantly, we'll run our FieryPit (../lildaemon/goat) REST server there as well.  We can probably just deploy that python project as a .venv and repo along with the rest?

Additionally, we have an Edgequake graph RAG system built from source at ../edgequake.  That is another MCP server we'd like to deploy to AWS though we may want a bigger system to host it.  Let's look at that as well.  Should we just create a new, larger instance in AWS to host these components of Clara, use the existing instance, etc.

The Clara system is in demo phase right now, so we won't need to run it continuously.

So here are the projects under ~/Development:

* clara-api : our main REST API server offering the DemonicVoice with Prolog, CLIPS, and Deduction features
* clara-prolog : the MCP server for clara-api's Prolog engine
* clara-clips : the MCP server for the CLIPS engine
* clara-frontdesk-poc : our demo
* lildaemon : the FieryPit REST API server offering the FieryPit protocol
* edgequake : our graph RAG system integration, with its own MCP server and frontend

Note, the model for the FastText classication has moved under ./models/dagda-0.2.bin and we've updated our Groq API key which we'll be using for inference in this deployment.

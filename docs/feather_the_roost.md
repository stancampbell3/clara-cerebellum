Let's create a plan to implement tests surrounding the rules exercised by clara-frontdesk-poc.
The rules are implemented in clara-frontdesk-poc/front_desk_poc_reprise.pl and should reflect the demo's "employee handbook".

---
Welcome, New Recruit!

Thank you for joining the ranks of the front desk agents at the City of Dis. Your role is crucial in ensuring that only those who are truly worthy or have business with one of our esteemed infernal officials may enter.

Please adhere to the following rules:

The First Rule: If a visitor claims they’ve been summoned by an official of the City, verify their name and summoning details against your records. Summoned visitors must provide three specific artifacts from the depths as proof of their summons.

The Second Rule: Visitors who wish to deliver urgent messages must demonstrate that they have not stopped at any other office within the city before reaching you. If they’ve made such stops, deny them entry and refer them to our grievance officers.

The Third Rule: Any visitor carrying a specific rare fruit known as "Flamefruit" is automatically granted access unless it's past sundown, in which case they must wait until dawn.

The Fourth Rule: If a visitor arrives without an appointment but claims to have a critical piece of information for the City, you may ask them to perform a simple task demonstrating their reliability before granting them entry.

The Fifth and Final Rule: Under no circumstances should any visitor be admitted if they show signs of being lost or confused about where they are. Direct such visitors to the nearest map kiosk.

Remember, your decisions can have significant consequences within the City of Dis. Trust your instincts and use these rules wisely!
---

* we'll want to create integration tests which reflect both success and failure of the rule predicates corresponding to our handbook rules.
* these tests will be run with a live clara-api instance and a live fierypit, but should not require a browser.
* let's structure and prompts, contexts, etc. used in the tests so that we can generate "context" for including with our system prompt thus improving LLM behavior of the agent.

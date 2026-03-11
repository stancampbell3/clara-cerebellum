## City of Dis - Front Desk POC
==============================
# This is a small self-contained web application which makes use of the Clara API to demonstrate how to use it to build a front desk application.

### We will:
1.  Demonstrate the use of the LittleDaemon client to power both conversational and decision making capabilities of the application.
2.  Show how to use the Clara API to manage conversations and decisions.
3.  Provide a simple user interface to interact with the application.
4.  We'll have a simple backend server to handle API requests and manage the state of the application.
5. Provide a simple frontend application to interact with the backend server and display the results to the user.

### We will simulate a visitor entering the City of Dis administrative offices and the interactions between the visitor
and the agent on duty there.

### We will leverage the eval endpoint of a KindlingEvaluator through FieryPit (running on port 6666 on the same server)
to evaluate the visitor's responses and determine the next steps in the conversation.

### We will leverage the /deduce endpoint of a KindlingDeductor through FieryPit to make decisions based on the visitor's
responses and the current state of the conversation.

### Final states are that the visitor has been granted access to the City of Dis administrative offices and has been given
a visitor badge or the visitor, the visitor has been denied access, and the visitor has been directed
elsewhere (say to a help kiosk or another office).

## Existing resources
* in clara-frontdesk-poc/roost/front_desk_poc_reprise.pl is our source Prolog program which will enforce the "admit/2" predicate, giving a Reason for admittance if it evaluates to true.  It will be "transduced" and decorated into .roost/front_desk_poc_reprise_clara.pl and roost/front_desk_poc_reprise_clara.clp.
* please see ./docs/deduce_endpoint.md for details on the using /deduce to check admittance and for getting Suggestions
* the Clara reasoning systems clara-cycle coordinates evaluation of goals, suggestions of new goals, and recommendations to the caller.  we'll use it to check for admittance given the current conversational context.
* we'll "transduce" and produce the decorated Prolog and the CLIPS files at build time.  the _clara.pl and _clara.clp resources will be deployed with the POC webapp.
* we'll use the fierypitclient to establish a connection to a KindlingEvaluator.
* we'll use the /evaluate request for conversational interaction with the user/visitor
* we'll use the /deduce request for checking whether the current conversation context includes facts which justify admitting the visitor.
* if the /deduce returns "Suggestions" then we'll prompt the LLM during evaluate with this additional context to ask relevant questions or give relevant advice.

Feel free to rework the front_desk_poc_reprise.pl as needed, but remember *control* lives outside the /deduce system so we don't embed state machines or the like in Prolog.  we use it only to justify decisions and, through forward chaining, suggest alternative paths of reasoning or feedback to the visitor and/or agent.

We can assume a running FieryPit on port 6666 with a configured KindlingEvaluator available to set.
Also, the clara-api will be running locally to serve the FieryPit's instance of KindlingEvaluator through the Demonic Voice.



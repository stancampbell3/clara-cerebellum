Let's plan an extension to our /deduce endpoint.
We need to pass optional context information into the request.
This data corresponds to the conversation or message history for the Deduction session.
This is separate from any message history originating from the clara-cycle operation itself.
It's external data passed into the deduction process to be operated over by rules and engine-triggered llm evaluations.

Our current payload looks something like this:
蠲 stanc@Pineal:~/Desktop/Development/clara-cerebrum$ cat deduce_request.json 
{
    "prolog_clauses": ["consult('animal_id_clara.pl')."],
    "clips_constructs": [],
    "clips_file": "animal_id_clara.clp",
    "initial_goal": "animal(A), has(A,cold_blooded).",
    "max_cycles": 100
}

We should add a new field to hold the messages "context".
This data should be made available to the Prolog and CLIPS engines during the clara-cycle so should be injected possibly as an assert in both?
Maybe a coire-event?  Suggestions on design here are welcome.

We'll be making use of the context in the Prolog layer (at least) in these calls:

- these are our prolog libraries built into the modules delivered with our binaries
* the_rabbit.pl - 
%% classify_text/2 - Classify text using the fastText classify tool
classify_text(Text, Result) :-
    dict_to_json(_{tool: classify, arguments: _{text: Text}}, Json),
    clara_evaluate(Json, Result).

%% classify_text_k/3 - Classify text returning top K predictions
classify_text_k(Text, K, Result) :-
    dict_to_json(_{tool: classify, arguments: _{text: Text, k: K}}, Json),
    clara_evaluate(Json, Result).

Probably, we'll just need to add in properly a properly formatted field to contain the context in the JSON payload to the evaluate (Rust built in FFI callback)

* the_rat.pl - clara_fy/2 makes use of top_status which eventually calls 
clara_fy(Text, TruthValue) :- top_status(Text, B1),
    format('B1: ~w~n', [B1]),
    TruthValue = B1.

makes use of from the_rabbit:
%% descriminate_k - Extract the response from the LLM and classify it with top K results
descriminate_k(Text, K, Results) :-
    ponder_text(Text, LLMSez), % Get the JSON response from the LLM
    extract_nested(LLMSez, [hohi, response, response], Response),
    !,
    ( response_shortcut(Response, Shortcut) ->
        shortcut_label(Shortcut, LabelStr),
        atom_json_dict(Results, _{predictions: [_{label: LabelStr, probability: 0.99}]}, [])
    ;
        atom_string(Text, TextStr),
        atom_string(Response, RespStr),
        string_concat(TextStr, " ", Tmp),
        string_concat(Tmp, RespStr, Pair), % Combine the text and LLM's response
        classify_text_k(Pair, K, Results) % Classify with top K results
    ).
descriminate_k(_, _, _) :-
    format(user_error, "Error: Could not extract response from LLM output.~n", []),
    fail.

For some conditions, we'd like to do a clara_fy including context information to build the parameters to the underlying FastText model at the prolog level.

Something along the lines of:

redirect_the_visitor(Visitor, help_kiosk) :- visitor(Visitor, Context), clara_fy('the visitor seems confused or lost', Context, R), R = true.

Where Context is our message history and the LLM will be tasked with verifying the statement using the context.





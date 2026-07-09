% Authored Prolog source for node n1 ("Clara") in the typed-edges live E2E.
%
% The `offering` edge e1 (n1 -> n2) in graph_layout.json is `pipeMode: auto`:
% edge transduction generates, on n1,
%
%   caws_auto_pipe_e1(Cid) :-
%       caws_pipe('e1', 'n2', 'consults/e1', Cid).
%   caws_auto_pipe_e1(_).
%
% plus CLIPS rules that (a) fire the pipe for every incoming Offering and
% (b) dispatch the correlated Hohi/Tabu reply through caws_edge_reply/3,
% which asserts edge_result(EdgeId, hohi|tabu, PayloadDict) and calls the
% optional on_edge_hohi/2 / on_edge_tabu/2 hooks.
%
% So NOTHING here publishes the Offering — the Run's query enters the
% deduction as a synthetic incoming Offering (lildaemon passes
% initial_offering), the generated rules forward it to n2 (Groq), and this
% source only CONSUMES the reply fact. The root goal fails until the reply
% (or timeout) lands; the cycle re-proves it once edge_result/3 exists.

% Clause 1: Groq answered — synthesize its answer with the local one.
reasoned_response(Query, Context, Response) :-
    edge_result(e1, hohi, GroqReply),
    ponder_text_with_context(Query, Context, ClaraAnswer),
    (   catch((get_dict(response, GroqReply, R1), get_dict(content, R1, GroqAnswer)), _, fail)
    ->  true
    ;   term_to_atom(GroqReply, GroqAnswer)
    ),
    format(atom(Synth), "Two answers to the question '~w' follow. Reconcile them into one short, best answer.~nAnswer A: ~w~nAnswer B: ~w", [Query, ClaraAnswer, GroqAnswer]),
    ponder_text_with_context(Synth, Context, Response).

% Clause 2: Groq refused or timed out (patience Tabu) — answer with Clara
% alone rather than failing the Run.
reasoned_response(Query, Context, Response) :-
    edge_result(e1, tabu, _),
    ponder_text_with_context(Query, Context, Response).

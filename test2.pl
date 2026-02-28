% Define the environment_info predicate for pattern matching
environment_info([(clips_session_id, _), (prolog_session_id, PrologSessionID), (cerebellum_url, _), (user_id, _), (session_name, _)]).

% Main query predicate to extract and print prolog_session_id
query_prolog_session_id(environment_info(List)) :-
    member((prolog_session_id, PrologSessionID), List),
    atom_string(PrologSessionID, PrologSessionIDString),
    format('The prolog session ID is: ~s~n', [ProLogSessionIDString]).
%% the_leannan.pl
%% --------------
%% leannan_sidhe: graph-mechanical divergent retrieval for the Id analyst
%% (lildaemon/goat/app/assistant/rulesets/id_analyst.pl). Design doc:
%% clara-cerebellum/docs/leannan_sidhe_planning.md (approved 2026-09-11).
%%
%% Shape: walk the knowledge graph away from a query's own entities
%% (neighbor-hop / sibling-by-type / bridge-between-two-seeds), feed the
%% walk's result to Edgequake's Mix mode as `ll_keywords` with deliberately
%% skewed arm weights, a varied `rrf_k`, and — for half the profiles — the
%% new `fringe` fusion mode (SPEC-084), which rewards chunks present across
%% arms but ranked poorly in each ("fringe consensus") instead of chunks
%% ranked well. Six fixed profiles, no LLM/think loop anywhere in the
%% perturbation itself — the divergence is entirely graph-mechanical.
%%
%% Structure: graph-walk primitives (leannan_entities/2,
%% leannan_neighborhood/3, leannan_by_label/2) are thin wrappers over the
%% `edgequake` tool's graph endpoints, dispatched the same way the_cow.pl
%% dispatches — {tool, arguments} dict -> the_rabbit:clara_evaluate/2 ->
%% atom_json_dict/3 -> status check — via the private helper
%% leannan_dispatch/2, since the_cow.pl has no equivalent for graph ops
%% (it only wraps `operation: query`). The actual per-spark Edgequake RAG
%% query, though, reuses the_cow.pl's ruminate_opts/3 and
%% ruminate_and_assert_citations/3 directly rather than re-implementing
%% that dispatch a second time: ruminate_opts/3's `Opts.put/1` merge
%% already forwards any Edgequake QueryRequest field (context_only,
%% ll_keywords, mode, mix_weights, rrf_k, fusion — SPEC-084), so there is
%% nothing graph-op-shaped left to hand-roll for the query call.
%%
%% Implementation-time corrections against live source (see the design doc
%% for the full trace, mirrors the SPEC-084 FR-004 correction there):
%%   - leannan_neighborhood/3 passes Hops straight through as Edgequake's
%%     native `depth` query param (server-clamped [1,3]) instead of the
%%     draft's "iterate for Hops > 1" — the endpoint already does
%%     multi-hop in one call, so manual iteration would just be redundant
%%     round-trips for the same result.
%%   - `bridge` does NOT use a `leannan_relationships(+Src,+Tgt,-Rels)`
%%     primitive as originally drafted: confirmed live 2026-09-11 that
%%     Edgequake's `GET /graph/relationships` list endpoint has no
%%     source/target filter at all (`ListRelationshipsQuery` is
%%     page/page_size/relationship_type only — see
%%     clara-toolbox/src/tools/edgequake.rs's `graph_search_relationships`
%%     doc comment for the full finding). `bridge` instead intersects the
%%     1-hop neighborhoods of each pair of seeds — entities appearing in
%%     both are exactly "what joins the query's own entities", without
%%     depending on a filter Edgequake doesn't have. `leannan_relationships/2`
%%     (label-only — the one filter that IS real) is still exported for
%%     completeness/future operators, just not used by `bridge`.
%%   - leannan_and_assert_citations/4 (SparkId, Query, Opts, Result) has
%%     one more argument than the draft's "mirroring
%%     ruminate_and_assert_citations/3" — SparkId is needed to key
%%     spark_cites/2 (which spark used which citation), something
%%     the_cow.pl's non-spark-scoped ruminate_and_assert_citations/3 has
%%     no reason to track.

:- module(the_leannan, [
    leannan_entities/2,
    leannan_neighborhood/3,
    leannan_by_label/2,
    leannan_relationships/2,
    leannan_perturb/4,
    leannan_spark/5,
    leannan_sparks/4,
    leannan_and_assert_citations/4,
    leannan_profiles/1
]).

:- use_module(library(http/json)).
:- use_module(library(the_rabbit), [dict_to_json/2]).
:- use_module(library(the_cow), [
    ruminate_opts/3,
    ruminate_and_assert_citations/3,
    ruminate_citations/2,
    citation/8,
    cite/2
]).

% spark/3 memoizes one spark's full result per SparkId, thread_local for the
% same reason the_cow.pl's citation/8 is (see that file's doc comment for
% the full mechanism): each /deduce call runs on its own SWI engine, but
% engines share one global dynamic-predicate database unless declared
% thread_local. This also gives id_step/3 (lildaemon Tier 3) its
% assert-once-per-retry-cycle memoization for free — the multi-cycle
% classify deduction re-enters this file's goals on every engine retry
% while `caws_offer`/`caws_await` legs are pending, and a bare Edgequake
% call inside that loop would otherwise re-run retrieval on every cycle
% (the anti-pattern rituals_101.md warns about for ponder_text).
% spark_cites/2 records which citation ids a given spark actually used,
% analogous to the_cow.pl's cites/2 (which records a *conclusion's*
% citations) but scoped to a spark instead.
:- thread_local spark/3.
:- thread_local spark_cites/2.

%% ---------------------------------------------------------------------
%% Graph-walk primitives
%% ---------------------------------------------------------------------

%% leannan_dispatch/2 - shared {tool, arguments} -> clara_evaluate ->
%%   atom_json_dict -> status-check boilerplate for the graph endpoints,
%%   mirroring the pattern repeated three times in the_cow.pl (there,
%%   left un-factored per-predicate; factored here since this module has
%%   four call sites needing it).
leannan_dispatch(Json, Dict) :-
    the_rabbit:clara_evaluate(Json, Raw),
    % value_string_as(atom): without this, atom_json_dict/3 decodes JSON
    % strings as SWI strings, which never unify with the bare atom `error`
    % below (same fix the_cow.pl / the_rabbit.pl already apply).
    atom_json_dict(Raw, Dict, [value_string_as(atom)]),
    ( get_dict(status, Dict, error) ->
        format(user_error, "edgequake tool error: ~w~n", [Dict.message]),
        fail
    ;
        true
    ).

%% entity(Id, Name, Type) - canonical entity shape this module normalizes
%%   every graph endpoint's response into, so callers never need to know
%%   whether an entity came from a search result (`entity_name`) or a
%%   neighborhood result (`label`) — both endpoints use different JSON key
%%   names for the same presentation-name concept. Id is the graph
%%   identity (stable across endpoints, what path-based lookups expect);
%%   Type is Edgequake's `entity_type` (the plan's "label" — see
%%   leannan_by_label/2's doc comment for why).
entity_from_search_item(Item, entity(Id, Name, Type)) :-
    Id = Item.get(id, ''),
    Name = Item.get(entity_name, Id),
    Type = Item.get(entity_type, none).

entity_from_neighborhood_node(Node, entity(Id, Name, Type)) :-
    Id = Node.get(id, ''),
    Name = Node.get(label, Id),
    Type = Node.get(entity_type, none).

%% leannan_entities(+Query, -Entities) - entities whose name/description
%%   match Query (substring search), as entity/3. The seed set for
%%   leannan_perturb/4. Edgequake's entity search is a plain substring
%%   match, not NER — a multi-word natural-language Query may match few or
%%   no entities; leannan_spark/5 falls back to the raw query text as its
%%   sole ll_keyword when Seeds comes back empty, so this degrading
%%   gracefully (not failing the whole spark) is by design.
leannan_entities(Query, Entities) :-
    dict_to_json(_{tool: edgequake,
                   arguments: _{operation: graph_search_entities,
                                search: Query}}, Json),
    leannan_dispatch(Json, Dict),
    Items = Dict.get(items, []),
    maplist(entity_from_search_item, Items, Entities).

%% leannan_neighborhood(+EntityId, +Hops, -Neighbors) - the Hops-hop
%%   neighborhood of one entity, as entity/3. Hops is Edgequake's native
%%   `depth` param (server-clamped to [1,3] — see module doc comment for
%%   why this replaced the draft's manual multi-call iteration).
leannan_neighborhood(EntityId, Hops, Neighbors) :-
    dict_to_json(_{tool: edgequake,
                   arguments: _{operation: graph_entity_neighborhood,
                                entity_name: EntityId,
                                depth: Hops}}, Json),
    leannan_dispatch(Json, Dict),
    Nodes = Dict.get(nodes, []),
    maplist(entity_from_neighborhood_node, Nodes, Neighbors).

%% leannan_by_label(+Label, -Entities) - entities of a given Edgequake
%%   `entity_type` (e.g. "PERSON", "ORGANIZATION"). Named `by_label` (not
%%   `by_type`) to match the design doc's operator-table terminology,
%%   where "label" means "the seed's category", i.e. Edgequake's
%%   `entity_type` field — not `NeighborhoodNode.label` (a *different*
%%   field on a *different* endpoint that means "presentation name").
%%   Confirmed live 2026-09-11 that the wire query param is `entity_type`;
%%   see clara-toolbox/src/tools/edgequake.rs's fix.
leannan_by_label(Label, Entities) :-
    dict_to_json(_{tool: edgequake,
                   arguments: _{operation: graph_search_entities,
                                label: Label}}, Json),
    leannan_dispatch(Json, Dict),
    Items = Dict.get(items, []),
    maplist(entity_from_search_item, Items, Entities).

%% leannan_relationships(+Label, -Rels) - relationships of a given
%%   Edgequake `relationship_type`, as raw dicts (source/target/
%%   relationship_type/weight per NeighborhoodEdge/RelationshipSummary
%%   shape). Label-only: Edgequake's `/graph/relationships` list endpoint
%%   has no source/target filter (confirmed live 2026-09-11 — see module
%%   doc comment), so this predicate does not claim arity/2 filtering it
%%   cannot perform. Not used by any v1 divergence operator (`bridge` uses
%%   neighborhood intersection instead); exported for future operators
%%   (`relation_hop`, deferred) that only need type filtering.
leannan_relationships(Label, Rels) :-
    dict_to_json(_{tool: edgequake,
                   arguments: _{operation: graph_search_relationships,
                                label: Label}}, Json),
    leannan_dispatch(Json, Dict),
    Rels = Dict.get(items, []).

%% ---------------------------------------------------------------------
%% Divergence operators (v1: neighbor/1, sibling, bridge — relation_hop
%% and contrast deferred, tracked in the design doc, not dropped)
%% ---------------------------------------------------------------------

seed_ids(Seeds, Ids) :-
    findall(Id, member(entity(Id, _, _), Seeds), Ids).

entity_names(Entities, Names) :-
    findall(Name, member(entity(_, Name, _), Entities), Names).

%% dedup_entities/2 - dedup an entity/3 list by Id, first occurrence wins,
%%   order preserved.
dedup_entities(Entities, Deduped) :-
    dedup_entities_(Entities, [], Deduped).

dedup_entities_([], _, []).
dedup_entities_([entity(Id, Name, Type) | T], Seen, Out) :-
    ( memberchk(Id, Seen)
    -> dedup_entities_(T, Seen, Out)
    ;  Out = [entity(Id, Name, Type) | Out1],
       dedup_entities_(T, [Id | Seen], Out1)
    ).

%% leannan_perturb(+Seeds, +Operator, -Perturbed, -Prov) - the graph walk
%%   itself. Perturbed is a deduped entity/3 list; Prov records the
%%   operator and seeds for the audit trail (Tier 2 design doc §2b).
leannan_perturb(Seeds, neighbor(Hops), Perturbed, Prov) :-
    !,
    findall(N,
            ( member(entity(Id, _, _), Seeds),
              leannan_neighborhood(Id, Hops, Ns),
              member(N, Ns)
            ),
            All),
    dedup_entities(All, Perturbed),
    Prov = perturb(neighbor(Hops), Seeds, Perturbed).
leannan_perturb(Seeds, sibling, Perturbed, Prov) :-
    !,
    seed_ids(Seeds, SeedIds),
    findall(N,
            ( member(entity(_, _, Type), Seeds),
              Type \== none,
              leannan_by_label(Type, Ns),
              member(N, Ns),
              N = entity(NId, _, _),
              \+ memberchk(NId, SeedIds)
            ),
            All),
    dedup_entities(All, Perturbed),
    Prov = perturb(sibling, Seeds, Perturbed).
leannan_perturb(Seeds, bridge, Perturbed, Prov) :-
    Seeds = [_, _ | _],
    !,
    seed_ids(Seeds, SeedIds),
    findall(B,
            ( select(entity(IdA, _, _), Seeds, Rest),
              member(entity(IdB, _, _), Rest),
              IdA @< IdB,
              leannan_neighborhood(IdA, 1, NA),
              leannan_neighborhood(IdB, 1, NB),
              member(B, NA),
              B = entity(BId, _, _),
              \+ memberchk(BId, SeedIds),
              memberchk(entity(BId, _, _), NB)
            ),
            All),
    dedup_entities(All, Perturbed),
    Prov = perturb(bridge, Seeds, Perturbed).
leannan_perturb(Seeds, bridge, Perturbed, Prov) :-
    % Fewer than 2 seeds: degrades to neighbor(1) per the design doc.
    !,
    leannan_perturb(Seeds, neighbor(1), Perturbed, NProv),
    Prov = perturb(bridge_degraded(neighbor(1)), Seeds, Perturbed, NProv).

%% ---------------------------------------------------------------------
%% Spark builder + fan-out
%% ---------------------------------------------------------------------

%% leannan_and_assert_citations(+SparkId, +Query, +Opts, -Result) - like
%%   the_cow:ruminate_and_assert_citations/3, but also records which
%%   citation ids this SparkId used (spark_cites/2), so id_analyst.pl
%%   (Tier 3) can footnote each impulse with only the citations that
%%   actually grounded it, not the whole union across all 6 sparks.
leannan_and_assert_citations(SparkId, Query, Opts, Result) :-
    ruminate_and_assert_citations(Query, Opts, Result),
    ruminate_citations(Result, Sources),
    forall(
        member(Source, Sources),
        ( Id = Source.id,
          ( spark_cites(SparkId, Id) -> true ; assertz(spark_cites(SparkId, Id)) )
        )
    ).

%% leannan_spark(+SparkId, +Query, +Profile, -Spark, -Prov) is det.
%%   Profile = spark(Operator, weights(Local,Global,Naive), K, Fusion).
%%   Builds ONE divergent retrieval spark: seed entities -> graph
%%   perturbation -> Edgequake Mix-mode context_only query (skewed
%%   weights/rrf_k/fusion) -> citations asserted. Spark =
%%   spark_result(Sources, Sources) — context_only mode's only payload is
%%   `sources`, serving both as the evidence text for id_analyst.pl's
%%   impulse prompt and as the citation list; there is nothing else to
%%   split it into.
%%
%%   Memoized: asserts spark(SparkId, Query-Profile, Spark-Prov) on first
%%   computation. id_step/3's classify deduction is multi-cycle (it awaits
%%   caws legs — pending means the goal clause fails and is retried next
%%   engine cycle); without this memo a bare Edgequake call here would
%%   re-run on every retry (rituals_101.md's ponder_text anti-pattern).
%%   Same assert-once-with-cut pattern as deliberative_analyst.pl's
%%   committee_deadline_for/3.
leannan_spark(SparkId, Query, Profile, Spark, Prov) :-
    spark(SparkId, Query-Profile, Spark-Prov), !.
leannan_spark(SparkId, Query, Profile, Spark, Prov) :-
    Profile = spark(Operator, weights(WL, WG, WN), K, Fusion),
    leannan_entities(Query, Seeds),
    leannan_perturb(Seeds, Operator, Perturbed, PProv),
    entity_names(Perturbed, Keywords0),
    ( Keywords0 == []
    -> Keywords = [Query]  % no recognized seeds — see leannan_entities/2 doc
    ;  Keywords = Keywords0
    ),
    Opts = _{context_only: true, mode: mix, ll_keywords: Keywords,
             mix_weights: _{local: WL, global: WG, naive: WN},
             rrf_k: K, fusion: Fusion},
    leannan_and_assert_citations(SparkId, Query, Opts, Result),
    ruminate_citations(Result, Sources),
    Spark = spark_result(Sources, Sources),
    Prov = spark_prov(SparkId, Operator, Seeds, PProv, weights(WL, WG, WN), K, Fusion),
    assertz(spark(SparkId, Query-Profile, Spark-Prov)).

%% leannan_profiles/1 - the fixed, deterministic set of 6 divergence
%%   profiles (design doc §2b table). Order matters: leannan_sparks/4
%%   takes the first N or cycles through all 6.
leannan_profiles([
    spark(neighbor(2), weights(3.0, 1.0, 0.2), 60,  rrf),
    spark(neighbor(1), weights(1.0, 1.0, 1.0), 500, rrf),
    spark(sibling,     weights(1.0, 3.0, 0.5), 60,  rrf),
    spark(sibling,     weights(1.0, 1.0, 1.0), 200, fringe),
    spark(bridge,      weights(2.0, 2.0, 0.3), 60,  rrf),
    spark(bridge,      weights(1.0, 1.0, 0.5), 500, fringe)
]).

%% leannan_sparks(+Query, +Count, -Sparks, -AllCitations) - fan out Count
%%   sparks, cycling leannan_profiles/1 when Count > 6. Explicit recursion
%%   over the profile list (codebase style, not maplist — matches the
%%   design doc's instruction for this specific loop). AllCitations is the
%%   union of every spark's citations, deduped by id.
leannan_sparks(Query, Count, Sparks, AllCitations) :-
    Count > 0,
    leannan_profiles(Profiles),
    length(Profiles, NumProfiles),
    leannan_sparks_(1, Count, Query, Profiles, NumProfiles, Sparks),
    findall(Sources, member(spark_result(Sources, _), Sparks), CitationLists),
    append(CitationLists, AllCitationsDup),
    dedup_citations(AllCitationsDup, AllCitations).

leannan_sparks_(I, Count, _Query, _Profiles, _NumProfiles, []) :-
    I > Count, !.
leannan_sparks_(I, Count, Query, Profiles, NumProfiles, [Spark | Rest]) :-
    I =< Count,
    ProfileIdx is ((I - 1) mod NumProfiles) + 1,
    nth1(ProfileIdx, Profiles, Profile),
    leannan_spark(I, Query, Profile, Spark, _Prov),
    I1 is I + 1,
    leannan_sparks_(I1, Count, Query, Profiles, NumProfiles, Rest).

%% dedup_citations/2 - dedup a list of Edgequake source dicts by `.id`,
%%   first occurrence wins.
dedup_citations(Sources, Deduped) :-
    dedup_citations_(Sources, [], Deduped).

dedup_citations_([], _, []).
dedup_citations_([S | T], Seen, Out) :-
    Id = S.id,
    ( memberchk(Id, Seen)
    -> dedup_citations_(T, Seen, Out)
    ;  Out = [S | Out1],
       dedup_citations_(T, [Id | Seen], Out1)
    ).

% For dev testing, clara_fy local

% we will use context to power the inference backed conditions
:- dynamic(context/1).
:- use_module(library(the_rat)).

%% City of Dis Reception
%% ---------------------

:- dynamic(visitor/1).
:- dynamic(official/1).
:- dynamic(office/1).
:- dynamic(summons/3).
:- dynamic(has_visited_office/2).
:- dynamic(task/1).

:- dynamic(current_time/1). % AST (abyssal standard time)
:- dynamic(sunset/1).

:- assert(sunset/18).
:- assert(visitor(perstefanie)).
:- assert(official(counselor_foultooth)).
:- assert(summons(perstefanie, counselor_foultooth, [flamefruit, towel, fish])).
:- assert(office(office_of_abuse)).
:- assert(task(count_stones_in_wales)).
:- assert(task(collect_bird_spit)).
:- assert(task(bottle_footfall_of_cat)).

% Utility
get_summons(Visitor, Artifacts) :- visitor(Visitor), summons(Visitor,_,Artifacts).

% Admittance - Entrance granted subject to the RULES
% 	rule 1
visitor_admitted(Visitor, Reason) :- visitor(Visitor), is_summoned(Visitor, _), Reason = "Rule 1: The visitor has been summoned".
% 	rule 2
visitor_admitted(Visitor, Reason) :- visitor(Visitor), has_delivered_message(Visitor), Reason = "Rule 2: the visitor has delivered a message".
% 	rule 3
visitor_admitted(Visitor, Reason) :- visitor(Visitor), has_flamefruit(Visitor), Reason = "Rule 3: the visitor is carrying a special totem object".
% 	rule 4
visitor_admitted(Visitor, Reason) :- visitor(Visitor), has_critical_information(Visitor), Reason = "Rule 4: the visitor has critical information for the City".

% Redirection
%	rule 5
visitor_redirected(Visitor) :- visitor(Visitor), redirect_to_map_kiosk(Visitor).

% Rule 1: The First Rule (R1)
is_summoned(Visitor, Official) :-
    visitor(Visitor),
    official(Official),
    clara_fy("A Visitor claims to have been summoned by an Official of the City", R),
    R == true,
    get_summons(Visitor, Artifacts),
    length(Artifacts, 3). % Check if there are exactly three artifacts

% Rule 2: The Second Rule (R2)
has_delivered_message(Visitor) :-
    visitor(Visitor),
    clara_fy("A Visitor wishes to deliver an urgent message and claims not to have stopped at any other office within the city", R),
    R == true.

is_valid_stop(Visitor, Office) :-
    visitor(Visitor),
    office(Office),
    has_visited_office(Visitor, Office).

deny_access_to_grievance_officer(Visitor) :-
    is_summoned(Visitor, _Official), !.
deny_access_to_grievance_officer(Visitor) :-
    has_delivered_message(Visitor),
    findall(Office, is_valid_stop(Visitor, Office), VisitedOffices),
    length(VisitedOffices, L),
    L > 0.

% Rule 3: The Third Rule (R3)
has_flamefruit(Visitor) :-
    visitor(Visitor),
    clara_fy("A Visitor carries Flamefruit", R),
    R == true.
is_daytime :-
    current_time(Time),
    Time < sunset. % Check if the time is past sundown

grant_access_due_to_flamefruit(Visitor) :-
    has_flamefruit(Visitor),
    is_daytime.

% Rule 4: The Fourth Rule (R4)
has_critical_information(Visitor) :-
    visitor(Visitor),
    clara_fy("A Visitor claims to have critical information for the City", R),
    R == true.

perform_task_for_reliability(Task, Visitor) :-
    visitor(Visitor),
    task(Task),
    clara_fy("A Visitor performs a simple task as proof of reliability", Task).

grant_access_after_performing_task(Visitor) :-
    has_critical_information(Visitor),
    perform_task_for_reliability(_Task, Visitor).

% Rule 5: The Fifth Rule (R5)
is_lost_or_confused(Visitor) :-
    visitor(Visitor),
    clara_fy("A Visitor shows signs of being lost or confused", R),
    R == true.

redirect_to_map_kiosk(Visitor) :-
    visitor(Visitor),
    is_lost_or_confused(Visitor).

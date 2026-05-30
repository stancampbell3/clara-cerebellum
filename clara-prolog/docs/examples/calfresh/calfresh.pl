%% calfresh.pl - Example decision rules for CalFresh eligibility and suggested actions.

% dynamic(household_size/1).
% dynamic(gross_monthly_income/1).
% dynamic(immigration_status/1).
% dynamic(student_status/1).
% dynamic(has_senior_or_disabled_member/1).
% dynamic(aba_status/1).

% testing deduction predicate
evidence_of(A, Tv) :- write(A),
    read(Answer),
    Answer == "true",
    Tv = true.

evidence_of_value(A, Value) :- write(A),
    read(Answer),
    integer(Answer),
    Value = Answer.

% top predicate
elgible(Decision, Requirements) :- high_level_criteria(Requirements),
    other_criteria,
    Decision = true.


high_level_criteria(Requirements) :- immigration_citizenship,
    income_thresholds,
    work_requirements(Requirements).

household_composition :- evidence_of_value("How many individuals in the household?", Value),
    assert(household_size(Value)).

household_income :- evidence_of_value("What is the gross monthly income?", Value),
    assert(gross_monthly_income(Value)).

immigration_citizenship :- evidence_of("is a united states citizen", true).
immigration_citizenship :- evidence_of("is a united states national", true).
immigration_citizenship :- evidence_of("is a lawful_permanent_resident", true).
immigration_citizenship :- evidence_of("is a cuban or hatian entrant", true).
immigration_citizenship :- evidence_of("is under COFA agreement", true).

income_thresholds :-
    household_composition,
    household_income,
    (   exempt_income_limit
    ->  true
    ;   household_size(S),
        gross_monthly_income_limit(S, Limit),
        gross_monthly_income(Income),
        Income < Limit
    ).

exempt_income_limit :-
    evidence_of("Meets condition: households with a member aged 60+ or disabled...", true).

%% gross_monthly_income_limit(Household_size, IncomeLimit)
gross_monthly_income_limit(1, 2610).
gross_monthly_income_limit(2, 3526).
gross_monthly_income_limit(3, 4442).
gross_monthly_income_limit(4, 5360).
gross_monthly_income_limit(5, 6276).
gross_monthly_income_limit(6, 7192).
gross_monthly_income_limit(7, 8110).
gross_monthly_income_limit(8, 9026).

gross_monthly_income_limit(Household_size, IncomeLimit) :-
	Household_size > 8,
	gross_monthly_income_limit(8, Value),
	IncomeLimit = Value + 918.

%% California utilizes "Modified Categorical Eligibility" (MCE), allowing most households to have a gross monthly income up to **200% of the Federal Poverty Level (FPL)**.
%% Households with a member aged 60+ or disabled may have higher effective income limits and are not subject to asset limits in many scenarios.
gross_monthly_income_limit(_, _) :- evidence_of("Meets condition: households with a member aged 60+ or disabled may have higher effective income limits and are not subject to asset limits", true).

able_bodied_adult_without_dependants :- evidence_of("age is within 18 to 64 years", true),
	evidence_of("has no dependents", true).

%% 3. Work Requirements (ABAWD)
%% Starting **June 1, 2026**, Able-Bodied Adults Without Dependents (ABAWDs)—defined as ages 18–64—are subject to
%% stricter time limits.
%%
%% **The Rule:** Benefits are limited to 3 months within a 36-month period unless the individual works, volunteers, or participates in approved training for at least **20 hours per week** (or averages 80 hours monthly).
%% **Exemptions:** Caring for a child under 14, disability, or living in a county with a current waiver.

exemptions_work_requirements :- evidence_of("caring for a child under 14", true).

exemptions_work_requirements :- evidence_of("living in a county with a current waiver", true).

work_requirements(Requirements) :- able_bodied_adult_without_dependants,
	\+ exemptions_work_requirements,
	Requirements = "Benefits are limited to 3 months within a 36-month period unless the individual works, volunteers, or participates in approved training for at least **20 hours per week** (or averages 80 hours monthly).".

other_criteria :- student_elgibility.

student_elgibility :- evidence_of("Awarded federal work-study", true).
student_elgibility :- evidence_of("Working 20 hours/week", true).
student_elgibility :- evidence_of("Have a dependent under age 12", true).
student_elgibility :- evidence_of("Enrolled in a state/local program that increases employability", true).
student_elgibility :- evidence_of("Receiving TANF-funded financial aid", true).

expedited_service :- evidence_of("Gross monthly income is <$150 and liquid resources are <$100",true).
expedited_service :- evidence_of("Monthly income/resources are less than monthly rent/utilities combined", true).
expedited_service :- evidence_of("The applicant is a migrant or seasonal farmworker with <$100 in liquid resources", true).

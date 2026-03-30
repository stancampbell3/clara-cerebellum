// deduce_basic_test.rs
// --------------------
// Test the clara cycle controller by evaluating a simple deduction request
// We will use the prolog source in ./resources/deduce_basic_test.pl
// It has been "transduced" into deduce_basic_test_clara.pl and deduce_basic_test_clara.clp

// This simple test will check that the cycle controller can run through the entire cycle and converge on the correct answer for a simple deduction request
// which involves forwarding a derived fact (assertion) from Prolog to CLIPS, triggering a new goal, and back to Prolog with the new goal, arriving at an answer that matches the expected result.


// We should assert two dynamic predicates:
// visitor(bob) and egg(unbroken)
// The initial goal should be "omelette(bob, Dish)" which we expect to converge to true with Dish = lovely_fluffy_goodness

/*  Example deduce request we want to run through the clara-cycle's controller:
{
    "prolog_clauses": ["consult('./deduce_basic_test_clara.pl').", "visitor(bob), egg(unbroken)."],
    "clips_constructs": [],
    "clips_file": "./test9_clara.clp",
    "initial_goal": "omelette(bob, X).",
    "context": [],
    "max_cycles": 3
}
 */


use serde_json::{json, Value};

/// Schema for prolog.query tool
pub fn query_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "goal": {
                "type": "string",
                "description": "Prolog goal to query (e.g., 'member(X, [1,2,3])' or 'parent(tom, X)')"
            },
            "all_solutions": {
                "type": "boolean",
                "description": "If true, return all solutions; if false, return first solution only (default: false)",
                "default": false
            }
        },
        "required": ["goal"]
    })
}

/// Schema for prolog.consult tool
pub fn consult_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "clauses": {
                "type": "array",
                "description": "List of Prolog clauses (facts and rules) to load into the knowledge base",
                "items": {
                    "type": "string",
                    "description": "Prolog clause (e.g., 'parent(tom, mary)' or 'grandparent(X, Z) :- parent(X, Y), parent(Y, Z)')"
                }
            }
        },
        "required": ["clauses"]
    })
}

/// Schema for prolog.retract tool
pub fn retract_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "clause": {
                "type": "string",
                "description": "Prolog clause pattern to retract (e.g., 'parent(tom, _)' to remove all facts where tom is a parent)"
            },
            "all": {
                "type": "boolean",
                "description": "If true, retract all matching clauses; if false, retract only first match (default: false)",
                "default": false
            }
        },
        "required": ["clause"]
    })
}

/// Schema for prolog.status tool
pub fn status_schema() -> Value {
    json!({
        "type": "object",
        "properties": {}
    })
}

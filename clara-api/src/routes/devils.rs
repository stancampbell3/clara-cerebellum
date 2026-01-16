// Re-export devils (Prolog) handlers
pub use crate::handlers::devils_handler::{
    create_prolog_session, get_prolog_session, list_prolog_sessions,
    terminate_prolog_session, query_prolog, consult_prolog,
};

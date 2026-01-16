//! Low-level FFI bindings to SWI-Prolog C API
//!
//! These bindings are based on SWI-Prolog.h from swipl-devel.
//! See: https://www.swi-prolog.org/pldoc/man?section=foreign

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use libc::{c_char, c_int, c_void};

// =============================================================================
// Type Definitions
// =============================================================================

/// Opaque engine handle (PL_engine_t)
/// Each session gets its own engine for isolation
pub type PL_engine_t = *mut c_void;

/// Term reference - handle to a Prolog term
/// Terms are local to a query/foreign frame
pub type term_t = usize;

/// Atom reference - interned string identifier
pub type atom_t = usize;

/// Functor reference - name/arity pair
pub type functor_t = usize;

/// Module reference
pub type module_t = *mut c_void;

/// Predicate reference
pub type predicate_t = *mut c_void;

/// Query handle for iterating solutions
pub type qid_t = *mut c_void;

/// Foreign frame ID for memory management
pub type fid_t = usize;

/// Foreign function pointer type
pub type pl_function_t = *const c_void;

// =============================================================================
// Constants
// =============================================================================

// Engine constants (from SWI-Prolog.h)
pub const PL_ENGINE_MAIN: PL_engine_t = 0x1 as PL_engine_t;
pub const PL_ENGINE_CURRENT: PL_engine_t = 0x2 as PL_engine_t;

// Engine set return values
pub const PL_ENGINE_SET: c_int = 0;
pub const PL_ENGINE_INVAL: c_int = 2;
pub const PL_ENGINE_INUSE: c_int = 3;

// Query flags (PL_Q_*)
pub const PL_Q_NORMAL: c_int = 0x0002;
pub const PL_Q_NODEBUG: c_int = 0x0004;
pub const PL_Q_CATCH_EXCEPTION: c_int = 0x0008;
pub const PL_Q_PASS_EXCEPTION: c_int = 0x0010;
pub const PL_Q_ALLOW_YIELD: c_int = 0x0020;
pub const PL_Q_EXT_STATUS: c_int = 0x0040;

// Term types (PL_term_type return values)
pub const PL_VARIABLE: c_int = 1;
pub const PL_ATOM: c_int = 2;
pub const PL_INTEGER: c_int = 3;
pub const PL_RATIONAL: c_int = 4;
pub const PL_FLOAT: c_int = 5;
pub const PL_STRING: c_int = 6;
pub const PL_TERM: c_int = 7;
pub const PL_NIL: c_int = 8;
pub const PL_BLOB: c_int = 9;
pub const PL_LIST_PAIR: c_int = 10;
pub const PL_DICT: c_int = 44;

// Foreign predicate flags (PL_FA_*)
pub const PL_FA_NOTRACE: c_int = 0x01;
pub const PL_FA_TRANSPARENT: c_int = 0x02;
pub const PL_FA_NONDETERMINISTIC: c_int = 0x04;
pub const PL_FA_VARARGS: c_int = 0x08;
pub const PL_FA_ISO: c_int = 0x20;
pub const PL_FA_META: c_int = 0x40;

// CVT flags for PL_get_chars
pub const CVT_ATOM: c_int = 0x00000001;
pub const CVT_STRING: c_int = 0x00000002;
pub const CVT_LIST: c_int = 0x00000004;
pub const CVT_INTEGER: c_int = 0x00000008;
pub const CVT_RATIONAL: c_int = 0x00000010;
pub const CVT_FLOAT: c_int = 0x00000020;
pub const CVT_VARIABLE: c_int = 0x00000040;
pub const CVT_NUMBER: c_int = CVT_INTEGER | CVT_RATIONAL | CVT_FLOAT;
pub const CVT_ATOMIC: c_int = CVT_NUMBER | CVT_ATOM | CVT_STRING;
pub const CVT_WRITE: c_int = 0x00000080;
pub const CVT_WRITE_CANONICAL: c_int = 0x00000100;
pub const CVT_WRITEQ: c_int = 0x00000200;
pub const CVT_ALL: c_int = CVT_ATOMIC | CVT_LIST;

// Buffer flags
pub const BUF_DISCARDABLE: c_int = 0x00000000;
pub const BUF_STACK: c_int = 0x00010000;
pub const BUF_MALLOC: c_int = 0x00020000;
pub const BUF_ALLOW_STACK: c_int = 0x00040000;

// Encoding flags
pub const REP_ISO_LATIN_1: c_int = 0x00000000;
pub const REP_UTF8: c_int = 0x00100000;
pub const REP_MB: c_int = 0x00200000;

// =============================================================================
// FFI Function Declarations
// =============================================================================

extern "C" {
    // =========================================================================
    // Initialization and Cleanup
    // =========================================================================

    /// Initialize the Prolog engine
    /// Must be called before any other Prolog operations
    pub fn PL_initialise(argc: c_int, argv: *mut *mut c_char) -> c_int;

    /// Check if Prolog is initialized
    pub fn PL_is_initialised(argc: *mut c_int, argv: *mut *mut *mut c_char) -> c_int;

    /// Cleanup the Prolog engine
    pub fn PL_cleanup(status: c_int) -> c_int;

    /// Halt the Prolog system (calls cleanup then exit)
    pub fn PL_halt(status: c_int) -> c_int;

    // =========================================================================
    // Engine Management (for session isolation)
    // =========================================================================

    /// Create a new Prolog engine
    /// Returns NULL on failure
    pub fn PL_create_engine(attributes: *mut c_void) -> PL_engine_t;

    /// Set the current engine, returns old engine in *old
    /// Returns PL_ENGINE_SET on success
    pub fn PL_set_engine(engine: PL_engine_t, old: *mut PL_engine_t) -> c_int;

    /// Destroy a Prolog engine
    pub fn PL_destroy_engine(engine: PL_engine_t) -> c_int;

    /// Get the current engine
    pub fn PL_current_engine() -> PL_engine_t;

    // =========================================================================
    // Term Reference Management
    // =========================================================================

    /// Allocate a single term reference
    pub fn PL_new_term_ref() -> term_t;

    /// Allocate n term references, returns handle to first
    pub fn PL_new_term_refs(n: c_int) -> term_t;

    /// Copy a term reference
    pub fn PL_copy_term_ref(from: term_t) -> term_t;

    /// Reset term references (for reuse within a foreign frame)
    pub fn PL_reset_term_refs(r: term_t);

    // =========================================================================
    // Putting Values into Terms
    // =========================================================================

    /// Put a variable into term
    pub fn PL_put_variable(t: term_t) -> c_int;

    /// Put an atom into term
    pub fn PL_put_atom(t: term_t, a: atom_t) -> c_int;

    /// Put an atom from C string into term
    pub fn PL_put_atom_chars(t: term_t, chars: *const c_char) -> c_int;

    /// Put an integer into term
    pub fn PL_put_integer(t: term_t, i: i64) -> c_int;

    /// Put a float into term
    pub fn PL_put_float(t: term_t, f: f64) -> c_int;

    /// Put a string into term
    pub fn PL_put_string_chars(t: term_t, chars: *const c_char) -> c_int;

    /// Put nil (empty list) into term
    pub fn PL_put_nil(l: term_t) -> c_int;

    /// Copy term value
    pub fn PL_put_term(t1: term_t, t2: term_t) -> c_int;

    /// Put a functor into term (creates compound with unbound args)
    pub fn PL_put_functor(t: term_t, f: functor_t) -> c_int;

    /// Construct a list cell [H|T]
    pub fn PL_cons_list(l: term_t, h: term_t, t: term_t) -> c_int;

    // =========================================================================
    // Getting Values from Terms
    // =========================================================================

    /// Get atom from term
    pub fn PL_get_atom(t: term_t, a: *mut atom_t) -> c_int;

    /// Get atom as C string (pointer valid until next call)
    pub fn PL_get_atom_chars(t: term_t, a: *mut *mut c_char) -> c_int;

    /// Get string from term
    pub fn PL_get_string(t: term_t, s: *mut *mut c_char, len: *mut usize) -> c_int;

    /// Get chars with conversion options
    pub fn PL_get_chars(t: term_t, s: *mut *mut c_char, flags: c_int) -> c_int;

    /// Get integer from term
    pub fn PL_get_integer(t: term_t, i: *mut c_int) -> c_int;

    /// Get 64-bit integer from term
    pub fn PL_get_int64(t: term_t, i: *mut i64) -> c_int;

    /// Get float from term
    pub fn PL_get_float(t: term_t, f: *mut f64) -> c_int;

    /// Get functor of compound term
    pub fn PL_get_functor(t: term_t, f: *mut functor_t) -> c_int;

    /// Get argument of compound term (1-indexed)
    pub fn PL_get_arg(index: c_int, t: term_t, a: term_t) -> c_int;

    /// Get head and tail of a list
    pub fn PL_get_list(l: term_t, h: term_t, t: term_t) -> c_int;

    /// Get head of a list
    pub fn PL_get_head(l: term_t, h: term_t) -> c_int;

    /// Get tail of a list
    pub fn PL_get_tail(l: term_t, t: term_t) -> c_int;

    /// Check if term is nil
    pub fn PL_get_nil(l: term_t) -> c_int;

    // =========================================================================
    // Type Checking
    // =========================================================================

    /// Get the type of a term
    pub fn PL_term_type(t: term_t) -> c_int;

    /// Check if term is a variable
    pub fn PL_is_variable(t: term_t) -> c_int;

    /// Check if term is an atom
    pub fn PL_is_atom(t: term_t) -> c_int;

    /// Check if term is an integer
    pub fn PL_is_integer(t: term_t) -> c_int;

    /// Check if term is a float
    pub fn PL_is_float(t: term_t) -> c_int;

    /// Check if term is a number
    pub fn PL_is_number(t: term_t) -> c_int;

    /// Check if term is a string
    pub fn PL_is_string(t: term_t) -> c_int;

    /// Check if term is a compound term
    pub fn PL_is_compound(t: term_t) -> c_int;

    /// Check if term is callable (atom or compound)
    pub fn PL_is_callable(t: term_t) -> c_int;

    /// Check if term is a list (including nil)
    pub fn PL_is_list(t: term_t) -> c_int;

    /// Check if term is a proper list pair
    pub fn PL_is_pair(t: term_t) -> c_int;

    /// Check if term is atomic
    pub fn PL_is_atomic(t: term_t) -> c_int;

    /// Check if term is ground (no unbound variables)
    pub fn PL_is_ground(t: term_t) -> c_int;

    // =========================================================================
    // Unification
    // =========================================================================

    /// Unify two terms
    pub fn PL_unify(t1: term_t, t2: term_t) -> c_int;

    /// Unify term with atom
    pub fn PL_unify_atom(t: term_t, a: atom_t) -> c_int;

    /// Unify term with atom from C string
    pub fn PL_unify_atom_chars(t: term_t, chars: *const c_char) -> c_int;

    /// Unify term with integer
    pub fn PL_unify_integer(t: term_t, n: i64) -> c_int;

    /// Unify term with float
    pub fn PL_unify_float(t: term_t, f: f64) -> c_int;

    /// Unify term with string
    pub fn PL_unify_string_chars(t: term_t, chars: *const c_char) -> c_int;

    /// Unify term with nil
    pub fn PL_unify_nil(l: term_t) -> c_int;

    /// Unify term with functor (creates compound)
    pub fn PL_unify_functor(t: term_t, f: functor_t) -> c_int;

    /// Unify argument of compound term
    pub fn PL_unify_arg(index: c_int, t: term_t, a: term_t) -> c_int;

    /// Unify with list cell
    pub fn PL_unify_list(l: term_t, h: term_t, t: term_t) -> c_int;

    // =========================================================================
    // Atom and Functor Handling
    // =========================================================================

    /// Create or find atom from C string
    pub fn PL_new_atom(s: *const c_char) -> atom_t;

    /// Get C string from atom
    pub fn PL_atom_chars(a: atom_t) -> *const c_char;

    /// Create or find functor from atom and arity
    pub fn PL_new_functor(name: atom_t, arity: c_int) -> functor_t;

    /// Get name atom of functor
    pub fn PL_functor_name(f: functor_t) -> atom_t;

    /// Get arity of functor
    pub fn PL_functor_arity(f: functor_t) -> c_int;

    // =========================================================================
    // Query Execution
    // =========================================================================

    /// Get predicate handle
    pub fn PL_predicate(name: *const c_char, arity: c_int, module: *const c_char) -> predicate_t;

    /// Open a query for iterating solutions
    pub fn PL_open_query(m: module_t, flags: c_int, pred: predicate_t, t0: term_t) -> qid_t;

    /// Get next solution (true = success, false = no more)
    pub fn PL_next_solution(qid: qid_t) -> c_int;

    /// Close query (discards remaining solutions)
    pub fn PL_close_query(qid: qid_t) -> c_int;

    /// Cut query (commit to current solution)
    pub fn PL_cut_query(qid: qid_t) -> c_int;

    /// Get current query
    pub fn PL_current_query() -> qid_t;

    // =========================================================================
    // Direct Goal Calling
    // =========================================================================

    /// Parse a goal from C string into term
    pub fn PL_chars_to_term(chars: *const c_char, term: term_t) -> c_int;

    /// Call a goal (single solution)
    pub fn PL_call(t: term_t, m: module_t) -> c_int;

    /// Call a predicate directly
    pub fn PL_call_predicate(
        m: module_t,
        flags: c_int,
        pred: predicate_t,
        t0: term_t,
    ) -> c_int;

    // =========================================================================
    // Exception Handling
    // =========================================================================

    /// Get exception term from query (0 if no exception)
    pub fn PL_exception(qid: qid_t) -> term_t;

    /// Raise an exception
    pub fn PL_raise_exception(exception: term_t) -> c_int;

    /// Clear pending exception
    pub fn PL_clear_exception();

    // =========================================================================
    // Foreign Frame Management (Memory Scoping)
    // =========================================================================

    /// Open a foreign frame (terms allocated in this frame are local)
    pub fn PL_open_foreign_frame() -> fid_t;

    /// Close a foreign frame (terms become accessible to parent)
    pub fn PL_close_foreign_frame(fid: fid_t);

    /// Discard a foreign frame (terms are garbage collected)
    pub fn PL_discard_foreign_frame(fid: fid_t);

    /// Rewind a foreign frame (keep frame open, reset terms)
    pub fn PL_rewind_foreign_frame(fid: fid_t);

    // =========================================================================
    // Foreign Predicate Registration
    // =========================================================================

    /// Register a foreign predicate
    /// name: predicate name
    /// arity: number of arguments
    /// func: C function pointer
    /// flags: PL_FA_* flags
    pub fn PL_register_foreign(
        name: *const c_char,
        arity: c_int,
        func: pl_function_t,
        flags: c_int,
        ...
    ) -> c_int;

    /// Register a foreign predicate in a specific module
    pub fn PL_register_foreign_in_module(
        module: *const c_char,
        name: *const c_char,
        arity: c_int,
        func: pl_function_t,
        flags: c_int,
        ...
    ) -> c_int;

    // =========================================================================
    // Thread Support
    // =========================================================================

    /// Attach current thread to a Prolog engine
    pub fn PL_thread_attach_engine(attr: *mut c_void) -> c_int;

    /// Detach and destroy current thread's engine
    pub fn PL_thread_destroy_engine() -> c_int;

    /// Get current thread ID
    pub fn PL_thread_self() -> c_int;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_defined() {
        // Verify constants are defined with expected values
        assert_eq!(PL_ENGINE_SET, 0);
        assert_eq!(PL_Q_NORMAL, 0x0002);
        assert_eq!(PL_VARIABLE, 1);
        assert_eq!(PL_ATOM, 2);
    }
}

//! Type conversion utilities between Rust and Prolog terms

use super::bindings::*;
use crate::error::{PrologError, PrologResult};
use libc::c_char;
use std::ffi::{CStr, CString};

/// Convert a Prolog term to a Rust string representation
///
/// Uses PL_get_chars with CVT_WRITE to get a readable string representation.
/// The returned string is valid until the foreign frame is closed.
///
/// # Safety
/// This function is unsafe because it dereferences raw pointers from FFI.
pub unsafe fn term_to_string(t: term_t) -> PrologResult<String> {
    let mut s: *mut c_char = std::ptr::null_mut();
    let flags = CVT_ALL | CVT_WRITE | BUF_STACK | REP_UTF8;

    if PL_get_chars(t, &mut s, flags) != 0 {
        if s.is_null() {
            return Err(PrologError::NullPointer(
                "PL_get_chars returned null".to_string(),
            ));
        }
        Ok(CStr::from_ptr(s).to_string_lossy().into_owned())
    } else {
        Err(PrologError::ConversionError(
            "Failed to convert term to string".to_string(),
        ))
    }
}

/// Convert a Prolog term to a JSON-compatible value
///
/// Handles atoms, strings, integers, floats, lists, and compounds.
///
/// # Safety
/// This function is unsafe because it calls FFI functions.
pub unsafe fn term_to_json(t: term_t) -> PrologResult<serde_json::Value> {
    let term_type = PL_term_type(t);

    match term_type {
        PL_VARIABLE => {
            // Unbound variable - represent as null or special marker
            Ok(serde_json::Value::Null)
        }
        PL_ATOM => {
            let mut a: atom_t = 0;
            if PL_get_atom(t, &mut a) != 0 {
                let chars = PL_atom_chars(a);
                if chars.is_null() {
                    return Err(PrologError::NullPointer("atom_chars null".to_string()));
                }
                let s = CStr::from_ptr(chars).to_string_lossy().into_owned();
                // Handle special atoms
                if s == "true" {
                    Ok(serde_json::Value::Bool(true))
                } else if s == "false" {
                    Ok(serde_json::Value::Bool(false))
                } else if s == "[]" {
                    Ok(serde_json::Value::Array(vec![]))
                } else {
                    Ok(serde_json::Value::String(s))
                }
            } else {
                Err(PrologError::ConversionError("Failed to get atom".to_string()))
            }
        }
        PL_INTEGER => {
            let mut i: i64 = 0;
            if PL_get_int64(t, &mut i) != 0 {
                Ok(serde_json::Value::Number(i.into()))
            } else {
                Err(PrologError::ConversionError(
                    "Failed to get integer".to_string(),
                ))
            }
        }
        PL_FLOAT => {
            let mut f: f64 = 0.0;
            if PL_get_float(t, &mut f) != 0 {
                Ok(serde_json::json!(f))
            } else {
                Err(PrologError::ConversionError("Failed to get float".to_string()))
            }
        }
        PL_STRING => {
            let mut s: *mut c_char = std::ptr::null_mut();
            let mut len: usize = 0;
            if PL_get_string(t, &mut s, &mut len) != 0 && !s.is_null() {
                let slice = std::slice::from_raw_parts(s as *const u8, len);
                let string = String::from_utf8_lossy(slice).into_owned();
                Ok(serde_json::Value::String(string))
            } else {
                Err(PrologError::ConversionError(
                    "Failed to get string".to_string(),
                ))
            }
        }
        PL_NIL => Ok(serde_json::Value::Array(vec![])),
        PL_LIST_PAIR => {
            // It's a list - convert to JSON array
            let mut result = Vec::new();
            let head = PL_new_term_ref();
            let tail = PL_copy_term_ref(t);

            while PL_get_list(tail, head, tail) != 0 {
                result.push(term_to_json(head)?);
            }

            // Check if it ended with nil (proper list)
            if PL_get_nil(tail) == 0 {
                // Improper list - fall back to string representation
                return Ok(serde_json::Value::String(term_to_string(t)?));
            }

            Ok(serde_json::Value::Array(result))
        }
        PL_TERM => {
            // Compound term - convert to object with functor and args
            let mut f: functor_t = 0;
            if PL_get_functor(t, &mut f) != 0 {
                let name_atom = PL_functor_name(f);
                let arity = PL_functor_arity(f);
                let name_chars = PL_atom_chars(name_atom);

                if name_chars.is_null() {
                    return Err(PrologError::NullPointer("functor name null".to_string()));
                }

                let name = CStr::from_ptr(name_chars).to_string_lossy().into_owned();

                // Special handling for common functors
                if name == "-" && arity == 2 {
                    // Key-Value pair: Key-Value -> {"Key": Value}
                    let key_term = PL_new_term_ref();
                    let val_term = PL_new_term_ref();
                    PL_get_arg(1, t, key_term);
                    PL_get_arg(2, t, val_term);

                    let key = term_to_json(key_term)?;
                    let val = term_to_json(val_term)?;

                    let key_str = match key {
                        serde_json::Value::String(s) => s,
                        _ => term_to_string(key_term)?,
                    };

                    let mut obj = serde_json::Map::new();
                    obj.insert(key_str, val);
                    return Ok(serde_json::Value::Object(obj));
                }

                // General compound: functor(args...) -> {"functor": "name", "args": [...]}
                let mut args = Vec::new();
                for i in 1..=arity {
                    let arg_term = PL_new_term_ref();
                    PL_get_arg(i, t, arg_term);
                    args.push(term_to_json(arg_term)?);
                }

                Ok(serde_json::json!({
                    "functor": name,
                    "args": args
                }))
            } else {
                // Fallback to string representation
                Ok(serde_json::Value::String(term_to_string(t)?))
            }
        }
        _ => {
            // Unknown type - use string representation
            Ok(serde_json::Value::String(term_to_string(t)?))
        }
    }
}

/// Convert a Rust string to a CString for Prolog
pub fn string_to_c_string(s: &str) -> PrologResult<CString> {
    CString::new(s).map_err(|e| PrologError::ConversionError(format!("CString error: {}", e)))
}

/// Safely convert a C string to a Rust string
///
/// # Safety
/// This function is unsafe because it dereferences a raw pointer.
pub unsafe fn c_string_to_string(c_str: *const c_char) -> String {
    if c_str.is_null() {
        return String::new();
    }
    CStr::from_ptr(c_str).to_string_lossy().into_owned()
}

/// Put a JSON value into a Prolog term
///
/// # Safety
/// This function is unsafe because it calls FFI functions.
pub unsafe fn json_to_term(value: &serde_json::Value, t: term_t) -> PrologResult<()> {
    match value {
        serde_json::Value::Null => {
            // Represent null as the atom 'null'
            let null_atom = string_to_c_string("null")?;
            if PL_put_atom_chars(t, null_atom.as_ptr()) == 0 {
                return Err(PrologError::ConversionError(
                    "Failed to put null atom".to_string(),
                ));
            }
        }
        serde_json::Value::Bool(b) => {
            let atom_str = if *b { "true" } else { "false" };
            let c_str = string_to_c_string(atom_str)?;
            if PL_put_atom_chars(t, c_str.as_ptr()) == 0 {
                return Err(PrologError::ConversionError(
                    "Failed to put bool atom".to_string(),
                ));
            }
        }
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if PL_put_integer(t, i) == 0 {
                    return Err(PrologError::ConversionError(
                        "Failed to put integer".to_string(),
                    ));
                }
            } else if let Some(f) = n.as_f64() {
                if PL_put_float(t, f) == 0 {
                    return Err(PrologError::ConversionError(
                        "Failed to put float".to_string(),
                    ));
                }
            }
        }
        serde_json::Value::String(s) => {
            let c_str = string_to_c_string(s)?;
            if PL_put_string_chars(t, c_str.as_ptr()) == 0 {
                return Err(PrologError::ConversionError(
                    "Failed to put string".to_string(),
                ));
            }
        }
        serde_json::Value::Array(arr) => {
            // Build list from end to front
            PL_put_nil(t);
            for item in arr.iter().rev() {
                let head = PL_new_term_ref();
                json_to_term(item, head)?;
                if PL_cons_list(t, head, t) == 0 {
                    return Err(PrologError::ConversionError(
                        "Failed to build list".to_string(),
                    ));
                }
            }
        }
        serde_json::Value::Object(obj) => {
            // Convert object to list of Key-Value pairs
            // Could also use dict{} syntax for SWI-Prolog dicts
            PL_put_nil(t);
            for (key, val) in obj.iter().rev() {
                // Create Key-Value compound
                let pair = PL_new_term_ref();
                let key_term = PL_new_term_ref();
                let val_term = PL_new_term_ref();

                let key_c = string_to_c_string(key)?;
                PL_put_atom_chars(key_term, key_c.as_ptr());
                json_to_term(val, val_term)?;

                // Create -(Key, Value) compound
                let dash_atom = PL_new_atom(string_to_c_string("-")?.as_ptr());
                let dash_functor = PL_new_functor(dash_atom, 2);
                PL_put_functor(pair, dash_functor);
                PL_unify_arg(1, pair, key_term);
                PL_unify_arg(2, pair, val_term);

                // Cons onto list
                if PL_cons_list(t, pair, t) == 0 {
                    return Err(PrologError::ConversionError(
                        "Failed to build object list".to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_to_c_string() {
        let result = string_to_c_string("hello");
        assert!(result.is_ok());
        let c_str = result.unwrap();
        assert_eq!(c_str.to_str().unwrap(), "hello");
    }

    #[test]
    fn test_string_to_c_string_with_null() {
        let result = string_to_c_string("hello\0world");
        assert!(result.is_err());
    }
}

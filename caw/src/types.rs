/// Type system for CAW language
/// Handles type checking and type inference

use crate::ast::{Expression, PrimitiveType, TypeExpr};
use crate::CawResult;
use std::collections::HashMap;

/// Type environment for type checking
pub struct TypeEnv {
    bindings: HashMap<String, TypeExpr>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn bind(&mut self, name: String, type_expr: TypeExpr) {
        self.bindings.insert(name, type_expr);
    }

    pub fn lookup(&self, name: &str) -> Option<&TypeExpr> {
        self.bindings.get(name)
    }

    pub fn clear(&mut self) {
        self.bindings.clear();
    }
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}

/// Type checker
pub struct TypeChecker {
    env: TypeEnv,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            env: TypeEnv::new(),
        }
    }

    /// Check the type of an expression
    pub fn check_expression(&self, _expr: &Expression) -> CawResult<TypeExpr> {
        // TODO: Implement full type checking
        Ok(TypeExpr::Primitive(PrimitiveType::String))
    }

    /// Get the current type environment
    pub fn env(&self) -> &TypeEnv {
        &self.env
    }

    /// Get mutable access to type environment
    pub fn env_mut(&mut self) -> &mut TypeEnv {
        &mut self.env
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

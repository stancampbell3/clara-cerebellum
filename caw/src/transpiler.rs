/// CLIPS Transpiler
/// Converts CAW AST to CLIPS-compatible syntax

use crate::ast::*;

/// Transpiles CAW to CLIPS
pub struct ClipsTranspiler;

impl ClipsTranspiler {
    pub fn new() -> Self {
        Self
    }

    /// Transpile a program to CLIPS
    pub fn transpile_program(&self, program: &Program) -> String {
        let mut output = String::new();

        // Add CLIPS header comment
        output.push_str("; Generated from CAW language\n");
        output.push_str("; CAW v0.1.0\n\n");

        for statement in &program.statements {
            output.push_str(&self.transpile_statement(statement));
            output.push_str("\n");
        }

        output
    }

    fn transpile_statement(&self, stmt: &Statement) -> String {
        match stmt {
            Statement::TypeDecl(td) => self.transpile_type_decl(td),
            Statement::AgentDecl(_ad) => {
                // Agents don't directly map to CLIPS - they're conceptual
                String::new()
            }
            Statement::FeatherDecl(fd) => self.transpile_feather_decl(fd),
            Statement::RuneDecl(rd) => self.transpile_rune_decl(rd),
            Statement::Expression(expr) => self.transpile_expression(expr),
        }
    }

    pub fn transpile_type_decl(&self, td: &TypeDecl) -> String {
        // CLIPS uses deftemplate for structured facts
        match &td.type_expr {
            TypeExpr::Record(fields) => {
                let mut output = format!("(deftemplate {} (\n", td.name);
                for (name, _type_expr) in fields {
                    output.push_str(&format!("  (slot {})\n", name));
                }
                output.push_str(")\n");
                output
            }
            _ => {
                // Simple types don't map directly to CLIPS
                String::new()
            }
        }
    }

    fn transpile_feather_decl(&self, fd: &FeatherDecl) -> String {
        let mut output = format!("(assert ({}", fd.type_name);
        for (key, expr) in &fd.value.fields {
            output.push_str(&format!(" ({} {})", key, self.transpile_expression(expr)));
        }
        output.push_str("))\n");
        output
    }

    fn transpile_rune_decl(&self, rd: &RuneDecl) -> String {
        let mut output = format!("(defrule {} \n", rd.name);
        output.push_str("  \"CAW-generated rule\"\n");

        // Conditions
        output.push_str("  (");
        for (i, cond) in rd.conditions.iter().enumerate() {
            if i > 0 {
                output.push_str("\n  (");
            }
            output.push_str(&self.transpile_expression(cond));
            output.push_str(")");
        }
        output.push_str("\n");

        // Arrow separator
        output.push_str("  =>\n");

        // Actions
        output.push_str("  (");
        for action in &rd.actions {
            output.push_str(&self.transpile_statement(action));
        }
        output.push_str(")\n");

        output.push_str(")\n");
        output
    }

    fn transpile_expression(&self, expr: &Expression) -> String {
        match expr {
            Expression::Literal(lit) => self.transpile_literal(lit),
            Expression::Identifier(id) => id.clone(),
            Expression::FunctionCall(fc) => {
                let mut output = format!("({}", fc.name);
                for arg in &fc.args {
                    output.push_str(" ");
                    output.push_str(&self.transpile_expression(arg));
                }
                output.push_str(")");
                output
            }
            Expression::Record(rec) => {
                let mut output = String::from("(");
                for (i, (key, expr)) in rec.fields.iter().enumerate() {
                    if i > 0 {
                        output.push_str(" ");
                    }
                    output.push_str(&format!("({} {})", key, self.transpile_expression(expr)));
                }
                output.push_str(")");
                output
            }
            Expression::AgentCall(ac) => {
                // Agent calls become function calls in CLIPS
                format!("({}.{}", ac.agent, ac.method)
            }
            Expression::MessageSend(lhs, rhs) => {
                format!("(send {} {})",
                    self.transpile_expression(lhs),
                    self.transpile_expression(rhs))
            }
        }
    }

    fn transpile_literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::String(s) => format!("\"{}\"", s),
            Literal::Number(n) => n.to_string(),
            Literal::Boolean(b) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
        }
    }
}

impl Default for ClipsTranspiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transpile_simple_type() {
        let td = TypeDecl {
            name: "Particle".to_string(),
            type_expr: TypeExpr::Record(vec![
                ("type".to_string(), Box::new(TypeExpr::Primitive(PrimitiveType::String))),
                ("state".to_string(), Box::new(TypeExpr::Primitive(PrimitiveType::String))),
            ]),
        };

        let transpiler = ClipsTranspiler::new();
        let output = transpiler.transpile_type_decl(&td);
        assert!(output.contains("deftemplate Particle"));
        assert!(output.contains("slot type"));
        assert!(output.contains("slot state"));
    }
}

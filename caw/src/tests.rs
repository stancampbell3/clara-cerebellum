/// Comprehensive test suite for CAW language

#[cfg(test)]
mod parser_tests {
    use crate::{CawParser, Statement, TypeExpr};

    #[test]
    fn test_parse_type_declaration_primitive() {
        let input = "type Age = Number";
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
    }

    #[test]
    fn test_parse_type_declaration_record() {
        let input = "type Particle = { type: String, state: String }";
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
        match &program.statements[0] {
            Statement::TypeDecl(td) => {
                assert_eq!(td.name, "Particle");
                match &td.type_expr {
                    TypeExpr::Record(fields) => assert_eq!(fields.len(), 2),
                    _ => panic!("Expected record type"),
                }
            }
            _ => panic!("Expected type declaration"),
        }
    }

    #[test]
    fn test_parse_type_declaration_vector() {
        let input = "type Numbers = [Number]";
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
    }

    #[test]
    fn test_parse_feather_declaration() {
        let input = r#"
type Particle = { type: String, state: String }
feather radium: Particle = { type: "radium", state: "unstable" }
        "#;
        let result = CawParser::parse_program(input);
        // For now, just ensure it parses without error
        // The exact structure of parsed results depends on grammar implementation details
        match result {
            Ok(_) => {}, // Success - grammar can parse it
            Err(e) => panic!("Parse failed: {}", e),
        }
    }

    #[test]
    fn test_parse_agent_declaration() {
        let input = "let albert = Expert(Physics.Nuclear.Particle._)";
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
        let program = result.unwrap();
        assert_eq!(program.statements.len(), 1);
        match &program.statements[0] {
            Statement::AgentDecl(ad) => {
                assert_eq!(ad.name, "albert");
                assert!(ad.domain.wildcard);
                assert_eq!(ad.domain.segments.len(), 3);
            }
            _ => panic!("Expected agent declaration"),
        }
    }

    #[test]
    fn test_parse_agent_declaration_without_wildcard() {
        let input = "let marie = Expert(Chemistry.Nuclear)";
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
        let program = result.unwrap();
        match &program.statements[0] {
            Statement::AgentDecl(ad) => {
                assert!(!ad.domain.wildcard);
                assert_eq!(ad.domain.segments.len(), 2);
            }
            _ => panic!("Expected agent declaration"),
        }
    }

    #[test]
    fn test_parse_agent_with_string_literal() {
        let input = r#"let x = Expert(domain.name._)"#;
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_agent_numeric() {
        let input = "let x = Expert(Math.Numeric._)";
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_agent_simple() {
        let input = "let x = Expert(Test._)";
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_multiple_statements() {
        let input = r#"
type X = String
let y = Expert(A._)
        "#;
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().statements.len(), 2);
    }

    #[test]
    fn test_parse_record_literal() {
        let input = r#"
type Particle = { type: String }
feather radium: Particle = { type: "value" }
        "#;
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_rune_declaration() {
        let input = r#"
rune "TestRule" when
  true
then
  let x = Expert(A._)
        "#;
        let result = CawParser::parse_program(input);
        // Just ensure it parses without error
        match result {
            Ok(_) => {}, // Success
            Err(e) => panic!("Parse failed: {}", e),
        }
    }

    #[test]
    fn test_parse_complete_program() {
        let input = r#"
type Particle = { type: String, state: String }

feather radium: Particle = { type: "radium", state: "unstable" }

let albert = Expert(Physics.Nuclear._)

rune "DecayLaw" when
  true
then
  true
        "#;
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
        let program = result.unwrap();
        assert!(program.statements.len() >= 3);
    }

    #[test]
    fn test_parse_error_invalid_syntax() {
        let input = "type = invalid";
        let result = CawParser::parse_program(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_error_unclosed_brace() {
        let input = "type X = { field: String";
        let result = CawParser::parse_program(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_union_type() {
        let input = "type Status = String | Number";
        let result = CawParser::parse_program(input);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod runtime_tests {
    use crate::{Runtime, Program, Statement, TypeDecl, TypeExpr, PrimitiveType};

    #[test]
    fn test_runtime_creation() {
        let runtime = Runtime::new();
        assert_eq!(runtime.facts().len(), 0);
        assert_eq!(runtime.rules().len(), 0);
        assert_eq!(runtime.agents().len(), 0);
    }

    #[test]
    fn test_execute_empty_program() {
        let mut runtime = Runtime::new();
        let program = Program { statements: vec![] };
        let result = runtime.execute_program(&program);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_type_declaration() {
        let mut runtime = Runtime::new();
        let type_decl = TypeDecl {
            name: "TestType".to_string(),
            type_expr: TypeExpr::Primitive(PrimitiveType::String),
        };
        let program = Program {
            statements: vec![Statement::TypeDecl(type_decl)],
        };
        let result = runtime.execute_program(&program);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_agent_declaration() {
        let mut runtime = Runtime::new();
        use crate::{AgentDecl, DomainPath};

        let agent_decl = AgentDecl {
            name: "test_agent".to_string(),
            domain: DomainPath {
                segments: vec!["Test".to_string(), "Domain".to_string()],
                wildcard: false,
            },
        };
        let program = Program {
            statements: vec![Statement::AgentDecl(agent_decl)],
        };
        let result = runtime.execute_program(&program);
        assert!(result.is_ok());
        assert_eq!(runtime.agents().len(), 1);
        assert!(runtime.agents().contains_key("test_agent"));
    }
}

#[cfg(test)]
mod type_system_tests {
    use crate::types::{TypeEnv, TypeChecker};
    use crate::{TypeExpr, PrimitiveType};

    #[test]
    fn test_type_env_binding() {
        let mut env = TypeEnv::new();
        let type_expr = TypeExpr::Primitive(PrimitiveType::String);
        env.bind("test".to_string(), type_expr.clone());

        assert!(env.lookup("test").is_some());
        assert!(env.lookup("unknown").is_none());
    }

    #[test]
    fn test_type_checker_creation() {
        let checker = TypeChecker::new();
        assert!(checker.env().lookup("anything").is_none());
    }

    #[test]
    fn test_type_env_clear() {
        let mut env = TypeEnv::new();
        env.bind("test".to_string(), TypeExpr::Primitive(PrimitiveType::String));
        assert!(env.lookup("test").is_some());

        env.clear();
        assert!(env.lookup("test").is_none());
    }
}

#[cfg(test)]
mod transpiler_tests {
    use crate::{ClipsTranspiler, TypeDecl, TypeExpr, PrimitiveType};

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

    #[test]
    fn test_transpile_empty_record_type() {
        let td = TypeDecl {
            name: "Empty".to_string(),
            type_expr: TypeExpr::Record(vec![]),
        };

        let transpiler = ClipsTranspiler::new();
        let output = transpiler.transpile_type_decl(&td);
        assert!(output.contains("deftemplate Empty"));
    }

    #[test]
    fn test_transpile_primitive_type() {
        let td = TypeDecl {
            name: "Age".to_string(),
            type_expr: TypeExpr::Primitive(PrimitiveType::Number),
        };

        let transpiler = ClipsTranspiler::new();
        let output = transpiler.transpile_type_decl(&td);
        // Primitive types don't generate output in current implementation
        assert_eq!(output, "");
    }

    #[test]
    fn test_transpile_program() {
        let transpiler = ClipsTranspiler::new();
        let program = crate::Program { statements: vec![] };
        let output = transpiler.transpile_program(&program);
        assert!(output.contains("Generated from CAW language"));
        assert!(output.contains("CAW v0.1.0"));
    }
}

#[cfg(test)]
mod ast_tests {
    use crate::{Program, DomainPath, Literal, Expression};

    #[test]
    fn test_program_creation() {
        let program = Program {
            statements: vec![],
        };
        assert_eq!(program.statements.len(), 0);
    }

    #[test]
    fn test_domain_path_with_wildcard() {
        let domain = DomainPath {
            segments: vec!["A".to_string(), "B".to_string()],
            wildcard: true,
        };
        let output = domain.to_string();
        assert!(output.contains("A.B._"));
    }

    #[test]
    fn test_domain_path_without_wildcard() {
        let domain = DomainPath {
            segments: vec!["A".to_string(), "B".to_string()],
            wildcard: false,
        };
        let output = domain.to_string();
        assert_eq!(output, "A.B");
        assert!(!output.contains("._"));
    }

    #[test]
    fn test_literal_string_display() {
        let lit = Literal::String("test".to_string());
        assert_eq!(lit.to_string(), "\"test\"");
    }

    #[test]
    fn test_literal_number_display() {
        let lit = Literal::Number(42.5);
        assert!(lit.to_string().contains("42.5"));
    }

    #[test]
    fn test_literal_boolean_display() {
        let lit_true = Literal::Boolean(true);
        let lit_false = Literal::Boolean(false);
        assert_eq!(lit_true.to_string(), "true");
        assert_eq!(lit_false.to_string(), "false");
    }

    #[test]
    fn test_expression_identifier_display() {
        let expr = Expression::Identifier("test_var".to_string());
        assert_eq!(expr.to_string(), "test_var");
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::{CawParser, Runtime};

    #[test]
    fn test_parse_and_execute_type_declaration() {
        let input = "type Person = { name: String, age: Number }";
        let program = CawParser::parse_program(input).expect("Parse failed");

        let mut runtime = Runtime::new();
        let result = runtime.execute_program(&program);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_and_execute_agent_declaration() {
        let input = "let alice = Expert(AI.Knowledge._)";
        let program = CawParser::parse_program(input).expect("Parse failed");

        let mut runtime = Runtime::new();
        let result = runtime.execute_program(&program);
        assert!(result.is_ok());
        assert_eq!(runtime.agents().len(), 1);
    }

    #[test]
    fn test_parse_and_execute_full_program() {
        let input = r#"
type Particle = {
  type: String,
  state: String
}

feather radium: Particle = {
  type: "radium",
  state: "unstable"
}

let albert = Expert(Physics.Nuclear._)
        "#;

        let program = CawParser::parse_program(input).expect("Parse failed");
        assert!(program.statements.len() >= 2);

        let mut runtime = Runtime::new();
        let result = runtime.execute_program(&program);
        assert!(result.is_ok());
        assert_eq!(runtime.agents().len(), 1);
    }

    #[test]
    fn test_parse_rune_and_execute() {
        let input = r#"
rune "TestRule" when
  true
then
  let x = Expert(A._)
        "#;

        let program = CawParser::parse_program(input).expect("Parse failed");
        let mut runtime = Runtime::new();
        let result = runtime.execute_program(&program);
        assert!(result.is_ok());
        // At least check that execution doesn't fail
    }
}

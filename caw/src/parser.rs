/// CAW Language Parser
/// Uses pest PEG parser with the grammar defined in caw.pest

use crate::ast::*;
use crate::CawError::ParseError;
use crate::CawResult;
use pest::Parser;

#[derive(Parser)]
#[grammar = "caw.pest"]
pub struct CawParser;

impl CawParser {
    /// Parse a complete CAW program
    pub fn parse_program(input: &str) -> CawResult<Program> {
        let pairs = Self::parse(Rule::program, input)
            .map_err(|e| ParseError(format!("Failed to parse program: {}", e)))?;

        let mut statements = Vec::new();

        for pair in pairs {
            match pair.as_rule() {
                Rule::statement => {
                    if let Ok(stmt) = Self::parse_statement(pair) {
                        statements.push(stmt);
                    }
                }
                Rule::EOI => break,
                _ => {}
            }
        }

        Ok(Program { statements })
    }

    fn parse_statement(pair: pest::iterators::Pair<Rule>) -> CawResult<Statement> {
        let inner = pair.into_inner().next().ok_or_else(|| {
            ParseError("Empty statement".to_string())
        })?;

        match inner.as_rule() {
            Rule::type_decl => Self::parse_type_decl(inner),
            Rule::agent_decl => Self::parse_agent_decl(inner),
            Rule::feather_decl => Self::parse_feather_decl(inner),
            Rule::rune_decl => Self::parse_rune_decl(inner),
            Rule::expression => Self::parse_expression(inner).map(Statement::Expression),
            _ => Err(ParseError(format!("Unexpected statement rule: {:?}", inner.as_rule()))),
        }
    }

    fn parse_type_decl(pair: pest::iterators::Pair<Rule>) -> CawResult<Statement> {
        let mut inner = pair.into_inner();
        let name = inner.next().ok_or_else(|| ParseError("Missing type name".to_string()))?
            .as_str()
            .to_string();
        let type_expr = Self::parse_type_expr(inner.next().ok_or_else(|| ParseError("Missing type expression".to_string()))?)?;

        Ok(Statement::TypeDecl(TypeDecl { name, type_expr }))
    }

    fn parse_type_expr(pair: pest::iterators::Pair<Rule>) -> CawResult<TypeExpr> {
        let mut inner = pair.into_inner();

        // Parse first type
        let first = Self::parse_single_type_expr(inner.next().ok_or_else(|| {
            ParseError("Empty type expression".to_string())
        })?)?;

        // Check for unions (|)
        let mut result = first;
        for expr_pair in inner {
            if expr_pair.as_rule() == Rule::vector_or_func_or_record_or_prim {
                let union_type = Self::parse_single_type_expr(expr_pair)?;
                result = TypeExpr::Union(Box::new(result), Box::new(union_type));
            }
        }

        Ok(result)
    }

    fn parse_single_type_expr(pair: pest::iterators::Pair<Rule>) -> CawResult<TypeExpr> {
        let inner = pair.into_inner().next().ok_or_else(|| {
            ParseError("Empty type expression".to_string())
        })?;

        match inner.as_rule() {
            Rule::primitive_type => {
                let prim = match inner.as_str() {
                    "String" => PrimitiveType::String,
                    "Number" => PrimitiveType::Number,
                    "Boolean" => PrimitiveType::Boolean,
                    _ => return Err(ParseError(format!("Unknown primitive type: {}", inner.as_str()))),
                };
                Ok(TypeExpr::Primitive(prim))
            }
            Rule::record_type => {
                let mut fields = Vec::new();
                for field in inner.into_inner() {
                    if field.as_rule() == Rule::field_type {
                        let mut parts = field.into_inner();
                        let name = parts.next().ok_or_else(|| ParseError("Missing field name".to_string()))?
                            .as_str()
                            .to_string();
                        let type_expr = Self::parse_single_type_expr(parts.next().ok_or_else(|| ParseError("Missing field type".to_string()))?)?;
                        fields.push((name, Box::new(type_expr)));
                    }
                }
                Ok(TypeExpr::Record(fields))
            }
            Rule::vector_type => {
                let type_inner = inner.into_inner().next().ok_or_else(|| ParseError("Missing vector element type".to_string()))?;
                let inner_type = Self::parse_type_expr(type_inner)?;
                Ok(TypeExpr::Vector(Box::new(inner_type)))
            }
            Rule::function_type => {
                let mut inner_parts = inner.into_inner();
                let type_list_pair = inner_parts.next().ok_or_else(|| ParseError("Missing type list".to_string()))?;
                let return_pair = inner_parts.next().ok_or_else(|| ParseError("Missing return type".to_string()))?;

                let mut arg_types = Vec::new();
                if type_list_pair.as_rule() == Rule::type_expr_list {
                    for type_item in type_list_pair.into_inner() {
                        arg_types.push(Self::parse_single_type_expr(type_item)?);
                    }
                }

                let return_type = Self::parse_type_expr(return_pair)?;
                Ok(TypeExpr::Function(arg_types, Box::new(return_type)))
            }
            _ => Err(ParseError(format!("Unsupported type expression: {:?}", inner.as_rule()))),
        }
    }

    fn parse_agent_decl(pair: pest::iterators::Pair<Rule>) -> CawResult<Statement> {
        let mut inner = pair.into_inner();
        let name = inner.next().ok_or_else(|| ParseError("Missing agent name".to_string()))?
            .as_str()
            .to_string();
        let agent_init = inner.next().ok_or_else(|| ParseError("Missing agent init".to_string()))?;

        let mut init_inner = agent_init.into_inner();
        let domain_path_pair = init_inner.next().ok_or_else(|| ParseError("Missing domain path".to_string()))?;
        let domain = Self::parse_domain_path(domain_path_pair)?;

        Ok(Statement::AgentDecl(AgentDecl { name, domain }))
    }

    fn parse_domain_path(pair: pest::iterators::Pair<Rule>) -> CawResult<DomainPath> {
        let text = pair.as_str();
        let wildcard = text.ends_with("._");
        let segments: Vec<String> = text
            .trim_end_matches("._")
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        Ok(DomainPath { segments, wildcard })
    }

    fn parse_feather_decl(pair: pest::iterators::Pair<Rule>) -> CawResult<Statement> {
        let mut inner = pair.into_inner();
        let name = inner.next().ok_or_else(|| ParseError("Missing feather name".to_string()))?
            .as_str()
            .to_string();
        let type_name = inner.next().ok_or_else(|| ParseError("Missing type name".to_string()))?
            .as_str()
            .to_string();
        let value = Self::parse_record(inner.next().ok_or_else(|| ParseError("Missing feather value".to_string()))?)?;

        Ok(Statement::FeatherDecl(FeatherDecl { name, type_name, value }))
    }

    fn parse_rune_decl(pair: pest::iterators::Pair<Rule>) -> CawResult<Statement> {
        let mut inner = pair.into_inner();
        let name = inner.next().ok_or_else(|| ParseError("Missing rune name".to_string()))?
            .as_str()
            .trim_matches('"')
            .to_string();

        let condition_block = inner.next().ok_or_else(|| ParseError("Missing condition block".to_string()))?;
        let mut conditions = Vec::new();
        for expr in condition_block.into_inner() {
            if expr.as_rule() == Rule::expression {
                conditions.push(Self::parse_expression(expr)?);
            }
        }

        let action_block = inner.next().ok_or_else(|| ParseError("Missing action block".to_string()))?;
        let mut actions = Vec::new();
        for stmt in action_block.into_inner() {
            if stmt.as_rule() == Rule::statement {
                actions.push(Self::parse_statement(stmt)?);
            }
        }

        Ok(Statement::RuneDecl(RuneDecl { name, conditions, actions }))
    }

    fn parse_expression(pair: pest::iterators::Pair<Rule>) -> CawResult<Expression> {
        let inner = pair.into_inner().next().ok_or_else(|| {
            ParseError("Empty expression".to_string())
        })?;

        match inner.as_rule() {
            Rule::message_send => {
                let mut parts = inner.into_inner();
                let mut lhs = Self::parse_agent_call(parts.next().ok_or_else(|| ParseError("Missing message LHS".to_string()))?)?;

                for rhs_pair in parts {
                    let rhs = Self::parse_agent_call(rhs_pair)?;
                    lhs = Expression::MessageSend(Box::new(lhs), Box::new(rhs));
                }
                Ok(lhs)
            }
            Rule::agent_call => Self::parse_agent_call(inner),
            _ => Err(ParseError(format!("Unexpected expression rule: {:?}", inner.as_rule()))),
        }
    }

    fn parse_agent_call(pair: pest::iterators::Pair<Rule>) -> CawResult<Expression> {
        let mut parts = pair.into_inner();
        let func_call = Self::parse_function_call(parts.next().ok_or_else(|| ParseError("Missing function call".to_string()))?)?;

        let mut expr = Expression::FunctionCall(func_call);

        for method_pair in parts {
            if method_pair.as_rule() == Rule::function_call {
                let method_call = Self::parse_function_call(method_pair)?;
                // Check if we have a function call that could be converted to agent call
                if let Expression::FunctionCall(ref func) = expr {
                    if func.args.is_empty() {
                        let agent_name = func.name.clone();
                        expr = Expression::AgentCall(AgentCall {
                            agent: agent_name,
                            method: method_call.name,
                            args: method_call.args,
                        });
                        continue;
                    }
                }
            }
        }
        Ok(expr)
    }

    fn parse_function_call(pair: pest::iterators::Pair<Rule>) -> CawResult<FunctionCall> {
        let mut inner = pair.into_inner();
        let primary = inner.next().ok_or_else(|| ParseError("Missing function name".to_string()))?;

        let name = if primary.as_rule() == Rule::primary {
            let primary_inner = primary.into_inner().next().ok_or_else(|| ParseError("Missing primary".to_string()))?;
            if primary_inner.as_rule() == Rule::identifier {
                primary_inner.as_str().to_string()
            } else {
                return Err(ParseError("Expected identifier as function name".to_string()));
            }
        } else {
            return Err(ParseError("Expected primary in function call".to_string()));
        };

        let mut args = Vec::new();
        for arg_pair in inner {
            if arg_pair.as_rule() == Rule::arg_list {
                for arg in arg_pair.into_inner() {
                    if arg.as_rule() == Rule::expression {
                        args.push(Self::parse_expression(arg)?);
                    }
                }
            }
        }

        Ok(FunctionCall { name, args })
    }

    fn parse_record(pair: pest::iterators::Pair<Rule>) -> CawResult<Record> {
        let mut fields = Vec::new();
        for field_pair in pair.into_inner() {
            if field_pair.as_rule() == Rule::field_list {
                for field in field_pair.into_inner() {
                    if field.as_rule() == Rule::field_assignment {
                        let mut parts = field.into_inner();
                        let name = parts.next().ok_or_else(|| ParseError("Missing field name".to_string()))?
                            .as_str()
                            .to_string();
                        let value = Self::parse_expression(parts.next().ok_or_else(|| ParseError("Missing field value".to_string()))?)?;
                        fields.push((name, value));
                    }
                }
            }
        }
        Ok(Record { fields })
    }
}

// omni-compiler/src/lib.rs
// Public API of the Omni compiler frontend

pub mod token;
pub mod lexer;
pub mod ast;
pub mod parser;
pub mod types;
pub mod symbol_table;
pub mod semantic;
pub mod bytecode;
pub mod codegen;

use lexer::Lexer;
use parser::Parser;
use semantic::Analyzer;

/// Full compilation pipeline: lex → parse → semantic analysis → annotated AST.
/// Returns errors as a combined string if anything fails.
pub fn compile(source: &str) -> Result<ast::Program, String> {
    let mut all_tokens = Vec::new();
    let mut imported_files = std::collections::HashSet::new();
    
    // Internal helper for recursive imports
    fn resolve_imports(source: &str, all_tokens: &mut Vec<lexer::SpannedToken>, imported: &mut std::collections::HashSet<String>, is_main: bool) -> Result<(), String> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().map_err(|e| format!("Lex error: {:?}", e))?;
        
        let mut i = 0;
        let mut remaining_tokens = Vec::new();
        let mut main_namespace_tokens = Vec::new();
        
        while i < tokens.len() {
            match &tokens[i].token {
                token::Token::Import => {
                    if i + 2 < tokens.len() {
                        if let token::Token::StringLiteral(ref path) = tokens[i+1].token {
                            if tokens[i+2].token == token::Token::Semicolon {
                                if !imported.contains(path) {
                                    imported.insert(path.clone());
                                    match std::fs::read_to_string(path) {
                                        Ok(content) => {
                                            resolve_imports(&content, all_tokens, imported, false)?;
                                        }
                                        Err(e) => return Err(format!("Import error: Cannot read '{}': {}", path, e)),
                                    }
                                }
                                i += 3;
                                continue;
                            }
                        }
                    }
                    return Err("Invalid import syntax. Expected: import \"filename.omni\";".to_string());
                }
                token::Token::Namespace => {
                    if is_main {
                        // Keep namespace from main file
                        main_namespace_tokens.push(tokens[i].clone()); // namespace
                        i += 1;
                        // Expect identifier(s) and semicolon
                        while i < tokens.len() && tokens[i].token != token::Token::Semicolon {
                            main_namespace_tokens.push(tokens[i].clone());
                            i += 1;
                        }
                        if i < tokens.len() {
                            main_namespace_tokens.push(tokens[i].clone()); // semicolon
                            i += 1;
                        }
                    } else {
                        // Skip namespace in imported files
                        i += 1;
                        while i < tokens.len() && tokens[i].token != token::Token::Semicolon {
                            i += 1;
                        }
                        if i < tokens.len() { i += 1; }
                    }
                }
                token::Token::Eof => {
                    i += 1;
                }
                _ => {
                    remaining_tokens.push(tokens[i].clone());
                    i += 1;
                }
            }
        }
        
        // Ensure namespace stays at the very top of all_tokens
        if is_main && !main_namespace_tokens.is_empty() {
            let mut new_tokens = main_namespace_tokens;
            new_tokens.extend(all_tokens.drain(..));
            new_tokens.extend(remaining_tokens);
            *all_tokens = new_tokens;
        } else {
            all_tokens.extend(remaining_tokens);
        }
        Ok(())
    }

    resolve_imports(source, &mut all_tokens, &mut imported_files, true)?;
    
    // Add a single EOF at the end
    all_tokens.push(lexer::SpannedToken {
        token: token::Token::Eof,
        span: lexer::Span { line: 0, col: 0 },
    });

    let mut parser = Parser::new(all_tokens);
    let program = parser.parse_program().map_err(|e| format!("Parse error: {:?}", e))?;

    // Phase 3: Semantic Analysis
    let mut analyzer = Analyzer::new();
    analyzer.analyze(&program);
    if !analyzer.errors.is_empty() {
        let msgs: Vec<String> = analyzer.errors.iter()
            .map(|e| format!("{:?}", e))
            .collect();
        return Err(format!("Semantic errors:\n{}", msgs.join("\n")));
    }

    Ok(program)
}

// ════════════════════════════════════════════════════════════════════════════
// Unit Tests — Phase 1, 2, and 3
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{Lexer, LexError};
    use crate::token::Token;
    use crate::semantic::{Analyzer, SemanticError};

    fn lex(src: &str) -> Vec<Token> {
        Lexer::new(src)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|st| st.token)
            .collect()
    }

    fn parse_and_analyze(src: &str) -> (ast::Program, Analyzer) {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().expect("lex failed");
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("parse failed");
        let mut analyzer = Analyzer::new();
        analyzer.analyze(&program);
        (program, analyzer)
    }

    fn no_errors(src: &str) {
        let (_, analyzer) = parse_and_analyze(src);
        assert!(
            analyzer.errors.is_empty(),
            "Expected no semantic errors, got: {:?}",
            analyzer.errors
        );
    }

    fn has_error<F>(src: &str, matcher: F)
    where F: Fn(&SemanticError) -> bool {
        let (_, analyzer) = parse_and_analyze(src);
        assert!(
            analyzer.errors.iter().any(|e| matcher(e)),
            "Expected a matching semantic error, got: {:?}",
            analyzer.errors
        );
    }

    // ── Phase 1 & 2 regression tests ─────────────────────────────────────

    #[test]
    fn test_keywords_recognized() {
        let tokens = lex("class function var foreach forall");
        assert_eq!(tokens[0], Token::Class);
        assert_eq!(tokens[1], Token::Function);
        assert_eq!(tokens[2], Token::Var);
        assert_eq!(tokens[3], Token::Foreach);
        assert_eq!(tokens[4], Token::Forall);
    }

    #[test]
    fn test_goto_is_rejected() {
        let mut lexer = Lexer::new("goto label;");
        let result = lexer.tokenize();
        assert!(matches!(result, Err(LexError::GotoForbidden(_))));
    }

    #[test]
    fn test_class_ident_uppercase() {
        let tokens = lex("class Student");
        assert!(matches!(tokens[1], Token::ClassIdent(ref s) if s == "Student"));
    }

    #[test]
    fn test_variable_ident_lowercase() {
        let tokens = lex("var total");
        assert!(matches!(tokens[1], Token::Ident(ref s) if s == "total"));
    }

    #[test]
    fn test_optional_type_question_mark() {
        let tokens = lex("String ?");
        assert!(tokens.contains(&Token::Question));
    }

    #[test]
    fn test_integer_literal() { assert_eq!(lex("42")[0], Token::IntLiteral(42)); }

    #[test]
    fn test_float_literal() { assert_eq!(lex("3.14")[0], Token::FloatLiteral(3.14)); }

    #[test]
    fn test_string_literal() {
        assert!(matches!(lex("\"Hello Omni\"")[0], Token::StringLiteral(ref s) if s == "Hello Omni"));
    }

    #[test]
    fn test_line_comment_ignored() {
        let tokens = lex("var x = 5 // this is ignored\n");
        let non_eof: Vec<_> = tokens.iter().filter(|t| **t != Token::Eof).collect();
        assert_eq!(non_eof.len(), 4);
    }

    #[test]
    fn test_parse_simple_class() {
        let src = r#"class Person {
            private var name : String ;
            public function greet() { print(name); }
        }"#;
        compile(src).expect("should compile");
    }

    #[test]
    fn test_parse_foreach_loop() {
        // `items` is passed as a parameter so the semantic analyzer sees it as declared.
        let src = r#"class Runner {
            public function run(in items : List) {
                foreach (item in items) { print(item); }
            }
        }"#;
        compile(src).expect("should compile");
    }

    #[test]
    fn test_parse_type_inferred_var() {
        let src = r#"class Calc {
            public function calc() { var total = 0; }
        }"#;
        let prog = compile(src).expect("should compile");
        use crate::ast::{ClassMember, Stmt};
        if let ClassMember::Method(_, m) = &prog.classes[0].members[0] {
            if let Stmt::VarDecl(decl) = &m.body[0] {
                assert!(decl.ty.is_none(), "type inference: ty should be None");
            }
        }
    }

    #[test]
    fn test_parse_closure() {
        let src = r#"class Adder {
            public function makeAdder(in x : Int) {
                return function(y : Int) { return x + y; };
            }
        }"#;
        compile(src).expect("closure should parse cleanly");
    }

    #[test]
    fn test_parse_try_catch_finally() {
        let src = r#"class Processor {
            public function run() throws ProcessingException {
                try { item.process(); }
                catch (NetworkException e) { log("fail"); }
                finally { item.releaseResources(); }
            }
        }"#;
        compile(src).expect("try-catch-finally should parse");
    }

    #[test]
    fn test_naming_violation_class_lowercase_rejected() {
        let src = r#"class person { }"#;
        let result = compile(src);
        assert!(result.is_err(), "lowercase class name must be rejected");
    }

    // ── Phase 3: Semantic Analysis Tests ─────────────────────────────────

    /// Type inference: `var total = 0` should be inferred as Int with no errors.
    #[test]
    fn test_type_inference_int() {
        no_errors(r#"class Calc {
            public function run() { var total = 0; }
        }"#);
    }

    /// Type inference: `var msg = "hello"` → String.
    #[test]
    fn test_type_inference_string() {
        no_errors(r#"class Msg {
            public function run() { var msg = "hello"; }
        }"#);
    }

    /// Null safety: assigning null to a non-Optional type must error.
    #[test]
    fn test_null_to_non_optional_fails() {
        has_error(
            r#"class Null {
                public function run() { var name : String = null; }
            }"#,
            |e| matches!(e, SemanticError::NullToNonOptional { var, .. } if var == "name"),
        );
    }

    /// Null safety: assigning null to an Optional type is allowed.
    #[test]
    fn test_null_to_optional_ok() {
        no_errors(r#"class Opt {
            public function run() { var name : String ? = null; }
        }"#);
    }

    /// Duplicate variable declaration in the same scope must be caught.
    #[test]
    fn test_duplicate_declaration_caught() {
        has_error(
            r#"class Dup {
                public function run() {
                    var x = 1;
                    var x = 2;
                }
            }"#,
            |e| matches!(e, SemanticError::DuplicateDeclaration(n) if n == "x"),
        );
    }

    /// Two classes with the same name must be caught.
    #[test]
    fn test_duplicate_class_caught() {
        has_error(
            r#"class Dog { } class Dog { }"#,
            |e| matches!(e, SemanticError::DuplicateDeclaration(n) if n == "Dog"),
        );
    }

    /// A valid Student Grade Tracker (full whitepaper example) should have zero errors.
    #[test]
    fn test_student_grade_tracker_clean() {
        no_errors(r#"
            class Student {
                private var name : String ;
                private var grade : Int ;
                public Student(in n : String, in g : Int) { name = n; grade = g; }
                public function getGrade() : Int { return grade; }
                public function describe() { print(name); }
            }
        "#);
    }
}

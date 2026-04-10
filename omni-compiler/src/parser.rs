// omni-compiler/src/parser.rs
// Phase 2: Recursive-Descent Parser for the Omni Language
//
// Each method in this file corresponds directly to one non-terminal
// in the Omni EBNF grammar, as designed in the whitepaper.
// The pairwise-disjointness property of the grammar eliminates the
// need for backtracking — the look-ahead of 1 token always suffices.

use crate::ast::*;
use crate::token::Token;
use crate::lexer::SpannedToken;

/// Errors produced during parsing.
#[derive(Debug)]
pub enum ParseError {
    UnexpectedToken { expected: &'static str, found: Token, line: usize, col: usize },
    UnexpectedEof,
    NamingViolation { message: String, line: usize, col: usize },
}

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    // ── Navigation helpers ────────────────────────────────────────────────

    fn current(&self) -> &SpannedToken {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek(&self) -> &Token {
        &self.current().token
    }

    fn advance(&mut self) -> &SpannedToken {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if self.pos < self.tokens.len() - 1 { self.pos += 1; }
        tok
    }

    fn expect(&mut self, expected: Token, desc: &'static str) -> Result<(), ParseError> {
        let st = self.current().clone();
        if st.token == expected {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::UnexpectedToken {
                expected: desc,
                found: st.token.clone(),
                line: st.span.line,
                col: st.span.col,
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        let mut name = String::new();
        loop {
            let st = self.current().clone();
            match &st.token {
                Token::Ident(n) | Token::ClassIdent(n) => {
                    name.push_str(n);
                    self.advance();
                }
                _ => return Err(ParseError::UnexpectedToken {
                    expected: "identifier",
                    found: st.token.clone(),
                    line: st.span.line,
                    col: st.span.col,
                })
            }
            if *self.peek() == Token::DoubleColon {
                self.advance();
                name.push_str("::");
            } else {
                break;
            }
        }
        Ok(name)
    }

    fn expect_class_ident(&mut self) -> Result<String, ParseError> {
        let st = self.current().clone();
        let name = self.expect_ident()?;
        let last_segment = name.split("::").last().unwrap();
        if last_segment.chars().next().unwrap().is_uppercase() {
            Ok(name)
        } else {
            Err(ParseError::NamingViolation {
                message: format!("Class name '{}' must end with an uppercase segment", name),
                line: st.span.line,
                col: st.span.col,
            })
        }
    }

    fn expect_var_ident(&mut self) -> Result<String, ParseError> {
        let st = self.current().clone();
        match &st.token {
            Token::Ident(name) => { let n = name.clone(); self.advance(); Ok(n) }
            Token::ClassIdent(name) => Err(ParseError::NamingViolation {
                message: format!("Variable '{}' must start with a lowercase letter", name),
                line: st.span.line,
                col: st.span.col,
            }),
            _ => Err(ParseError::UnexpectedToken {
                expected: "variable name (lowercase)",
                found: st.token.clone(),
                line: st.span.line,
                col: st.span.col,
            })
        }
    }

    // ── Top-level: Program ───────────────────────────────────────────────

    /// Entry point. Parses a complete Omni source file.
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut imports = Vec::new();
        let mut namespace = None;
        let mut interfaces = Vec::new();
        let mut classes = Vec::new();

        // Parse imports and namespace in any order
        loop {
            match self.peek() {
                Token::Import => {
                    self.advance();
                    let name = self.expect_ident()?;
                    self.expect(Token::Semicolon, "';' after import")?;
                    imports.push(name);
                }
                Token::Namespace => {
                    self.advance();
                    let name = self.expect_ident()?;
                    self.expect(Token::Semicolon, "';' after namespace")?;
                    namespace = Some(name);
                }
                _ => break,
            }
        }

        // Class and Interface definitions
        while *self.peek() != Token::Eof {
            if *self.peek() == Token::Interface {
                interfaces.push(self.parse_interface_def()?);
            } else {
                classes.push(self.parse_class_def()?);
            }
        }

        Ok(Program { imports, namespace, interfaces, classes })
    }

    // ── EBNF: <interface_def> ────────────────────────────────────────────
    // <interface_def> → interface <id> [extends <ident_list>] '{' { <method_decl> } '}'

    fn parse_interface_def(&mut self) -> Result<InterfaceDef, ParseError> {
        self.expect(Token::Interface, "'interface'")?;
        let name = self.expect_class_ident()?;

        let mut type_params = Vec::new();
        if *self.peek() == Token::LAngle {
            self.advance();
            loop {
                let param_name = self.expect_ident()?;
                let mut bound = None;
                if *self.peek() == Token::Extends {
                    self.advance();
                    bound = Some(self.expect_class_ident()?);
                }
                type_params.push(TypeParam { name: param_name, bound });

                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RAngle, "'>'")?;
        }

        let extends = if *self.peek() == Token::Extends {
            self.advance();
            self.parse_ident_list()?
        } else {
            Vec::new()
        };

        self.expect(Token::LBrace, "'{'")?;
        let mut methods = Vec::new();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            methods.push(self.parse_method_decl()?);
        }
        self.expect(Token::RBrace, "'}'")?;

        Ok(InterfaceDef { name, type_params, extends, methods })
    }

    fn parse_method_decl(&mut self) -> Result<MethodDecl, ParseError> {
        if *self.peek() == Token::Public {
            self.advance();
        }
        self.expect(Token::Function, "'function'")?;
        let name = self.expect_ident()?;

        self.expect(Token::LParen, "'('")?;
        let params = if *self.peek() != Token::RParen {
            self.parse_param_list()?
        } else {
            Vec::new()
        };
        self.expect(Token::RParen, "')'")?;

        let return_type = if *self.peek() == Token::Colon {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        let throws = if *self.peek() == Token::Throws {
            self.advance();
            self.parse_ident_list()?
        } else {
            Vec::new()
        };

        self.expect(Token::Semicolon, "';' after interface method declaration")?;

        Ok(MethodDecl { name, params, throws, return_type })
    }

    // ── EBNF: <class_def> ────────────────────────────────────────────────
    // <class_def> → class <id> [extends <id>] [implements <ident_list>]
    //               '{' { <class_member> } '}'

    fn parse_class_def(&mut self) -> Result<ClassDef, ParseError> {
        self.expect(Token::Class, "'class'")?;
        let name = self.expect_class_ident()?;

        let mut type_params = Vec::new();
        if *self.peek() == Token::LAngle {
            self.advance();
            loop {
                // e.g. T
                let param_name = self.expect_ident()?;
                let mut bound = None;
                // [extends Comparable]
                if *self.peek() == Token::Extends {
                    self.advance();
                    bound = Some(self.expect_class_ident()?);
                }
                type_params.push(TypeParam { name: param_name, bound });

                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RAngle, "'>'")?;
        }

        let extends = if *self.peek() == Token::Extends {
            self.advance();
            Some(self.expect_class_ident()?)
        } else {
            None
        };

        let implements = if *self.peek() == Token::Implements {
            self.advance();
            self.parse_ident_list()?
        } else {
            Vec::new()
        };

        self.expect(Token::LBrace, "'{'")?;
        let mut members = Vec::new();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            members.push(self.parse_class_member()?);
        }
        self.expect(Token::RBrace, "'}'")?;

        Ok(ClassDef { name, type_params, extends, implements, members })
    }

    // ── EBNF: <class_member> ─────────────────────────────────────────────
    // <class_member> → <access_mod> <method_def> | <access_mod> <var_decl> ;

    fn parse_class_member(&mut self) -> Result<ClassMember, ParseError> {
        let access = self.parse_access_mod()?;
        match self.peek() {
            Token::Function => Ok(ClassMember::Method(access, self.parse_method_def()?)),
            Token::Var => {
                let decl = self.parse_var_decl()?;
                self.expect(Token::Semicolon, "';' after field declaration")?;
                Ok(ClassMember::Field(access, decl))
            }
            // Constructor: same name as class (ClassIdent followed by '(')
            Token::ClassIdent(_) => Ok(ClassMember::Method(access, self.parse_method_def()?)),
            _ => {
                let st = self.current().clone();
                Err(ParseError::UnexpectedToken {
                    expected: "'function' or 'var'",
                    found: st.token.clone(),
                    line: st.span.line,
                    col: st.span.col,
                })
            }
        }
    }

    // ── EBNF: <access_mod> ───────────────────────────────────────────────

    fn parse_access_mod(&mut self) -> Result<AccessMod, ParseError> {
        let st = self.current().clone();
        match st.token {
            Token::Public    => { self.advance(); Ok(AccessMod::Public) }
            Token::Private   => { self.advance(); Ok(AccessMod::Private) }
            Token::Protected => { self.advance(); Ok(AccessMod::Protected) }
            _ => Err(ParseError::UnexpectedToken {
                expected: "access modifier (public | private | protected)",
                found: st.token.clone(),
                line: st.span.line,
                col: st.span.col,
            })
        }
    }

    // ── EBNF: <method_def> ───────────────────────────────────────────────
    // <method_def> → function <id> ( [<param_list>] ) [throws <ident_list>] <block>

    fn parse_method_def(&mut self) -> Result<MethodDef, ParseError> {
        // Allow constructor-style name (ClassIdent) or function keyword
        let name = if *self.peek() == Token::Function {
            self.advance();
            self.expect_ident()?
        } else {
            self.expect_ident()?
        };

        self.expect(Token::LParen, "'('")?;
        let params = if *self.peek() == Token::RParen {
            Vec::new()
        } else {
            self.parse_param_list()?
        };
        self.expect(Token::RParen, "')'")?;

        let return_type = if *self.peek() == Token::Colon {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        let throws = if *self.peek() == Token::Throws {
            self.advance();
            self.parse_ident_list()?
        } else {
            Vec::new()
        };

        let body = self.parse_block()?;

        Ok(MethodDef { name, params, throws, return_type, body })
    }

    // ── EBNF: <param_list> ───────────────────────────────────────────────

    fn parse_param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        if *self.peek() == Token::RParen {
            return Ok(Vec::new());
        }
        let mut params = vec![self.parse_param()?];
        while *self.peek() == Token::Comma {
            self.advance();
            params.push(self.parse_param()?);
        }
        Ok(params)
    }

    fn parse_closure_param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        if *self.peek() == Token::RParen {
            return Ok(Vec::new());
        }
        let mut params = vec![self.parse_closure_param()?];
        while *self.peek() == Token::Comma {
            self.advance();
            params.push(self.parse_closure_param()?);
        }
        Ok(params)
    }

    fn parse_closure_param(&mut self) -> Result<Param, ParseError> {
        let name = self.expect_var_ident()?;
        let ty = if *self.peek() == Token::Colon {
            self.advance();
            self.parse_type_expr()?
        } else {
            TypeExpr::Named { name: "method".to_string(), type_args: vec![], optional: false }
        };
        Ok(Param { is_in_mode: false, name, ty })
    }

    fn parse_param(&mut self) -> Result<Param, ParseError> {
        let is_in_mode = if *self.peek() == Token::In {
            self.advance();
            true
        } else {
            false
        };
        let name = self.expect_var_ident()?;
        self.expect(Token::Colon, "':' after parameter name")?;
        let ty = self.parse_type_expr()?;
        Ok(Param { is_in_mode, name, ty })
    }

    // ── EBNF: <block> ────────────────────────────────────────────────────

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        self.expect(Token::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            stmts.push(self.parse_statement()?);
        }
        self.expect(Token::RBrace, "'}'")?;
        Ok(stmts)
    }

    // ── EBNF: <statement> ────────────────────────────────────────────────
    // <statement> → <assignment_stmt> ; | <selection_stmt>
    //             | <iteration_stmt>  | <method_call> ;

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        match self.peek().clone() {
            Token::Var     => self.parse_var_decl_stmt(),
            Token::Return  => self.parse_return_stmt(),
            Token::If      => self.parse_if_stmt(),
            Token::Foreach => self.parse_foreach_stmt(),
            Token::Forall  => self.parse_forall_stmt(),
            Token::Try     => self.parse_try_stmt(),
            Token::Throw   => self.parse_throw_stmt(),
            Token::Monitor => self.parse_monitor_stmt(),
            Token::Switch  => self.parse_switch_stmt(),
            _              => self.parse_expr_or_assign_stmt(),
        }
    }

    fn parse_var_decl_stmt(&mut self) -> Result<Stmt, ParseError> {
        let decl = self.parse_var_decl()?;
        self.expect(Token::Semicolon, "';' after var declaration")?;
        Ok(Stmt::VarDecl(decl))
    }

    // ── EBNF: <var_decl> ─────────────────────────────────────────────────
    // <var_decl> → var <id> : <type> [?] | var <id> = <expr>

    fn parse_var_decl(&mut self) -> Result<VarDecl, ParseError> {
        self.expect(Token::Var, "'var'")?;
        let name = self.expect_var_ident()?;

        if *self.peek() == Token::Colon {
            // Explicit type declaration
            self.advance();
            let ty = self.parse_type_expr()?;
            let optional = ty.is_optional();
            let initializer = if *self.peek() == Token::Assign {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };
            Ok(VarDecl { name, ty: Some(ty), optional, initializer })
        } else {
            // Type inference: var x = expr
            self.expect(Token::Assign, "'=' for type-inferred var")?;
            let initializer = self.parse_expr()?;
            Ok(VarDecl { name, ty: None, optional: false, initializer: Some(initializer) })
        }
    }

    fn parse_return_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // consume 'return'
        let val = if *self.peek() != Token::Semicolon {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(Token::Semicolon, "';' after return")?;
        Ok(Stmt::Return(val))
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'if'
        self.expect(Token::LParen, "'('")?;
        let cond = self.parse_expr()?;
        self.expect(Token::RParen, "')'")?;
        let then_block = self.parse_block()?;
        let else_block = if *self.peek() == Token::Else {
            self.advance();
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Stmt::If { cond, then_block, else_block })
    }

    // ── EBNF: <iteration_stmt> ───────────────────────────────────────────
    // <iteration_stmt> → foreach (<id> in <collection>) <block>

    fn parse_foreach_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'foreach'
        self.expect(Token::LParen, "'('")?;
        let var = self.expect_var_ident()?;
        self.expect(Token::In, "'in'")?;
        let collection = self.parse_expr()?;
        self.expect(Token::RParen, "')'")?;
        let body = self.parse_block()?;
        Ok(Stmt::Foreach { var, collection, body })
    }

    fn parse_forall_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'forall'
        self.expect(Token::LParen, "'('")?;
        let var = self.expect_var_ident()?;
        self.expect(Token::Assign, "'='")?;
        let start = self.parse_expr()?;
        self.expect(Token::To, "'to'")?;
        let end = self.parse_expr()?;
        self.expect(Token::RParen, "')'")?;
        let body = self.parse_block()?;
        Ok(Stmt::Forall { var, start, end, body })
    }

    fn parse_try_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'try'
        let try_block = self.parse_block()?;
        let mut catches = Vec::new();
        while *self.peek() == Token::Catch {
            self.advance();
            self.expect(Token::LParen, "'('")?;
            let etype = self.expect_class_ident()?;
            let binding = self.expect_var_ident()?;
            self.expect(Token::RParen, "')'")?;
            let body = self.parse_block()?;
            catches.push(CatchClause { exception_type: etype, binding, body });
        }
        let finally_block = if *self.peek() == Token::Finally {
            self.advance();
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Stmt::TryCatch { try_block, catches, finally_block })
    }

    fn parse_throw_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'throw'
        let expr = self.parse_expr()?;
        self.expect(Token::Semicolon, "';' after throw")?;
        Ok(Stmt::Throw(expr))
    }

    fn parse_monitor_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'monitor'
        self.expect(Token::LParen, "'('")?;
        let target = self.parse_expr()?;
        self.expect(Token::RParen, "')'")?;
        let body = self.parse_block()?;
        Ok(Stmt::Monitor { target, body })
    }

    fn parse_switch_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // 'switch'
        self.expect(Token::LParen, "'('")?;
        let condition = self.parse_expr()?;
        self.expect(Token::RParen, "')'")?;
        self.expect(Token::LBrace, "'{'")?;
        let mut cases = Vec::new();
        let mut default_case = None;
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            if *self.peek() == Token::Case {
                self.advance();
                let value = self.parse_expr()?;
                self.expect(Token::Colon, "':' after case value")?;
                let body = self.parse_block()?;
                cases.push(SwitchCase { value, body });
            } else if *self.peek() == Token::Default {
                self.advance();
                self.expect(Token::Colon, "':' after default")?;
                default_case = Some(self.parse_block()?);
                break; 
            } else {
                break;
            }
        }
        self.expect(Token::RBrace, "'}'")?;
        Ok(Stmt::Switch { condition, cases, default_case })
    }

    fn parse_expr_or_assign_stmt(&mut self) -> Result<Stmt, ParseError> {
        let lhs = self.parse_expr()?;
        if *self.peek() == Token::Assign {
            self.advance();
            let rhs = self.parse_expr()?;
            self.expect(Token::Semicolon, "';' after assignment")?;
            Ok(Stmt::Assign { target: lhs, value: rhs })
        } else {
            self.expect(Token::Semicolon, "';' after expression statement")?;
            Ok(Stmt::ExprStmt(lhs))
        }
    }

    // ── Expressions (Pratt / recursive precedence) ────────────────────────

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_and_expr()?;
        while *self.peek() == Token::Or {
            self.advance();
            let rhs = self.parse_and_expr()?;
            lhs = Expr::BinOp { op: BinOp::Or, left: Box::new(lhs), right: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_eq_expr()?;
        while *self.peek() == Token::And {
            self.advance();
            let rhs = self.parse_eq_expr()?;
            lhs = Expr::BinOp { op: BinOp::And, left: Box::new(lhs), right: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_eq_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_rel_expr()?;
        loop {
            let op = match self.peek() {
                Token::Eq    => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                _            => break,
            };
            self.advance();
            let rhs = self.parse_rel_expr()?;
            lhs = Expr::BinOp { op, left: Box::new(lhs), right: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_rel_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_add_expr()?;
        loop {
            let op = match self.peek() {
                Token::Lt   => BinOp::Lt,
                Token::LtEq => BinOp::LtEq,
                Token::Gt   => BinOp::Gt,
                Token::GtEq => BinOp::GtEq,
                _           => break,
            };
            self.advance();
            let rhs = self.parse_add_expr()?;
            lhs = Expr::BinOp { op, left: Box::new(lhs), right: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_add_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_mul_expr()?;
        loop {
            let op = match self.peek() {
                Token::Plus  => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _            => break,
            };
            self.advance();
            let rhs = self.parse_mul_expr()?;
            lhs = Expr::BinOp { op, left: Box::new(lhs), right: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_mul_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_unary_expr()?;
        loop {
            let op = match self.peek() {
                Token::Star    => BinOp::Mul,
                Token::Slash   => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _              => break,
            };
            self.advance();
            let rhs = self.parse_unary_expr()?;
            lhs = Expr::BinOp { op, left: Box::new(lhs), right: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Not => {
                self.advance();
                let operand = self.parse_unary_expr()?;
                Ok(Expr::UnaryOp { op: UnaryOp::Not, operand: Box::new(operand) })
            }
            Token::Minus => {
                self.advance();
                let operand = self.parse_unary_expr()?;
                Ok(Expr::UnaryOp { op: UnaryOp::Neg, operand: Box::new(operand) })
            }
            _ => self.parse_postfix_expr(),
        }
    }

    fn parse_postfix_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary_expr()?;
        loop {
            match self.peek() {
                Token::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    if *self.peek() == Token::LParen {
                        self.advance();
                        let args = self.parse_arg_list()?;
                        self.expect(Token::RParen, "')'")?;
                        expr = Expr::Call {
                            callee: Box::new(Expr::FieldAccess { object: Box::new(expr), field }),
                            args,
                        };
                    } else {
                        expr = Expr::FieldAccess { object: Box::new(expr), field };
                    }
                }
                Token::LParen => {
                    self.advance();
                    let args = self.parse_arg_list()?;
                    self.expect(Token::RParen, "')'")?;
                    expr = Expr::Call { callee: Box::new(expr), args };
                }
                Token::LBracket => {
                    self.advance();
                    let mut indices = vec![self.parse_expr()?];
                    while *self.peek() == Token::Comma {
                        self.advance();
                        indices.push(self.parse_expr()?);
                    }
                    self.expect(Token::RBracket, "']'")?;
                    expr = Expr::ArrayAccess { array: Box::new(expr), indices };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        let st = self.current().clone();
        match st.token.clone() {
            Token::IntLiteral(n)    => { self.advance(); Ok(Expr::IntLit(n)) }
            Token::FloatLiteral(f)  => { self.advance(); Ok(Expr::FloatLit(f)) }
            Token::StringLiteral(s) => { self.advance(); Ok(Expr::StringLit(s)) }
            Token::BoolLiteral(b)   => { self.advance(); Ok(Expr::BoolLit(b)) }
            Token::Null             => { self.advance(); Ok(Expr::Null) }
            Token::Ident(name)      => { self.advance(); Ok(Expr::Ident(name)) }
            Token::ClassIdent(name) => { self.advance(); Ok(Expr::Ident(name)) }
            Token::This             => { self.advance(); Ok(Expr::This) }
            Token::Super            => { self.advance(); Ok(Expr::Super) }
            Token::New => {
                self.advance();
                let class_name = self.expect_type_name()?;
                
                let mut type_args = Vec::new();
                if *self.peek() == Token::LAngle {
                    self.advance();
                    type_args.push(self.parse_type_expr()?);
                    while *self.peek() == Token::Comma {
                        self.advance();
                        type_args.push(self.parse_type_expr()?);
                    }
                    self.expect(Token::RAngle, "'>'")?;
                }
                
                if *self.peek() == Token::LBracket {
                    // Handle new Int[10, 10]
                    self.advance();
                    let mut sizes = vec![self.parse_expr()?];
                    while *self.peek() == Token::Comma {
                        self.advance();
                        sizes.push(self.parse_expr()?);
                    }
                    self.expect(Token::RBracket, "']'")?;
                    return Ok(Expr::ArrayAlloc { 
                        element_type: TypeExpr::Named { name: class_name, type_args, optional: false }, 
                        sizes 
                    });
                } else {
                    self.expect(Token::LParen, "'('")?;
                    let args = self.parse_arg_list()?;
                    self.expect(Token::RParen, "')'")?;
                    Ok(Expr::New { class_name, type_args, args })
                }
            }
            Token::Function | Token::Method => {
                self.advance();
                self.expect(Token::LParen, "'('")?;
                let params = if *self.peek() == Token::RParen {
                    Vec::new()
                } else {
                    self.parse_closure_param_list()?
                };
                self.expect(Token::RParen, "')'")?;
                
                let body = if *self.peek() == Token::DoubleArrow {
                    self.advance(); // consume =>
                    let expr = self.parse_expr()?;
                    vec![Stmt::Return(Some(expr))]
                } else {
                    self.parse_block()?
                };
                Ok(Expr::Closure { params, body })
            }
            Token::LParen => {
                let is_arrow = if self.pos + 1 < self.tokens.len() {
                    let next_tok = &self.tokens[self.pos + 1].token;
                    match next_tok {
                        Token::RParen => true, // `()`
                        Token::In => true,     // `(in ...`
                        Token::Ident(_) => {
                            if self.pos + 2 < self.tokens.len() && self.tokens[self.pos + 2].token == Token::Colon {
                                true
                            } else {
                                false
                            }
                        }
                        _ => false,
                    }
                } else {
                    false
                };
                
                if is_arrow {
                    self.advance(); // consume '('
                    let params = if *self.peek() == Token::RParen {
                        Vec::new()
                    } else {
                        self.parse_closure_param_list()?
                    };
                    self.expect(Token::RParen, "')'")?;
                    self.expect(Token::Arrow, "'->'")?;
                    let body = self.parse_block()?;
                    return Ok(Expr::Closure { params, body });
                }

                self.advance();
                let expr = self.parse_expr()?;
                self.expect(Token::RParen, "')'")?;
                Ok(expr)
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "expression",
                found: st.token.clone(),
                line: st.span.line,
                col: st.span.col,
            })
        }
    }



    fn parse_arg_list(&mut self) -> Result<Vec<Arg>, ParseError> {
        if *self.peek() == Token::RParen {
            return Ok(Vec::new());
        }
        let mut args = Vec::new();
        loop {
            let name = if let Token::Ident(n) = self.peek() {
                // Peek lookahead for `name:`
                if self.pos + 1 < self.tokens.len() && self.tokens[self.pos + 1].token == Token::Colon {
                    let name = n.clone();
                    self.advance(); // identifier
                    self.advance(); // colon
                    Some(name)
                } else {
                    None
                }
            } else {
                None
            };
            
            let value = self.parse_expr()?;
            args.push(Arg { name, value });
            
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        Ok(args)
    }

    // ── Type Expressions ──────────────────────────────────────────────────

    /// Accept any token that can name a type: built-in keywords OR class names.
    fn expect_type_name(&mut self) -> Result<String, ParseError> {
        let st = self.current().clone();
        let name = match &st.token {
            Token::TypeInt    => "Int".to_string(),
            Token::TypeFloat  => "Float".to_string(),
            Token::TypeString => "String".to_string(),
            Token::TypeBool   => "Bool".to_string(),
            Token::Method     => "method".to_string(),
            Token::ClassIdent(n) => n.clone(),
            Token::Ident(n)      => n.clone(),
            _ => return Err(ParseError::UnexpectedToken {
                expected: "type name",
                found: st.token.clone(),
                line: st.span.line,
                col: st.span.col,
            }),
        };
        self.advance();
        Ok(name)
    }

    fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        // Function type: (T1, T2) -> T3
        if *self.peek() == Token::LParen {
            self.advance(); // consume '('
            let mut params = Vec::new();
            if *self.peek() != Token::RParen {
                params.push(self.parse_type_expr()?);
                while *self.peek() == Token::Comma {
                    self.advance();
                    params.push(self.parse_type_expr()?);
                }
            }
            self.expect(Token::RParen, "')'")?;
            self.expect(Token::Arrow, "'->'")?;
            let return_type = Box::new(self.parse_type_expr()?);
            let optional = if *self.peek() == Token::Question {
                self.advance();
                true
            } else {
                false
            };
            return Ok(TypeExpr::Function { params, return_type, optional });
        }

        let name = self.expect_type_name()?;
        
        let type_args = if *self.peek() == Token::LAngle {
            self.advance();
            let mut args = vec![self.parse_type_expr()?];
            while *self.peek() == Token::Comma {
                self.advance();
                args.push(self.parse_type_expr()?);
            }
            self.expect(Token::RAngle, "'>'")?;
            args
        } else {
            Vec::new()
        };

        // Check for array dimensions: [,,]
        if *self.peek() == Token::LBracket {
            self.advance();
            let mut dimensions = 1;
            while *self.peek() == Token::Comma {
                self.advance();
                dimensions += 1;
            }
            self.expect(Token::RBracket, "']'")?;
            
            let optional = if *self.peek() == Token::Question {
                self.advance();
                true
            } else {
                false
            };
            
            return Ok(TypeExpr::Array { 
                element_type: Box::new(TypeExpr::Named { name, type_args, optional: false }),
                dimensions,
                optional
            });
        }

        let optional = if *self.peek() == Token::Question {
            self.advance();
            true
        } else {
            false
        };
        Ok(TypeExpr::Named { name, type_args, optional })
    }

    // ── Utilities ─────────────────────────────────────────────────────────

    fn parse_ident_list(&mut self) -> Result<Vec<String>, ParseError> {
        let mut list = vec![self.expect_ident()?];
        while *self.peek() == Token::Comma {
            self.advance();
            list.push(self.expect_ident()?);
        }
        Ok(list)
    }
}

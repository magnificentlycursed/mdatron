//! Recursive-descent parser for expression strings.
//!
//! Converts the expression strings in pattern YAML files into [`Expr`] trees the
//! evaluator already handles. Returns a [`ParseError`] with byte position on failure.
//!
//! Grammar (lowest precedence first):
//! ```text
//! expression  = or_expr
//! or_expr     = and_expr ("or" and_expr)*
//! and_expr    = not_expr ("and" not_expr)*
//! not_expr    = "not" not_expr | in_expr
//! in_expr     = eq_expr (("in" | "not_in") eq_expr)?
//! eq_expr     = postfix (("==" | "!=") postfix)?
//! postfix     = primary ("." identifier)*
//! primary     = "(" expression ")" | string | int | bool | null | array | var_ref
//!             | "every" "(" identifier "in" expression "," expression ")"
//!             | "some"  "(" identifier "in" expression "," expression ")"
//!             | identifier "(" args ")"
//! var_ref     = "$" identifier            ; $self / $file / $project / $<binding>
//! identifier  = [a-zA-Z_][a-zA-Z0-9_]*
//! string      = '"' (escape | char)* '"'
//! escape      = '\"' | '\\' | '\n'
//! int         = '-'? digit+
//! array       = "[" (literal ("," literal)*)? "]"
//! ```
//!
//! Array literals contain only literal values in v0.1.x; expression-typed elements
//! land later when the use case appears.

use std::fmt;

use thiserror::Error;

use super::expr::{Expr, Value, VarRef};

#[derive(Debug, Error, PartialEq)]
pub struct ParseError {
    pub position: usize,
    pub message: String,
}

impl ParseError {
    fn new(position: usize, message: impl Into<String>) -> Self {
        Self {
            position,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at byte {}: {}", self.position, self.message)
    }
}

/// Parse a complete expression string.
///
/// Returns [`ParseError`] if the string does not parse as a complete expression
/// (trailing input after the expression is also an error).
pub fn parse_expression(input: &str) -> Result<Expr, ParseError> {
    let mut p = Parser::new(input);
    let expr = p.parse_or_expr()?;
    p.skip_whitespace();
    if p.pos < p.input.len() {
        return Err(ParseError::new(
            p.pos,
            format!("unexpected trailing input: '{}'", &p.input[p.pos..]),
        ));
    }
    Ok(expr)
}

// ── Parser ─────────────────────────────────────────────────────────────────────

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let b = self.input.as_bytes()[self.pos];
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    /// Consume an exact literal string. Returns true on success.
    fn consume_str(&mut self, s: &str) -> bool {
        if self.input[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    /// Consume a keyword (word followed by non-identifier boundary).
    fn consume_keyword(&mut self, kw: &str) -> bool {
        let end = self.pos + kw.len();
        if end > self.input.len() {
            return false;
        }
        if &self.input[self.pos..end] != kw {
            return false;
        }
        if let Some(c) = self.input[end..].chars().next() {
            if c.is_alphanumeric() || c == '_' {
                return false;
            }
        }
        self.pos = end;
        true
    }

    // ── Precedence chain ───────────────────────────────────────────────────

    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and_expr()?;
        loop {
            self.skip_whitespace();
            let saved = self.pos;
            if self.consume_keyword("or") {
                self.skip_whitespace();
                let right = self.parse_and_expr()?;
                left = Expr::Or(Box::new(left), Box::new(right));
            } else {
                self.pos = saved;
                break;
            }
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_not_expr()?;
        loop {
            self.skip_whitespace();
            let saved = self.pos;
            if self.consume_keyword("and") {
                self.skip_whitespace();
                let right = self.parse_not_expr()?;
                left = Expr::And(Box::new(left), Box::new(right));
            } else {
                self.pos = saved;
                break;
            }
        }
        Ok(left)
    }

    fn parse_not_expr(&mut self) -> Result<Expr, ParseError> {
        self.skip_whitespace();
        if self.consume_keyword("not") {
            self.skip_whitespace();
            let inner = self.parse_not_expr()?;
            Ok(Expr::Not(Box::new(inner)))
        } else {
            self.parse_in_expr()
        }
    }

    fn parse_in_expr(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_eq_expr()?;
        self.skip_whitespace();
        let saved = self.pos;
        if self.consume_keyword("not_in") {
            self.skip_whitespace();
            let right = self.parse_eq_expr()?;
            Ok(Expr::NotIn(Box::new(left), Box::new(right)))
        } else if self.consume_keyword("in") {
            self.skip_whitespace();
            let right = self.parse_eq_expr()?;
            Ok(Expr::In(Box::new(left), Box::new(right)))
        } else {
            self.pos = saved;
            Ok(left)
        }
    }

    fn parse_eq_expr(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_postfix()?;
        self.skip_whitespace();
        if self.consume_str("==") {
            self.skip_whitespace();
            let right = self.parse_postfix()?;
            Ok(Expr::Eq(Box::new(left), Box::new(right)))
        } else if self.consume_str("!=") {
            self.skip_whitespace();
            let right = self.parse_postfix()?;
            Ok(Expr::Ne(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            self.skip_whitespace();
            // Distinguish `.field` (postfix field access) from a `.` that begins something else.
            // Only consume `.` when followed by an identifier start.
            let after_dot = self.pos + 1;
            let dot_is_field = self.peek_char() == Some('.') && after_dot < self.input.len() && {
                let c = self.input[after_dot..].chars().next();
                matches!(c, Some(ch) if ch.is_alphabetic() || ch == '_')
            };
            if !dot_is_field {
                break;
            }
            self.pos += 1; // consume '.'
            let field = self.parse_identifier()?;
            expr = Expr::Field(Box::new(expr), field);
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        self.skip_whitespace();
        let c = self
            .peek_char()
            .ok_or_else(|| ParseError::new(self.pos, "unexpected end of input"))?;
        match c {
            '"' => self.parse_string_literal(),
            '(' => {
                self.pos += 1;
                let expr = self.parse_or_expr()?;
                self.skip_whitespace();
                if !self.consume_str(")") {
                    return Err(ParseError::new(self.pos, "expected ')'"));
                }
                Ok(expr)
            }
            '[' => self.parse_array_literal(),
            '$' => self.parse_var_ref(),
            c if c.is_ascii_digit() || c == '-' => self.parse_int_literal(),
            c if c.is_alphabetic() || c == '_' => self.parse_word(),
            other => Err(ParseError::new(
                self.pos,
                format!("unexpected character '{other}'"),
            )),
        }
    }

    /// Parse an identifier-led primary: keyword literal (true / false / null), `every`,
    /// `some`, or a function call.
    fn parse_word(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        let ident = self.parse_identifier()?;
        match ident.as_str() {
            "true" => Ok(Expr::Lit(Value::Bool(true))),
            "false" => Ok(Expr::Lit(Value::Bool(false))),
            "null" => Ok(Expr::Lit(Value::Null)),
            "every" => self.parse_quantifier_body(QuantifierKind::Every),
            "some" => self.parse_quantifier_body(QuantifierKind::Some),
            // Reserved keywords that cannot appear as a primary:
            "and" | "or" | "not" | "in" | "not_in" => Err(ParseError::new(
                start,
                format!("'{ident}' is a reserved keyword, not a primary expression"),
            )),
            name => {
                // Identifier disambiguation: `(` means function call; otherwise it is a
                // bare binding reference (e.g. a quantifier loop variable like the `d`
                // in `every(d in xs, d in ys)`).
                let saved = self.pos;
                self.skip_whitespace();
                if self.consume_str("(") {
                    let args = self.parse_call_args()?;
                    Ok(Expr::Call(name.to_string(), args))
                } else {
                    self.pos = saved;
                    Ok(Expr::Var(VarRef::Binding(name.to_string())))
                }
            }
        }
    }

    fn parse_quantifier_body(&mut self, kind: QuantifierKind) -> Result<Expr, ParseError> {
        self.skip_whitespace();
        if !self.consume_str("(") {
            return Err(ParseError::new(
                self.pos,
                format!("expected '(' after '{}'", kind.name()),
            ));
        }
        self.skip_whitespace();
        let binding = self.parse_identifier()?;
        self.skip_whitespace();
        if !self.consume_keyword("in") {
            return Err(ParseError::new(
                self.pos,
                format!(
                    "expected 'in' after binding name '{binding}' in {} quantifier",
                    kind.name()
                ),
            ));
        }
        self.skip_whitespace();
        let collection = self.parse_or_expr()?;
        self.skip_whitespace();
        if !self.consume_str(",") {
            return Err(ParseError::new(
                self.pos,
                format!(
                    "expected ',' separating collection from predicate in {}",
                    kind.name()
                ),
            ));
        }
        self.skip_whitespace();
        let predicate = self.parse_or_expr()?;
        self.skip_whitespace();
        if !self.consume_str(")") {
            return Err(ParseError::new(
                self.pos,
                format!("expected ')' closing {} quantifier", kind.name()),
            ));
        }
        Ok(match kind {
            QuantifierKind::Every => {
                Expr::Every(binding, Box::new(collection), Box::new(predicate))
            }
            QuantifierKind::Some => Expr::Some_(binding, Box::new(collection), Box::new(predicate)),
        })
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        self.skip_whitespace();
        if self.consume_str(")") {
            return Ok(args);
        }
        loop {
            let arg = self.parse_or_expr()?;
            args.push(arg);
            self.skip_whitespace();
            if self.consume_str(",") {
                self.skip_whitespace();
                continue;
            }
            if self.consume_str(")") {
                return Ok(args);
            }
            return Err(ParseError::new(
                self.pos,
                "expected ',' or ')' in function call arguments",
            ));
        }
    }

    fn parse_var_ref(&mut self) -> Result<Expr, ParseError> {
        if !self.consume_str("$") {
            return Err(ParseError::new(self.pos, "expected '$'"));
        }
        let ident = self.parse_identifier()?;
        let var = match ident.as_str() {
            "self" => VarRef::SelfVar,
            "file" => VarRef::File,
            "project" => VarRef::Project,
            other => VarRef::Binding(other.to_string()),
        };
        Ok(Expr::Var(var))
    }

    fn parse_string_literal(&mut self) -> Result<Expr, ParseError> {
        if !self.consume_str("\"") {
            return Err(ParseError::new(self.pos, "expected '\"'"));
        }
        let mut value = String::new();
        while let Some(c) = self.peek_char() {
            if c == '"' {
                self.pos += 1;
                return Ok(Expr::Lit(Value::Str(value)));
            }
            if c == '\\' {
                self.pos += 1;
                let escaped = self
                    .peek_char()
                    .ok_or_else(|| ParseError::new(self.pos, "unterminated escape"))?;
                match escaped {
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    other => {
                        return Err(ParseError::new(
                            self.pos,
                            format!("invalid escape sequence '\\{other}'"),
                        ));
                    }
                }
                self.pos += escaped.len_utf8();
            } else {
                value.push(c);
                self.pos += c.len_utf8();
            }
        }
        Err(ParseError::new(self.pos, "unterminated string literal"))
    }

    fn parse_int_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        if self.peek_char() == Some('-') {
            self.pos += 1;
        }
        let digits_start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == digits_start {
            return Err(ParseError::new(start, "expected integer digits"));
        }
        let text = &self.input[start..self.pos];
        let n: i64 = text
            .parse()
            .map_err(|_| ParseError::new(start, format!("invalid integer '{text}'")))?;
        Ok(Expr::Lit(Value::Int(n)))
    }

    fn parse_array_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        if !self.consume_str("[") {
            return Err(ParseError::new(start, "expected '['"));
        }
        let mut elements = Vec::new();
        self.skip_whitespace();
        if self.consume_str("]") {
            return Ok(Expr::Lit(Value::Array(elements)));
        }
        loop {
            self.skip_whitespace();
            let elem_start = self.pos;
            let elem = self.parse_primary()?;
            match elem {
                Expr::Lit(v) => elements.push(v),
                _ => {
                    return Err(ParseError::new(
                        elem_start,
                        "array elements must be literal values in v0.1.x",
                    ));
                }
            }
            self.skip_whitespace();
            if self.consume_str(",") {
                continue;
            }
            if self.consume_str("]") {
                return Ok(Expr::Lit(Value::Array(elements)));
            }
            return Err(ParseError::new(self.pos, "expected ',' or ']' in array"));
        }
    }

    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        if let Some(c) = self.peek_char() {
            if !(c.is_alphabetic() || c == '_') {
                return Err(ParseError::new(start, "expected identifier"));
            }
        } else {
            return Err(ParseError::new(start, "expected identifier"));
        }
        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '_' {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        Ok(self.input[start..self.pos].to_string())
    }
}

#[derive(Copy, Clone)]
enum QuantifierKind {
    Every,
    Some,
}

impl QuantifierKind {
    fn name(self) -> &'static str {
        match self {
            Self::Every => "every",
            Self::Some => "some",
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::expr::{evaluate, EvalContext};
    use super::*;
    use std::collections::BTreeMap;

    fn s(name: &str) -> Value {
        Value::Str(name.to_string())
    }

    fn arr(values: impl IntoIterator<Item = Value>) -> Value {
        Value::Array(values.into_iter().collect())
    }

    fn obj(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    // ── Literals ───────────────────────────────────────────────────────────

    #[test]
    fn parses_string_literal() {
        let expr = parse_expression("\"hello\"").unwrap();
        assert_eq!(expr, Expr::Lit(s("hello")));
    }

    #[test]
    fn parses_string_with_escape() {
        let expr = parse_expression(r#""he said \"hi\"""#).unwrap();
        assert_eq!(expr, Expr::Lit(s("he said \"hi\"")));
    }

    #[test]
    fn parses_int_literal() {
        let expr = parse_expression("42").unwrap();
        assert_eq!(expr, Expr::Lit(Value::Int(42)));
    }

    #[test]
    fn parses_negative_int() {
        let expr = parse_expression("-7").unwrap();
        assert_eq!(expr, Expr::Lit(Value::Int(-7)));
    }

    #[test]
    fn parses_bool_literals() {
        assert_eq!(
            parse_expression("true").unwrap(),
            Expr::Lit(Value::Bool(true))
        );
        assert_eq!(
            parse_expression("false").unwrap(),
            Expr::Lit(Value::Bool(false))
        );
    }

    #[test]
    fn parses_null_literal() {
        assert_eq!(parse_expression("null").unwrap(), Expr::Lit(Value::Null));
    }

    #[test]
    fn parses_array_literal() {
        let expr = parse_expression(r#"["resolved", "deferred"]"#).unwrap();
        assert_eq!(expr, Expr::Lit(arr([s("resolved"), s("deferred")])));
    }

    #[test]
    fn parses_empty_array_literal() {
        let expr = parse_expression("[]").unwrap();
        assert_eq!(expr, Expr::Lit(Value::Array(vec![])));
    }

    // ── Variable refs + field access ───────────────────────────────────────

    #[test]
    fn parses_self_var() {
        let expr = parse_expression("$self").unwrap();
        assert_eq!(expr, Expr::Var(VarRef::SelfVar));
    }

    #[test]
    fn parses_file_var() {
        let expr = parse_expression("$file").unwrap();
        assert_eq!(expr, Expr::Var(VarRef::File));
    }

    #[test]
    fn parses_binding_var() {
        let expr = parse_expression("$expected").unwrap();
        assert_eq!(expr, Expr::Var(VarRef::Binding("expected".into())));
    }

    #[test]
    fn parses_field_access() {
        let expr = parse_expression("$self.phase").unwrap();
        assert_eq!(
            expr,
            Expr::Field(Box::new(Expr::Var(VarRef::SelfVar)), "phase".into())
        );
    }

    #[test]
    fn parses_chained_field_access() {
        let expr = parse_expression("$self.a.b.c").unwrap();
        let inner = Expr::Field(Box::new(Expr::Var(VarRef::SelfVar)), "a".into());
        let middle = Expr::Field(Box::new(inner), "b".into());
        let outer = Expr::Field(Box::new(middle), "c".into());
        assert_eq!(expr, outer);
    }

    // ── Function calls ──────────────────────────────────────────────────────

    #[test]
    fn parses_zero_arg_call() {
        let expr = parse_expression("now()").unwrap();
        assert_eq!(expr, Expr::Call("now".into(), vec![]));
    }

    #[test]
    fn parses_single_arg_call() {
        let expr = parse_expression("count($self.relevant_domains)").unwrap();
        assert_eq!(
            expr,
            Expr::Call(
                "count".into(),
                vec![Expr::Field(
                    Box::new(Expr::Var(VarRef::SelfVar)),
                    "relevant_domains".into()
                )]
            )
        );
    }

    #[test]
    fn parses_multi_arg_call() {
        let expr = parse_expression(r#"union($a, $b)"#).unwrap();
        assert_eq!(
            expr,
            Expr::Call(
                "union".into(),
                vec![
                    Expr::Var(VarRef::Binding("a".into())),
                    Expr::Var(VarRef::Binding("b".into())),
                ]
            )
        );
    }

    // ── Operators + precedence ──────────────────────────────────────────────

    #[test]
    fn parses_equality() {
        let expr = parse_expression(r#"$self.phase == "phase-1a""#).unwrap();
        assert_eq!(
            expr,
            Expr::Eq(
                Box::new(Expr::Field(
                    Box::new(Expr::Var(VarRef::SelfVar)),
                    "phase".into()
                )),
                Box::new(Expr::Lit(s("phase-1a")))
            )
        );
    }

    #[test]
    fn parses_not_prefix() {
        let expr = parse_expression("not true").unwrap();
        assert_eq!(expr, Expr::Not(Box::new(Expr::Lit(Value::Bool(true)))));
    }

    #[test]
    fn or_has_lower_precedence_than_and() {
        // a or b and c parses as a or (b and c)
        let expr = parse_expression("$a or $b and $c").unwrap();
        match expr {
            Expr::Or(left, right) => {
                assert_eq!(*left, Expr::Var(VarRef::Binding("a".into())));
                assert!(matches!(*right, Expr::And(_, _)));
            }
            other => panic!("expected Or at top level, got {other:?}"),
        }
    }

    #[test]
    fn parens_override_precedence() {
        // (a or b) and c parses as Or-wrapped left of And
        let expr = parse_expression("($a or $b) and $c").unwrap();
        match expr {
            Expr::And(left, right) => {
                assert!(matches!(*left, Expr::Or(_, _)));
                assert_eq!(*right, Expr::Var(VarRef::Binding("c".into())));
            }
            other => panic!("expected And at top level, got {other:?}"),
        }
    }

    #[test]
    fn parses_in_operator() {
        let expr = parse_expression(r#""resolved" in ["resolved", "deferred"]"#).unwrap();
        assert!(matches!(expr, Expr::In(_, _)));
    }

    #[test]
    fn parses_not_in_operator() {
        let expr = parse_expression(r#""hallucinated" not_in $allowed"#).unwrap();
        assert!(matches!(expr, Expr::NotIn(_, _)));
    }

    #[test]
    fn parses_eq_or_eq() {
        // Looks like: $self.validator == $expected.validator_pair or $self.validator == "sanity-check"
        let expr = parse_expression(
            r#"$self.validator == $expected.validator_pair or $self.validator == "sanity-check""#,
        )
        .unwrap();
        match expr {
            Expr::Or(left, right) => {
                assert!(matches!(*left, Expr::Eq(_, _)));
                assert!(matches!(*right, Expr::Eq(_, _)));
            }
            other => panic!("expected Or at top, got {other:?}"),
        }
    }

    // ── Quantifiers ─────────────────────────────────────────────────────────

    #[test]
    fn parses_every_quantifier() {
        let expr = parse_expression("every(d in $required, d in $self.relevant_domains)").unwrap();
        match expr {
            Expr::Every(binding, collection, predicate) => {
                assert_eq!(binding, "d");
                assert_eq!(*collection, Expr::Var(VarRef::Binding("required".into())));
                assert!(matches!(*predicate, Expr::In(_, _)));
            }
            other => panic!("expected Every, got {other:?}"),
        }
    }

    #[test]
    fn parses_some_quantifier() {
        let expr = parse_expression(r#"some(x in $self.tags, x == "important")"#).unwrap();
        assert!(matches!(expr, Expr::Some_(name, _, _) if name == "x"));
    }

    // ── Errors ──────────────────────────────────────────────────────────────

    #[test]
    fn error_on_empty_input() {
        let err = parse_expression("").unwrap_err();
        assert!(err.message.contains("unexpected end of input"));
    }

    #[test]
    fn error_on_unterminated_string() {
        let err = parse_expression(r#""no closing"#).unwrap_err();
        assert!(err.message.contains("unterminated string"));
    }

    #[test]
    fn error_on_unmatched_paren() {
        let err = parse_expression("(true").unwrap_err();
        assert!(err.message.contains("expected ')'"));
    }

    #[test]
    fn error_on_trailing_input() {
        let err = parse_expression("true extra").unwrap_err();
        assert!(err.message.contains("trailing input"));
    }

    #[test]
    fn error_on_reserved_keyword_as_primary() {
        let err = parse_expression("and").unwrap_err();
        assert!(err.message.contains("reserved keyword"));
    }

    #[test]
    fn error_on_missing_arg_separator() {
        let err = parse_expression("count($a $b)").unwrap_err();
        assert!(err.message.contains("expected ',' or ')'"));
    }

    #[test]
    fn error_on_every_missing_in_keyword() {
        let err = parse_expression("every(d, $self.x)").unwrap_err();
        assert!(err.message.contains("expected 'in'"));
    }

    // ── End-to-end: parse then evaluate ────────────────────────────────────

    #[test]
    fn classification_universe_rule_parses_and_evaluates() {
        let finding = obj([("classification", s("resolved"))]);
        let domain = obj([(
            "classification_universe",
            arr([s("resolved"), s("deferred"), s("dismissed")]),
        )]);
        let file_v = Value::Null;
        let project_v = Value::Null;
        let mut bindings = BTreeMap::new();
        bindings.insert("expected".to_string(), domain);
        let context = EvalContext {
            self_value: &finding,
            file_value: &file_v,
            project_value: &project_v,
            bindings,
            indices: None,
        };

        let expr =
            parse_expression("$self.classification in $expected.classification_universe").unwrap();
        let result = evaluate(&expr, &context).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn phase_composition_rule_parses_and_evaluates() {
        // every(d in $expected.required, d in $self.relevant_domains)
        let primer = obj([(
            "relevant_domains",
            arr([s("software-engineer"), s("quality-engineer")]),
        )]);
        let expected = obj([(
            "required",
            arr([s("software-engineer"), s("quality-engineer")]),
        )]);
        let file_v = Value::Null;
        let project_v = Value::Null;
        let mut bindings = BTreeMap::new();
        bindings.insert("expected".to_string(), expected);
        let context = EvalContext {
            self_value: &primer,
            file_value: &file_v,
            project_value: &project_v,
            bindings,
            indices: None,
        };

        let expr = parse_expression("every(d in $expected.required, d in $self.relevant_domains)")
            .unwrap();
        let result = evaluate(&expr, &context).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn phase_composition_rule_detects_missing_required_domain() {
        let primer = obj([(
            "relevant_domains",
            arr([s("software-engineer")]), // missing quality-engineer
        )]);
        let expected = obj([(
            "required",
            arr([s("software-engineer"), s("quality-engineer")]),
        )]);
        let file_v = Value::Null;
        let project_v = Value::Null;
        let mut bindings = BTreeMap::new();
        bindings.insert("expected".to_string(), expected);
        let context = EvalContext {
            self_value: &primer,
            file_value: &file_v,
            project_value: &project_v,
            bindings,
            indices: None,
        };

        let expr = parse_expression("every(d in $expected.required, d in $self.relevant_domains)")
            .unwrap();
        let result = evaluate(&expr, &context).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn validator_pair_match_rule_parses_and_evaluates() {
        // $self.validator == $expected.validator_pair or $self.validator == "sanity-check"
        let finding = obj([("validator", s("sanity-check"))]);
        let domain = obj([("validator_pair", s("solution-architect"))]);
        let file_v = Value::Null;
        let project_v = Value::Null;
        let mut bindings = BTreeMap::new();
        bindings.insert("expected".to_string(), domain);
        let context = EvalContext {
            self_value: &finding,
            file_value: &file_v,
            project_value: &project_v,
            bindings,
            indices: None,
        };

        let expr = parse_expression(
            r#"$self.validator == $expected.validator_pair or $self.validator == "sanity-check""#,
        )
        .unwrap();
        let result = evaluate(&expr, &context).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    // ── Whitespace tolerance ────────────────────────────────────────────────

    #[test]
    fn tolerates_extra_whitespace() {
        let expr1 = parse_expression("  $self.phase  ==  \"phase-1a\"  ").unwrap();
        let expr2 = parse_expression(r#"$self.phase=="phase-1a""#).unwrap();
        assert_eq!(expr1, expr2);
    }
}

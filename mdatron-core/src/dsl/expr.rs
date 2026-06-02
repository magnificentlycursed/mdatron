//! Expression AST + evaluator + minimal standard library.
//!
//! v0.1.x scope: equality (`==`, `!=`), boolean logic (`and`, `or`, `not`), set membership
//! (`in`, `not_in`), and the quantifier forms `every(x in xs, pred)` / `some(x in xs, pred)`.
//! Standard library: `count`, `len`, `union`, `intersect`, `difference`, `join`, `defined`.
//!
//! Deferred for subsequent iterations: arithmetic (`+ - * / %`), ordered comparisons
//! (`< <= > >=`), array indexing (`[i]`), the `let:` rebinding mechanism (currently
//! handled by the caller before calling `evaluate`), string functions (`match`,
//! `extract`, `slug`, etc.), markdown AST helpers (`headings`, `tables`, ...), the
//! `key()` cross-file lookup (depends on a separate cross-file index module).
//!
//! Callers construct [`Expr`] trees directly; a string parser ("0.1.0 -> Expr") will land
//! in a follow-up iteration once the evaluator surface is stable.

use std::collections::BTreeMap;

use thiserror::Error;

use super::index::IndexRegistry;

// ── Value ──────────────────────────────────────────────────────────────────────

/// A runtime value produced by evaluating an expression. Maps cleanly to/from
/// `serde_json::Value` and `serde_yaml::Value`; we use our own type so the evaluator
/// can return owned values without leaking the serde representation through its API.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// True for `Bool(true)`. Other truthy-coercion shortcuts are not provided; rules
    /// must produce explicit booleans for their `assert` clauses.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Self::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Str(_) => "str",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }
}

// ── Expression AST ─────────────────────────────────────────────────────────────

/// AST for a single expression. Constructed by the (future) parser or by tests.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Literal value.
    Lit(Value),
    /// Variable reference (`$self`, `$file`, `$project`, or a let-binding name).
    Var(VarRef),
    /// `expr.field` — object property access.
    Field(Box<Expr>, String),
    /// `a == b`
    Eq(Box<Expr>, Box<Expr>),
    /// `a != b`
    Ne(Box<Expr>, Box<Expr>),
    /// `a and b` (short-circuit on false).
    And(Box<Expr>, Box<Expr>),
    /// `a or b` (short-circuit on true).
    Or(Box<Expr>, Box<Expr>),
    /// `not a`
    Not(Box<Expr>),
    /// `a in b` — element-of for arrays; substring-of for strings; key-of for objects.
    In(Box<Expr>, Box<Expr>),
    /// `a not_in b` — negation of `in`.
    NotIn(Box<Expr>, Box<Expr>),
    /// Standard library function call by name.
    Call(String, Vec<Expr>),
    /// `every(<binding> in <collection>, <predicate>)` — universal quantifier.
    Every(String, Box<Expr>, Box<Expr>),
    /// `some(<binding> in <collection>, <predicate>)` — existential quantifier.
    Some_(String, Box<Expr>, Box<Expr>),
}

/// What a `Var(...)` references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarRef {
    /// `$self` — the current artifact's parsed frontmatter (Value::Object).
    SelfVar,
    /// `$file` — file metadata (path, line, etc.).
    File,
    /// `$project` — project metadata.
    Project,
    /// A `let:` binding by name, or a quantifier binding (`every` / `some`).
    Binding(String),
}

// ── Evaluation context + errors ────────────────────────────────────────────────

/// Read-only context an expression evaluates against.
#[derive(Debug, Clone)]
pub struct EvalContext<'a> {
    pub self_value: &'a Value,
    pub file_value: &'a Value,
    pub project_value: &'a Value,
    /// `let:` bindings + quantifier bindings.
    pub bindings: BTreeMap<String, Value>,
    /// Cross-file indices used by `key()` lookups. `None` when no indices are wired
    /// in (existing tests + simple rules that don't reach across files).
    pub indices: Option<&'a IndexRegistry>,
}

impl<'a> EvalContext<'a> {
    pub fn new(
        self_value: &'a Value,
        file_value: &'a Value,
        project_value: &'a Value,
    ) -> Self {
        Self {
            self_value,
            file_value,
            project_value,
            bindings: BTreeMap::new(),
            indices: None,
        }
    }

    /// Wire a cross-file index registry into the context. Required for `key()` lookups.
    pub fn with_indices(mut self, indices: &'a IndexRegistry) -> Self {
        self.indices = Some(indices);
        self
    }

    /// Returns a clone of this context with `(name, value)` added/overridden in bindings.
    /// Used for quantifier evaluation.
    fn with_binding(&self, name: String, value: Value) -> Self {
        let mut bindings = self.bindings.clone();
        bindings.insert(name, value);
        Self {
            self_value: self.self_value,
            file_value: self.file_value,
            project_value: self.project_value,
            bindings,
            indices: self.indices,
        }
    }
}

#[derive(Debug, Clone, Error, PartialEq)]
pub enum EvalError {
    #[error("undefined binding: {0}")]
    UndefinedBinding(String),

    #[error("field '{field}' not found on {on}")]
    FieldNotFound { field: String, on: &'static str },

    #[error("type mismatch: expected {expected}, got {got}")]
    TypeMismatch {
        expected: &'static str,
        got: &'static str,
    },

    #[error("unknown function: {0}")]
    UnknownFunction(String),

    #[error("arity mismatch: function '{name}' expected {expected} args, got {got}")]
    ArityMismatch {
        name: String,
        expected: usize,
        got: usize,
    },

    #[error("key() called but no IndexRegistry is wired into the EvalContext")]
    NoIndexRegistry,
}

// ── Evaluator ──────────────────────────────────────────────────────────────────

/// Evaluate an expression against the context.
pub fn evaluate(expr: &Expr, ctx: &EvalContext) -> Result<Value, EvalError> {
    match expr {
        Expr::Lit(v) => Ok(v.clone()),

        Expr::Var(VarRef::SelfVar) => Ok(ctx.self_value.clone()),
        Expr::Var(VarRef::File) => Ok(ctx.file_value.clone()),
        Expr::Var(VarRef::Project) => Ok(ctx.project_value.clone()),
        Expr::Var(VarRef::Binding(name)) => ctx
            .bindings
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UndefinedBinding(name.clone())),

        Expr::Field(inner, name) => {
            let v = evaluate(inner, ctx)?;
            match v {
                Value::Object(o) => o.get(name).cloned().ok_or_else(|| {
                    EvalError::FieldNotFound {
                        field: name.clone(),
                        on: "object",
                    }
                }),
                Value::Null => Ok(Value::Null),
                other => Err(EvalError::TypeMismatch {
                    expected: "object",
                    got: type_name_str(&other),
                }),
            }
        }

        Expr::Eq(a, b) => Ok(Value::Bool(evaluate(a, ctx)? == evaluate(b, ctx)?)),
        Expr::Ne(a, b) => Ok(Value::Bool(evaluate(a, ctx)? != evaluate(b, ctx)?)),

        Expr::And(a, b) => {
            let av = expect_bool(evaluate(a, ctx)?)?;
            if !av {
                return Ok(Value::Bool(false));
            }
            let bv = expect_bool(evaluate(b, ctx)?)?;
            Ok(Value::Bool(bv))
        }
        Expr::Or(a, b) => {
            let av = expect_bool(evaluate(a, ctx)?)?;
            if av {
                return Ok(Value::Bool(true));
            }
            let bv = expect_bool(evaluate(b, ctx)?)?;
            Ok(Value::Bool(bv))
        }
        Expr::Not(a) => {
            let av = expect_bool(evaluate(a, ctx)?)?;
            Ok(Value::Bool(!av))
        }

        Expr::In(needle, haystack) => {
            let n = evaluate(needle, ctx)?;
            let h = evaluate(haystack, ctx)?;
            Ok(Value::Bool(value_in(&n, &h)?))
        }
        Expr::NotIn(needle, haystack) => {
            let n = evaluate(needle, ctx)?;
            let h = evaluate(haystack, ctx)?;
            Ok(Value::Bool(!value_in(&n, &h)?))
        }

        Expr::Call(name, args) => call_function(name, args, ctx),

        Expr::Every(binding_name, collection_expr, predicate_expr) => {
            let collection = expect_array(evaluate(collection_expr, ctx)?)?;
            for item in collection {
                let child_ctx = ctx.with_binding(binding_name.clone(), item);
                let result = expect_bool(evaluate(predicate_expr, &child_ctx)?)?;
                if !result {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }
        Expr::Some_(binding_name, collection_expr, predicate_expr) => {
            let collection = expect_array(evaluate(collection_expr, ctx)?)?;
            for item in collection {
                let child_ctx = ctx.with_binding(binding_name.clone(), item);
                let result = expect_bool(evaluate(predicate_expr, &child_ctx)?)?;
                if result {
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(false))
        }
    }
}

// ── Standard library dispatch ──────────────────────────────────────────────────

fn call_function(name: &str, args: &[Expr], ctx: &EvalContext) -> Result<Value, EvalError> {
    match name {
        "count" => {
            arity(name, args, 1)?;
            let v = evaluate(&args[0], ctx)?;
            let arr = expect_array(v)?;
            Ok(Value::Int(arr.len() as i64))
        }
        "len" => {
            arity(name, args, 1)?;
            let v = evaluate(&args[0], ctx)?;
            match v {
                Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
                Value::Array(a) => Ok(Value::Int(a.len() as i64)),
                other => Err(EvalError::TypeMismatch {
                    expected: "string or array",
                    got: type_name_str(&other),
                }),
            }
        }
        "defined" => {
            arity(name, args, 1)?;
            let v = evaluate(&args[0], ctx)?;
            let is_defined = match &v {
                Value::Null => false,
                Value::Str(s) => !s.is_empty(),
                _ => true,
            };
            Ok(Value::Bool(is_defined))
        }
        "union" => {
            arity(name, args, 2)?;
            let a = expect_array(evaluate(&args[0], ctx)?)?;
            let b = expect_array(evaluate(&args[1], ctx)?)?;
            let mut out = a.clone();
            for v in b {
                if !out.contains(&v) {
                    out.push(v);
                }
            }
            Ok(Value::Array(out))
        }
        "intersect" => {
            arity(name, args, 2)?;
            let a = expect_array(evaluate(&args[0], ctx)?)?;
            let b = expect_array(evaluate(&args[1], ctx)?)?;
            let out: Vec<Value> = a.into_iter().filter(|v| b.contains(v)).collect();
            Ok(Value::Array(out))
        }
        "difference" => {
            arity(name, args, 2)?;
            let a = expect_array(evaluate(&args[0], ctx)?)?;
            let b = expect_array(evaluate(&args[1], ctx)?)?;
            let out: Vec<Value> = a.into_iter().filter(|v| !b.contains(v)).collect();
            Ok(Value::Array(out))
        }
        "concat" => {
            arity(name, args, 2)?;
            let a = expect_string(evaluate(&args[0], ctx)?)?;
            let b = expect_string(evaluate(&args[1], ctx)?)?;
            Ok(Value::Str(format!("{a}{b}")))
        }
        "join" => {
            arity(name, args, 2)?;
            let a = expect_array(evaluate(&args[0], ctx)?)?;
            let sep = expect_string(evaluate(&args[1], ctx)?)?;
            let parts: Vec<String> = a
                .into_iter()
                .map(|v| match v {
                    Value::Str(s) => s,
                    other => format!("{other:?}"),
                })
                .collect();
            Ok(Value::Str(parts.join(&sep)))
        }
        "key" => {
            arity(name, args, 2)?;
            let registry = ctx.indices.ok_or(EvalError::NoIndexRegistry)?;
            let index_name = expect_string(evaluate(&args[0], ctx)?)?;
            let key_value = evaluate(&args[1], ctx)?;
            let key_str = match key_value {
                Value::Str(s) => s,
                other => {
                    return Err(EvalError::TypeMismatch {
                        expected: "string (index key)",
                        got: type_name_str(&other),
                    });
                }
            };
            // Lookup miss returns Null (per cross-file index semantics);
            // chained field access on Null propagates Null per existing convention.
            Ok(registry
                .lookup(&index_name, &key_str)
                .cloned()
                .unwrap_or(Value::Null))
        }
        other => Err(EvalError::UnknownFunction(other.to_string())),
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn arity(name: &str, args: &[Expr], expected: usize) -> Result<(), EvalError> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(EvalError::ArityMismatch {
            name: name.to_string(),
            expected,
            got: args.len(),
        })
    }
}

fn expect_bool(v: Value) -> Result<bool, EvalError> {
    v.as_bool().ok_or_else(|| EvalError::TypeMismatch {
        expected: "bool",
        got: type_name_str_owned(&v),
    })
}

fn expect_array(v: Value) -> Result<Vec<Value>, EvalError> {
    if let Value::Array(a) = v {
        Ok(a)
    } else {
        Err(EvalError::TypeMismatch {
            expected: "array",
            got: type_name_str_owned(&v),
        })
    }
}

fn expect_string(v: Value) -> Result<String, EvalError> {
    if let Value::Str(s) = v {
        Ok(s)
    } else {
        Err(EvalError::TypeMismatch {
            expected: "string",
            got: type_name_str_owned(&v),
        })
    }
}

/// `in` semantics: needle in haystack.
/// - Array haystack: element equality.
/// - String haystack with string needle: substring containment.
/// - Object haystack with string needle: key presence.
/// - Null haystack: always false (lookup-miss-propagation).
fn value_in(needle: &Value, haystack: &Value) -> Result<bool, EvalError> {
    match haystack {
        Value::Array(arr) => Ok(arr.contains(needle)),
        Value::Str(s) => match needle {
            Value::Str(n) => Ok(s.contains(n.as_str())),
            other => Err(EvalError::TypeMismatch {
                expected: "string (string haystack requires string needle)",
                got: type_name_str(other),
            }),
        },
        Value::Object(o) => match needle {
            Value::Str(n) => Ok(o.contains_key(n)),
            other => Err(EvalError::TypeMismatch {
                expected: "string (object haystack requires string key)",
                got: type_name_str(other),
            }),
        },
        Value::Null => Ok(false),
        other => Err(EvalError::TypeMismatch {
            expected: "array, string, or object",
            got: type_name_str(other),
        }),
    }
}

fn type_name_str(v: &Value) -> &'static str {
    v.type_name()
}

fn type_name_str_owned(v: &Value) -> &'static str {
    v.type_name()
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn null_ctx() -> (Value, Value, Value) {
        (Value::Null, Value::Null, Value::Null)
    }

    fn ctx<'a>(values: &'a (Value, Value, Value)) -> EvalContext<'a> {
        EvalContext::new(&values.0, &values.1, &values.2)
    }

    fn s(name: &str) -> Value {
        Value::Str(name.to_string())
    }

    fn arr(values: impl IntoIterator<Item = Value>) -> Value {
        Value::Array(values.into_iter().collect())
    }

    fn obj(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        Value::Object(
            pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    // ── Literals + variable refs ────────────────────────────────────────────

    #[test]
    fn evaluates_string_literal() {
        let cv = null_ctx();
        let result = evaluate(&Expr::Lit(s("hello")), &ctx(&cv)).unwrap();
        assert_eq!(result, s("hello"));
    }

    #[test]
    fn self_var_returns_self_value() {
        let self_v = s("artifact-X");
        let file_v = Value::Null;
        let project_v = Value::Null;
        let context = EvalContext::new(&self_v, &file_v, &project_v);
        let result = evaluate(&Expr::Var(VarRef::SelfVar), &context).unwrap();
        assert_eq!(result, s("artifact-X"));
    }

    #[test]
    fn binding_lookup_returns_bound_value() {
        let self_v = Value::Null;
        let file_v = Value::Null;
        let project_v = Value::Null;
        let mut bindings = BTreeMap::new();
        bindings.insert("x".to_string(), Value::Int(42));
        let context = EvalContext {
            self_value: &self_v,
            file_value: &file_v,
            project_value: &project_v,
            bindings,
            indices: None,
        };
        let result =
            evaluate(&Expr::Var(VarRef::Binding("x".to_string())), &context).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn binding_lookup_for_undefined_name_errors() {
        let cv = null_ctx();
        let result = evaluate(&Expr::Var(VarRef::Binding("missing".into())), &ctx(&cv));
        assert!(matches!(result, Err(EvalError::UndefinedBinding(_))));
    }

    // ── Field access ────────────────────────────────────────────────────────

    #[test]
    fn field_access_returns_object_field() {
        let self_v = obj([("phase", s("phase-2a")), ("version", s("0.1.0"))]);
        let file_v = Value::Null;
        let project_v = Value::Null;
        let context = EvalContext::new(&self_v, &file_v, &project_v);
        let result = evaluate(
            &Expr::Field(Box::new(Expr::Var(VarRef::SelfVar)), "phase".into()),
            &context,
        )
        .unwrap();
        assert_eq!(result, s("phase-2a"));
    }

    #[test]
    fn field_access_on_null_returns_null() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Field(Box::new(Expr::Lit(Value::Null)), "anything".into()),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn field_access_on_non_object_errors() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Field(Box::new(Expr::Lit(s("not an object"))), "field".into()),
            &ctx(&cv),
        );
        assert!(matches!(result, Err(EvalError::TypeMismatch { .. })));
    }

    #[test]
    fn missing_field_errors() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Field(
                Box::new(Expr::Lit(obj([("x", s("y"))]))),
                "missing".into(),
            ),
            &ctx(&cv),
        );
        assert!(matches!(result, Err(EvalError::FieldNotFound { .. })));
    }

    // ── Equality ────────────────────────────────────────────────────────────

    #[test]
    fn eq_returns_true_for_equal_strings() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Eq(Box::new(Expr::Lit(s("a"))), Box::new(Expr::Lit(s("a")))),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn ne_returns_true_for_unequal_values() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Ne(Box::new(Expr::Lit(s("a"))), Box::new(Expr::Lit(s("b")))),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    // ── Boolean logic + short-circuit ───────────────────────────────────────

    #[test]
    fn and_short_circuits_on_false() {
        let cv = null_ctx();
        // The right operand would error (string is not bool); short-circuit prevents.
        let result = evaluate(
            &Expr::And(
                Box::new(Expr::Lit(Value::Bool(false))),
                Box::new(Expr::Lit(s("not-a-bool"))),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn or_short_circuits_on_true() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Or(
                Box::new(Expr::Lit(Value::Bool(true))),
                Box::new(Expr::Lit(s("not-a-bool"))),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn not_negates_boolean() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Not(Box::new(Expr::Lit(Value::Bool(true)))),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    // ── `in` / `not_in` ─────────────────────────────────────────────────────

    #[test]
    fn in_returns_true_for_array_membership() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::In(
                Box::new(Expr::Lit(s("resolved"))),
                Box::new(Expr::Lit(arr([
                    s("resolved"),
                    s("deferred"),
                    s("dismissed"),
                ]))),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn in_returns_false_for_array_non_membership() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::In(
                Box::new(Expr::Lit(s("hallucinated"))),
                Box::new(Expr::Lit(arr([s("resolved"), s("deferred")]))),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn in_with_null_haystack_returns_false() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::In(Box::new(Expr::Lit(s("x"))), Box::new(Expr::Lit(Value::Null))),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn not_in_negates_in() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::NotIn(
                Box::new(Expr::Lit(s("x"))),
                Box::new(Expr::Lit(arr([s("y")]))),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    // ── Standard library ────────────────────────────────────────────────────

    #[test]
    fn count_returns_array_length() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call(
                "count".into(),
                vec![Expr::Lit(arr([s("a"), s("b"), s("c")]))],
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Int(3));
    }

    #[test]
    fn len_works_on_strings_and_arrays() {
        let cv = null_ctx();
        let r1 = evaluate(
            &Expr::Call("len".into(), vec![Expr::Lit(s("hello"))]),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(r1, Value::Int(5));
        let r2 = evaluate(
            &Expr::Call("len".into(), vec![Expr::Lit(arr([s("a")]))]),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(r2, Value::Int(1));
    }

    #[test]
    fn defined_returns_false_for_null_and_empty_string() {
        let cv = null_ctx();
        assert_eq!(
            evaluate(
                &Expr::Call("defined".into(), vec![Expr::Lit(Value::Null)]),
                &ctx(&cv)
            )
            .unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            evaluate(
                &Expr::Call("defined".into(), vec![Expr::Lit(s(""))]),
                &ctx(&cv)
            )
            .unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn difference_yields_left_minus_right() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call(
                "difference".into(),
                vec![
                    Expr::Lit(arr([s("a"), s("b"), s("c")])),
                    Expr::Lit(arr([s("b")])),
                ],
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, arr([s("a"), s("c")]));
    }

    #[test]
    fn union_dedupes() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call(
                "union".into(),
                vec![
                    Expr::Lit(arr([s("a"), s("b")])),
                    Expr::Lit(arr([s("b"), s("c")])),
                ],
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, arr([s("a"), s("b"), s("c")]));
    }

    #[test]
    fn intersect_finds_common_elements() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call(
                "intersect".into(),
                vec![
                    Expr::Lit(arr([s("a"), s("b")])),
                    Expr::Lit(arr([s("b"), s("c")])),
                ],
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, arr([s("b")]));
    }

    #[test]
    fn concat_joins_two_strings() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call(
                "concat".into(),
                vec![Expr::Lit(s("vsdd-")), Expr::Lit(s("phase-2a"))],
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, s("vsdd-phase-2a"));
    }

    #[test]
    fn concat_rejects_non_string_argument() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call(
                "concat".into(),
                vec![Expr::Lit(s("vsdd-")), Expr::Lit(Value::Int(42))],
            ),
            &ctx(&cv),
        );
        assert!(matches!(result, Err(EvalError::TypeMismatch { .. })));
    }

    #[test]
    fn join_concatenates_with_separator() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call(
                "join".into(),
                vec![Expr::Lit(arr([s("a"), s("b"), s("c")])), Expr::Lit(s(", "))],
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, s("a, b, c"));
    }

    #[test]
    fn unknown_function_errors() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call("nonexistent".into(), vec![]),
            &ctx(&cv),
        );
        assert!(matches!(result, Err(EvalError::UnknownFunction(_))));
    }

    #[test]
    fn arity_mismatch_errors() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call("count".into(), vec![Expr::Lit(arr([])), Expr::Lit(arr([]))]),
            &ctx(&cv),
        );
        assert!(matches!(result, Err(EvalError::ArityMismatch { .. })));
    }

    // ── Quantifiers ─────────────────────────────────────────────────────────

    #[test]
    fn every_returns_true_when_predicate_holds_for_all() {
        let cv = null_ctx();
        // every(d in ["a", "b", "c"], d in ["a", "b", "c", "d"])
        let result = evaluate(
            &Expr::Every(
                "d".into(),
                Box::new(Expr::Lit(arr([s("a"), s("b"), s("c")]))),
                Box::new(Expr::In(
                    Box::new(Expr::Var(VarRef::Binding("d".into()))),
                    Box::new(Expr::Lit(arr([s("a"), s("b"), s("c"), s("d")]))),
                )),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn every_returns_false_when_predicate_fails_for_one() {
        let cv = null_ctx();
        // every(d in ["a", "b", "MISSING"], d in ["a", "b"])
        let result = evaluate(
            &Expr::Every(
                "d".into(),
                Box::new(Expr::Lit(arr([s("a"), s("b"), s("MISSING")]))),
                Box::new(Expr::In(
                    Box::new(Expr::Var(VarRef::Binding("d".into()))),
                    Box::new(Expr::Lit(arr([s("a"), s("b")]))),
                )),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn every_returns_true_for_empty_collection() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Every(
                "d".into(),
                Box::new(Expr::Lit(arr([]))),
                Box::new(Expr::Lit(Value::Bool(false))),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true), "every over empty should be true");
    }

    #[test]
    fn some_returns_true_when_predicate_holds_for_at_least_one() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Some_(
                "x".into(),
                Box::new(Expr::Lit(arr([s("a"), s("b"), s("c")]))),
                Box::new(Expr::Eq(
                    Box::new(Expr::Var(VarRef::Binding("x".into()))),
                    Box::new(Expr::Lit(s("b"))),
                )),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn some_returns_false_for_empty_collection() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Some_(
                "x".into(),
                Box::new(Expr::Lit(arr([]))),
                Box::new(Expr::Lit(Value::Bool(true))),
            ),
            &ctx(&cv),
        )
        .unwrap();
        assert_eq!(
            result,
            Value::Bool(false),
            "some over empty should be false"
        );
    }

    // ── End-to-end: classification-universe rule shape ──────────────────────

    // ── key() function ──────────────────────────────────────────────────────

    #[test]
    fn key_without_registry_errors() {
        let cv = null_ctx();
        let result = evaluate(
            &Expr::Call(
                "key".into(),
                vec![Expr::Lit(s("any-index")), Expr::Lit(s("any-key"))],
            ),
            &ctx(&cv),
        );
        assert!(matches!(result, Err(EvalError::NoIndexRegistry)));
    }

    #[test]
    fn key_returns_null_for_unknown_index() {
        use super::super::index::IndexRegistry;
        let registry = IndexRegistry::new();
        let cv = null_ctx();
        let context = EvalContext::new(&cv.0, &cv.1, &cv.2).with_indices(&registry);
        let result = evaluate(
            &Expr::Call(
                "key".into(),
                vec![Expr::Lit(s("not-loaded")), Expr::Lit(s("anything"))],
            ),
            &context,
        )
        .unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn key_returns_indexed_value_when_present() {
        use super::super::index::{Index, IndexRegistry};
        let mut entries = BTreeMap::new();
        entries.insert("phase-2a".to_string(), obj([("required", arr([s("se"), s("qe")]))]));
        let idx = Index {
            name: "matrix".to_string(),
            entries,
        };
        let mut registry = IndexRegistry::new();
        registry.insert(idx);

        let cv = null_ctx();
        let context = EvalContext::new(&cv.0, &cv.1, &cv.2).with_indices(&registry);

        // key("matrix", "phase-2a").required
        let expr = Expr::Field(
            Box::new(Expr::Call(
                "key".into(),
                vec![Expr::Lit(s("matrix")), Expr::Lit(s("phase-2a"))],
            )),
            "required".into(),
        );
        let result = evaluate(&expr, &context).unwrap();
        assert_eq!(result, arr([s("se"), s("qe")]));
    }

    #[test]
    fn phase_composition_rule_with_key_lookup_evaluates_end_to_end() {
        use super::super::index::{Index, IndexRegistry};
        let mut entries = BTreeMap::new();
        entries.insert(
            "phase-2a".to_string(),
            obj([("required", arr([s("se"), s("qe")]))]),
        );
        let idx = Index {
            name: "composition-matrix".to_string(),
            entries,
        };
        let mut registry = IndexRegistry::new();
        registry.insert(idx);

        let primer = obj([
            ("phase", s("phase-2a")),
            ("relevant_domains", arr([s("se"), s("qe"), s("sa")])),
        ]);
        let file_v = Value::Null;
        let project_v = Value::Null;

        // Simulate the let-binding evaluation: $expected = key("composition-matrix", $self.phase)
        let key_expr = Expr::Call(
            "key".into(),
            vec![
                Expr::Lit(s("composition-matrix")),
                Expr::Field(Box::new(Expr::Var(VarRef::SelfVar)), "phase".into()),
            ],
        );
        let mut precompute_ctx =
            EvalContext::new(&primer, &file_v, &project_v).with_indices(&registry);
        let expected = evaluate(&key_expr, &precompute_ctx).unwrap();
        precompute_ctx.bindings.insert("expected".to_string(), expected);

        // Assert: every(d in $expected.required, d in $self.relevant_domains)
        let assert_expr = Expr::Every(
            "d".into(),
            Box::new(Expr::Field(
                Box::new(Expr::Var(VarRef::Binding("expected".into()))),
                "required".into(),
            )),
            Box::new(Expr::In(
                Box::new(Expr::Var(VarRef::Binding("d".into()))),
                Box::new(Expr::Field(
                    Box::new(Expr::Var(VarRef::SelfVar)),
                    "relevant_domains".into(),
                )),
            )),
        );
        let result = evaluate(&assert_expr, &precompute_ctx).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn classification_universe_rule_passes_for_valid_finding() {
        // finding $self.classification = "resolved"
        // domain has classification_universe = ["resolved", "deferred", "dismissed"]
        // assert: $self.classification in $domain.classification_universe
        //
        // We model `$domain` here as a let-binding "expected" (per the pattern shape in
        // DESIGN-MDATRON example). Cross-file `key()` is deferred; tests bind it directly.
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

        // $self.classification in $expected.classification_universe
        let expr = Expr::In(
            Box::new(Expr::Field(
                Box::new(Expr::Var(VarRef::SelfVar)),
                "classification".into(),
            )),
            Box::new(Expr::Field(
                Box::new(Expr::Var(VarRef::Binding("expected".into()))),
                "classification_universe".into(),
            )),
        );

        let result = evaluate(&expr, &context).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn classification_universe_rule_fails_for_invalid_finding() {
        let finding = obj([("classification", s("invented-classification"))]);
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
        let expr = Expr::In(
            Box::new(Expr::Field(
                Box::new(Expr::Var(VarRef::SelfVar)),
                "classification".into(),
            )),
            Box::new(Expr::Field(
                Box::new(Expr::Var(VarRef::Binding("expected".into()))),
                "classification_universe".into(),
            )),
        );
        let result = evaluate(&expr, &context).unwrap();
        assert_eq!(result, Value::Bool(false));
    }
}

//! Effect expressions — the one string grammar in Pascal's format (v4
//! `lib/pascal/expressions.ts`, NEW at `c4d4b0de`).
//!
//! An effect's `value` may be an expression: arithmetic, string concatenation,
//! parentheses, literals, and `{{ref}}` substitution. That is the whole
//! language. There are **no identifiers, no function calls, no member access** —
//! the only names the grammar admits are the same `{{...}}` reference families
//! [`super::custom_tools::render_template`] already substitutes (`value`,
//! `roll`, `dice`, `llm`, `params.x`, `metadata.key`, `state.path`), so there is
//! still nothing callable and nothing reachable beyond the run's own subjects.
//! No `eval`, no `Function`: a hand-rolled tokenizer, a recursive-descent
//! parser, and a tree walk.
//!
//! Failure semantics (normative, see the feature spec):
//! - **Parse failure is a load-time rejection** — syntax errors are typos, and
//!   the Workbench and discovery both catch them before a run exists.
//! - **Eval failure is fail-soft at run time** — the effect is skipped with a
//!   debug log; a broken effect never sinks a roll.
//!
//! Pure: no logging, no I/O, and since v4 `0506517d3` the one import is the
//! equally pure placeholder classifier ([`super::placeholders`]). (v4 calls this
//! "client-safe" because its Workbench runs the parser in the browser; v5's
//! browser half is its own TypeScript port under `apps/web/src/app/pascal/`.)
//!
//! ## Every error sentence is user-visible payload
//!
//! Parse reasons flow into `format_definition_issues` (the discovery rejection
//! the author reads) and eval reasons into the recorded skip reasons, so each
//! sentence below is byte-pinned by the definition corpus. Positions are
//! **UTF-16 code-unit indices**, matching JS `String` indexing — the tokenizer
//! therefore walks code units rather than `char`s.

use super::js_value::{number_to_string, to_precision};
use super::placeholders::{classify_placeholder, PlaceholderRef};
use crate::jsstr::js_trim;

/// A value an expression can produce: v4's `number | string | boolean`.
///
/// v4 declares its own `ExprValue`; v5 already had exactly that union as
/// [`ResolvedValue`] (v4's `Primitive`), so the two share one type rather than
/// carrying a second copy that could drift.
pub use super::metadata_match::ResolvedValue as ExprValue;

/// Cap on an effect expression's source text, in characters.
pub const MAX_EFFECT_EXPRESSION_LENGTH: usize = 500;

/// Cap on an expression's token count — no author needs a 65-token formula.
pub const MAX_EFFECT_EXPRESSION_TOKENS: usize = 64;

/// Cap on parenthesis nesting depth.
pub const MAX_EFFECT_EXPRESSION_DEPTH: usize = 8;

/// Render a number for display: integers plain, floats to 4 significant digits.
/// The template convention — `{{value}}` in an outcome message and a number
/// concatenated in an effect expression must read identically.
///
/// Moved here from `custom_tools` with v4's own move (`c4d4b0de`), which
/// re-exports it there for existing importers; v5 does the same.
pub fn format_value(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        return number_to_string(value);
    }
    // `toPrecision` can hand back exponential form for extremes; Number() folds
    // that back to the shortest faithful representation.
    let rounded = to_precision(value, 4).parse::<f64>().unwrap_or(value);
    number_to_string(rounded)
}

/// The parsed tree. Closed by construction: four node kinds and nothing else.
#[derive(Debug, Clone, PartialEq)]
enum ExprNode {
    Literal(ExprValue),
    Ref(String),
    Negate(Box<ExprNode>),
    Binary {
        op: BinOp,
        left: Box<ExprNode>,
        right: Box<ExprNode>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl BinOp {
    fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
        }
    }
}

/// A successfully parsed expression, ready to evaluate.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedExpression {
    root: ExprNode,
    /// Every `{{ref}}` the expression names, in first-appearance order, deduplicated.
    pub refs: Vec<String>,
    /// The source text, for records and messages.
    pub source: String,
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Str(String),
    Boolean(bool),
    Ref(String),
    Op(BinOp),
    LParen,
    RParen,
}

/// The reference families the grammar admits — `render_template`'s, verbatim.
///
/// v4 `0506517d3` routed this through the shared classifier; so does v5. The
/// one rule it carries is the one this module had already spelled out by hand:
/// a bare family prefix (`params.`) names nothing and is not a known ref.
fn is_known_ref(name: &str) -> bool {
    !matches!(classify_placeholder(name), PlaceholderRef::Unknown { .. })
}

/// v4's `ExpressionError` — a thrown message caught at the two entry points.
struct ExpressionError(String);

fn fail<T>(reason: impl Into<String>) -> Result<T, ExpressionError> {
    Err(ExpressionError(reason.into()))
}

/// Decode a UTF-16 code-unit slice back to a `String` (JS `source.slice(a, b)`).
/// A cut that would split a surrogate pair — only reachable with non-BMP text —
/// decodes lossily rather than producing JS's lone-surrogate string.
fn units_to_string(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

fn tokenize(source: &str) -> Result<Vec<Token>, ExpressionError> {
    let units: Vec<u16> = source.encode_utf16().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0usize;

    // ASCII helpers over code units; every syntactic character in the grammar is
    // ASCII, so a unit outside that range is data (a string body) or an error.
    let ch = |u: u16| -> Option<char> {
        if u < 128 {
            Some(u as u8 as char)
        } else {
            None
        }
    };

    while i < units.len() {
        let unit = units[i];
        let c = ch(unit);

        if matches!(c, Some(' ') | Some('\t') | Some('\n') | Some('\r')) {
            i += 1;
            continue;
        }

        if c == Some('(') {
            tokens.push(Token::LParen);
            i += 1;
            continue;
        }
        if c == Some(')') {
            tokens.push(Token::RParen);
            i += 1;
            continue;
        }
        if let Some(op) = match c {
            Some('+') => Some(BinOp::Add),
            Some('-') => Some(BinOp::Sub),
            Some('*') => Some(BinOp::Mul),
            Some('/') => Some(BinOp::Div),
            _ => None,
        } {
            tokens.push(Token::Op(op));
            i += 1;
            continue;
        }

        if c == Some('{') {
            if units.get(i + 1).copied() != Some(u16::from(b'{')) {
                return fail(format!(
                    "a reference opens with \"{{{{\", not a lone \"{{\" (at position {i})"
                ));
            }
            // JS `source.indexOf('}}', i + 2)`.
            let close = (i + 2..units.len().saturating_sub(1))
                .find(|&j| units[j] == u16::from(b'}') && units[j + 1] == u16::from(b'}'));
            let Some(close) = close else {
                return fail(format!(
                    "the reference starting at position {i} never closes with \"}}}}\""
                ));
            };
            let name = js_trim(&units_to_string(&units[i + 2..close])).to_string();
            if !is_known_ref(&name) {
                return fail(format!(
                    "{{{{{name}}}}} is not a reference this grammar knows — use value, roll, dice, llm, params.x, metadata.key, or state.path"
                ));
            }
            tokens.push(Token::Ref(name));
            i = close + 2;
            continue;
        }

        if c == Some('\'') || c == Some('"') {
            let quote = unit;
            let mut text: Vec<u16> = Vec::new();
            i += 1;
            let mut closed = false;
            while i < units.len() {
                let u = units[i];
                if u == u16::from(b'\\') {
                    if i + 1 >= units.len() {
                        return fail("a string ends on a dangling backslash");
                    }
                    text.push(units[i + 1]);
                    i += 2;
                    continue;
                }
                if u == quote {
                    closed = true;
                    i += 1;
                    break;
                }
                text.push(u);
                i += 1;
            }
            if !closed {
                return fail(format!(
                    "a string opened with {} never closes",
                    ch(quote).unwrap_or('?')
                ));
            }
            tokens.push(Token::Str(units_to_string(&text)));
            continue;
        }

        if matches!(c, Some('0'..='9')) {
            // `/^\d+(\.\d+)?/` — JS `\d` is ASCII [0-9].
            let start = i;
            while i < units.len() && matches!(ch(units[i]), Some('0'..='9')) {
                i += 1;
            }
            if ch(units.get(i).copied().unwrap_or(0)) == Some('.')
                && matches!(ch(units.get(i + 1).copied().unwrap_or(0)), Some('0'..='9'))
            {
                i += 1;
                while i < units.len() && matches!(ch(units[i]), Some('0'..='9')) {
                    i += 1;
                }
            }
            let text = units_to_string(&units[start..i]);
            // `Number(text)` over a `\d+(\.\d+)?` literal.
            tokens.push(Token::Number(text.parse::<f64>().unwrap_or(f64::NAN)));
            continue;
        }

        if matches!(c, Some('a'..='z') | Some('A'..='Z') | Some('_')) {
            // `/^[a-zA-Z_][a-zA-Z0-9_]*/`.
            let start = i;
            i += 1;
            while i < units.len()
                && matches!(
                    ch(units[i]),
                    Some('a'..='z') | Some('A'..='Z') | Some('0'..='9') | Some('_')
                )
            {
                i += 1;
            }
            let word = units_to_string(&units[start..i]);
            if word == "true" || word == "false" {
                tokens.push(Token::Boolean(word == "true"));
                continue;
            }
            // Bare words are the quoting trap: `broken pick` instead of `'broken
            // pick'`. Say so, because this is the mistake every author makes once.
            // NB the `{{{{ref}}}}` escape: `format!` folds each doubled brace, so
            // this renders v4's literal `{{ref}}`.
            return fail(format!(
                "\"{word}\" is a bare word — there are no identifiers here. Quote literal text ('{word}') or use a {{{{ref}}}}"
            ));
        }

        // JS interpolates the single code unit; a lone surrogate decodes lossily.
        return fail(format!(
            "unexpected character \"{}\" at position {i}",
            units_to_string(&units[i..i + 1])
        ));
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Parser — recursive descent over the closed grammar
// ---------------------------------------------------------------------------

/// ```text
/// expression = term { ("+" | "-") term } ;
/// term       = factor { ("*" | "/") factor } ;
/// factor     = number | string | boolean | ref | "(" expression ")" | "-" factor ;
/// ```
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn parse(&mut self) -> Result<ExprNode, ExpressionError> {
        let root = self.expression(0)?;
        if let Some(leftover) = self.peek() {
            return fail(format!(
                "unexpected {} after the end of the expression",
                describe_token(leftover)
            ));
        }
        Ok(root)
    }

    fn expression(&mut self, depth: usize) -> Result<ExprNode, ExpressionError> {
        let mut left = self.term(depth)?;
        loop {
            let op = match self.peek() {
                Some(Token::Op(op @ (BinOp::Add | BinOp::Sub))) => *op,
                _ => return Ok(left),
            };
            self.next();
            let right = self.term(depth)?;
            left = ExprNode::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
    }

    fn term(&mut self, depth: usize) -> Result<ExprNode, ExpressionError> {
        let mut left = self.factor(depth)?;
        loop {
            let op = match self.peek() {
                Some(Token::Op(op @ (BinOp::Mul | BinOp::Div))) => *op,
                _ => return Ok(left),
            };
            self.next();
            let right = self.factor(depth)?;
            left = ExprNode::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
    }

    fn factor(&mut self, depth: usize) -> Result<ExprNode, ExpressionError> {
        let Some(token) = self.next() else {
            return fail("the expression ends where a value was expected");
        };

        match token {
            Token::Number(n) => Ok(ExprNode::Literal(ExprValue::Number(n))),
            Token::Str(s) => Ok(ExprNode::Literal(ExprValue::String(s))),
            Token::Boolean(b) => Ok(ExprNode::Literal(ExprValue::Bool(b))),
            Token::Ref(name) => Ok(ExprNode::Ref(name)),
            Token::LParen => {
                if depth + 1 > MAX_EFFECT_EXPRESSION_DEPTH {
                    return fail(format!(
                        "parentheses nest deeper than {MAX_EFFECT_EXPRESSION_DEPTH} levels"
                    ));
                }
                let inner = self.expression(depth + 1)?;
                if self.next() != Some(Token::RParen) {
                    return fail("an opening \"(\" never closes");
                }
                Ok(inner)
            }
            Token::Op(BinOp::Sub) => Ok(ExprNode::Negate(Box::new(self.factor(depth)?))),
            Token::Op(op) => fail(format!(
                "\"{}\" appears where a value was expected",
                op.as_str()
            )),
            Token::RParen => fail("\")\" appears where a value was expected"),
        }
    }
}

fn describe_token(token: &Token) -> String {
    match token {
        Token::Number(n) => format!("number {}", number_to_string(*n)),
        Token::Str(s) => format!("string '{s}'"),
        Token::Boolean(b) => b.to_string(),
        Token::Ref(name) => format!("{{{{{name}}}}}"),
        Token::Op(op) => format!("\"{}\"", op.as_str()),
        Token::LParen => "\"(\"".to_string(),
        Token::RParen => "\")\"".to_string(),
    }
}

fn collect_refs(node: &ExprNode, into: &mut Vec<String>) {
    match node {
        ExprNode::Ref(name) => {
            if !into.iter().any(|n| n == name) {
                into.push(name.clone());
            }
        }
        ExprNode::Negate(operand) => collect_refs(operand, into),
        ExprNode::Binary { left, right, .. } => {
            collect_refs(left, into);
            collect_refs(right, into);
        }
        ExprNode::Literal(_) => {}
    }
}

/// Parse an effect expression. A failure here is a load-time rejection — the
/// dice-notation doctrine: syntax errors are typos, caught in the Workbench and
/// at discovery, never at the table.
///
/// `Err` carries v4's `reason` string verbatim.
pub fn parse_expression(source: &str) -> Result<ParsedExpression, String> {
    // JS `source.length` — UTF-16 code units.
    let len = crate::jsstr::utf16_len(source);
    if len == 0 {
        return Err("the expression is empty".to_string());
    }
    if len > MAX_EFFECT_EXPRESSION_LENGTH {
        return Err(format!(
            "the expression exceeds {MAX_EFFECT_EXPRESSION_LENGTH} characters"
        ));
    }

    let tokens = tokenize(source).map_err(|e| e.0)?;
    if tokens.is_empty() {
        return Err("the expression is empty".to_string());
    }
    if tokens.len() > MAX_EFFECT_EXPRESSION_TOKENS {
        return Err(format!(
            "the expression exceeds {MAX_EFFECT_EXPRESSION_TOKENS} tokens"
        ));
    }
    let root = Parser::new(tokens).parse().map_err(|e| e.0)?;
    let mut refs = Vec::new();
    collect_refs(&root, &mut refs);
    Ok(ParsedExpression {
        root,
        refs,
        source: source.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

/// Concatenation stringifies by the template convention.
fn stringify(value: &ExprValue) -> String {
    match value {
        ExprValue::Number(n) => format_value(*n),
        ExprValue::String(s) => s.clone(),
        ExprValue::Bool(b) => b.to_string(),
    }
}

/// JS `typeof`.
fn type_name(value: &ExprValue) -> &'static str {
    match value {
        ExprValue::Number(_) => "number",
        ExprValue::String(_) => "string",
        ExprValue::Bool(_) => "boolean",
    }
}

/// Any non-finite arithmetic result — including division by zero — fails the eval.
fn finite_or_fail(result: f64, op: &str) -> Result<f64, ExpressionError> {
    if !result.is_finite() {
        return fail(format!(
            "\"{op}\" produced a value that is not a finite number"
        ));
    }
    Ok(result)
}

fn evaluate_node(
    node: &ExprNode,
    resolve_ref: &mut dyn FnMut(&str) -> Option<ExprValue>,
) -> Result<ExprValue, ExpressionError> {
    match node {
        ExprNode::Literal(v) => Ok(v.clone()),

        ExprNode::Ref(name) => match resolve_ref(name) {
            Some(v) => Ok(v),
            None => fail(format!("{{{{{name}}}}} did not resolve to a value")),
        },

        ExprNode::Negate(operand) => {
            let operand = evaluate_node(operand, resolve_ref)?;
            let ExprValue::Number(n) = operand else {
                return fail(format!(
                    "\"-\" negates a {}, and only numbers can be negated",
                    type_name(&operand)
                ));
            };
            Ok(ExprValue::Number(-n))
        }

        ExprNode::Binary { op, left, right } => {
            let left = evaluate_node(left, resolve_ref)?;
            let right = evaluate_node(right, resolve_ref)?;

            if *op == BinOp::Add {
                if let (ExprValue::Number(a), ExprValue::Number(b)) = (&left, &right) {
                    return Ok(ExprValue::Number(finite_or_fail(a + b, "+")?));
                }
                // At least one string operand → concatenation. Booleans participate
                // only here (as "true"/"false"); there is no truthiness arithmetic.
                if matches!(left, ExprValue::String(_)) || matches!(right, ExprValue::String(_)) {
                    return Ok(ExprValue::String(format!(
                        "{}{}",
                        stringify(&left),
                        stringify(&right)
                    )));
                }
                return fail(format!(
                    "\"+\" adds a {} and a {} — it takes two numbers, or a string to concatenate",
                    type_name(&left),
                    type_name(&right)
                ));
            }

            let (ExprValue::Number(a), ExprValue::Number(b)) = (&left, &right) else {
                return fail(format!(
                    "\"{}\" takes two numbers, got a {} and a {}",
                    op.as_str(),
                    type_name(&left),
                    type_name(&right)
                ));
            };
            let result = match op {
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                BinOp::Div => a / b,
                BinOp::Add => unreachable!("handled above"),
            };
            Ok(ExprValue::Number(finite_or_fail(result, op.as_str())?))
        }
    }
}

/// Evaluate a parsed expression against a run's subjects.
///
/// `resolve_ref` returning `None` (an absent metadata key, a non-primitive state
/// value, a consult that never ran) is an eval failure — the caller skips that
/// effect fail-soft, per the `render_template` doctrine: a broken effect never
/// sinks a roll.
///
/// `Err` carries v4's `reason` string verbatim.
pub fn evaluate_expression(
    expr: &ParsedExpression,
    resolve_ref: &mut dyn FnMut(&str) -> Option<ExprValue>,
) -> Result<ExprValue, String> {
    evaluate_node(&expr.root, resolve_ref).map_err(|e| e.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(src: &str) -> Result<ExprValue, String> {
        let expr = parse_expression(src)?;
        evaluate_expression(&expr, &mut |_| None)
    }

    #[test]
    fn arithmetic_precedence_and_parens() {
        assert_eq!(eval("2 + 3 * 4").unwrap(), ExprValue::Number(14.0));
        assert_eq!(eval("(2 + 3) * 4").unwrap(), ExprValue::Number(20.0));
        assert_eq!(eval("-3 + 1").unwrap(), ExprValue::Number(-2.0));
    }

    #[test]
    fn division_by_zero_is_an_eval_failure() {
        assert_eq!(
            eval("1 / 0").unwrap_err(),
            "\"/\" produced a value that is not a finite number"
        );
    }

    #[test]
    fn concatenation_uses_the_template_number_convention() {
        assert_eq!(
            eval("'v' + 1.23456").unwrap(),
            ExprValue::String("v1.235".to_string())
        );
        assert_eq!(
            eval("'x' + true").unwrap(),
            ExprValue::String("xtrue".to_string())
        );
    }

    #[test]
    fn bare_words_carry_the_quoting_trap_sentence() {
        assert_eq!(
            parse_expression("broken pick").unwrap_err(),
            "\"broken\" is a bare word — there are no identifiers here. Quote literal text ('broken') or use a {{ref}}"
        );
    }

    #[test]
    fn refs_are_first_appearance_order_deduped() {
        let expr = parse_expression("{{value}} + {{params.a}} + {{value}}").unwrap();
        assert_eq!(expr.refs, vec!["value", "params.a"]);
    }

    #[test]
    fn unknown_reference_families_are_rejected() {
        assert_eq!(
            parse_expression("{{nope}}").unwrap_err(),
            "{{nope}} is not a reference this grammar knows — use value, roll, dice, llm, params.x, metadata.key, or state.path"
        );
    }

    #[test]
    fn positions_are_utf16_code_units() {
        // "é" is one UTF-16 unit but two UTF-8 bytes; the reported position is 1.
        assert_eq!(
            parse_expression("é#").unwrap_err(),
            "unexpected character \"é\" at position 0"
        );
        assert_eq!(
            parse_expression("1 é").unwrap_err(),
            "unexpected character \"é\" at position 2"
        );
    }

    #[test]
    fn depth_cap_counts_nesting_not_tokens() {
        let deep = format!("{}1{}", "(".repeat(9), ")".repeat(9));
        assert_eq!(
            parse_expression(&deep).unwrap_err(),
            "parentheses nest deeper than 8 levels"
        );
        let ok = format!("{}1{}", "(".repeat(8), ")".repeat(8));
        assert!(parse_expression(&ok).is_ok());
    }
}

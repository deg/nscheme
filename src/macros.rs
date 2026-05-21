//! `syntax-rules` macros (R7RS §4.3).
//!
//! Implements hygienic pattern-matching macros via the textbook
//! alpha-renaming approach (KFFD):
//!
//! 1. Each clause of the macro has a *pattern* and a *template*.
//! 2. Expansion picks the first clause whose pattern matches the input,
//!    collecting pattern-variable bindings.
//! 3. Identifiers introduced by the template that appear in *binding
//!    positions* (`let`, `lambda`, `define`, etc.) are renamed with a
//!    fresh `gensym` to avoid capturing user-supplied identifiers.
//! 4. Pattern variables are substituted from the bindings; `...` in
//!    template position splices sub-lists collected by `...` in pattern
//!    position.
//! 5. The expanded form is then re-evaluated through `step_eval`, so
//!    nested macro calls expand naturally.
//!
//! **Limitations documented for v1** (each is a deliberate scope cut):
//! - Free identifiers in templates use the *call site's* environment,
//!   not the *macro definition site's*. That means `(define-syntax m
//!   (syntax-rules () ((_ x) (+ x 1))))` followed by `(let ((+ -)) (m
//!   5))` returns 4, not 6. Most amateur Schemes have this bug;
//!   fixing it requires syntactic closures / sets of scopes.
//! - Nested ellipsis depth >1 is not supported.
//! - Custom ellipsis identifier (`(syntax-rules ... () ...)`) is not
//!   supported.

// The walkers here are inherently long — they enumerate syntactic
// forms — and the helper Matches/Bindings types have method-free
// passes. Allow clippy's size and style cops at module level.
#![allow(clippy::too_many_lines)]
#![allow(clippy::single_match_else)]
#![allow(clippy::collapsible_match)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::redundant_pattern_matching)]

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use crate::eval::EvalError;
use crate::value::{Symbol, Value};

/// A compiled `(syntax-rules (LITERALS...) CLAUSES...)` form.
#[derive(Debug)]
pub struct SyntaxRules {
    pub literals: HashSet<Symbol>,
    pub clauses: Vec<SyntaxClause>,
}

#[derive(Debug)]
pub struct SyntaxClause {
    pub pattern: Value,
    pub template: Value,
}

/// Parse a `(syntax-rules (literals...) clauses...)` form.
pub fn parse_syntax_rules(form: &Value) -> Result<SyntaxRules, EvalError> {
    let parts =
        collect_list(form).ok_or_else(|| malformed("syntax-rules form must be a proper list"))?;
    if parts.is_empty() || !matches!(&parts[0], Value::Symbol(s) if s.name() == "syntax-rules") {
        return Err(malformed("expected (syntax-rules ...)"));
    }
    if parts.len() < 2 {
        return Err(malformed("syntax-rules needs at least a literals list"));
    }
    let literals_list =
        collect_list(&parts[1]).ok_or_else(|| malformed("literals must be a proper list"))?;
    let mut literals: HashSet<Symbol> = HashSet::new();
    for lit in literals_list {
        match lit {
            Value::Symbol(s) => {
                literals.insert(s);
            }
            _ => return Err(malformed("literal must be an identifier")),
        }
    }
    let mut clauses: Vec<SyntaxClause> = Vec::new();
    for clause in &parts[2..] {
        let cparts = collect_list(clause)
            .ok_or_else(|| malformed("each clause must be a (pattern template) pair"))?;
        if cparts.len() != 2 {
            return Err(malformed("clause must be (pattern template)"));
        }
        clauses.push(SyntaxClause {
            pattern: cparts[0].clone(),
            template: cparts[1].clone(),
        });
    }
    Ok(SyntaxRules { literals, clauses })
}

/// Top-level expansion: walk the clauses, match the first one whose
/// pattern fits, and instantiate its template.
pub fn expand(rules: &SyntaxRules, call: &Value) -> Result<Value, EvalError> {
    for clause in &rules.clauses {
        let mut bindings = Bindings::default();
        if pattern_match(&clause.pattern, call, &rules.literals, &mut bindings) {
            // Determine template-introduced identifiers in binding
            // positions; rename them with a gensym to preserve hygiene.
            let mut binders = HashSet::new();
            collect_binders(&clause.template, &mut binders);
            // Pattern variables shouldn't be renamed even if they
            // happen to also appear in a binding position in the
            // template.
            for v in bindings.0.keys() {
                binders.remove(v);
            }
            let renames: HashMap<Symbol, Symbol> = binders
                .into_iter()
                .map(|s| (s.clone(), gensym(&s)))
                .collect();
            return instantiate(&clause.template, &bindings, &renames);
        }
    }
    Err(malformed("no syntax-rules clause matched"))
}

// ---------------------------------------------------------------------
// Pattern matching
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct Bindings(HashMap<Symbol, Match>);

#[derive(Debug, Clone)]
enum Match {
    /// Single-value binding (pattern variable not under `...`).
    Single(Value),
    /// Sequence of nested bindings, one per occurrence under `...`.
    Multi(Vec<Bindings>),
}

fn pattern_match(
    pattern: &Value,
    input: &Value,
    literals: &HashSet<Symbol>,
    bindings: &mut Bindings,
) -> bool {
    match pattern {
        Value::Symbol(s) => {
            if literals.contains(s) {
                // Literal — input must be the same symbol.
                matches!(input, Value::Symbol(i) if i == s)
            } else if s.name() == "_" {
                // Wildcard.
                true
            } else {
                bindings.0.insert(s.clone(), Match::Single(input.clone()));
                true
            }
        }
        Value::Null => matches!(input, Value::Null),
        Value::Pair(_) => match_list(pattern, input, literals, bindings),
        // Everything else: literal datum equality.
        _ => crate::value::equal(pattern, input),
    }
}

fn match_list(
    pattern: &Value,
    input: &Value,
    literals: &HashSet<Symbol>,
    bindings: &mut Bindings,
) -> bool {
    // Walk pattern looking for `...` after some pattern element.
    let elems = pattern_elems(pattern);
    let (proper_pattern, dotted_tail) = elems;

    // Find the position of `...` if any (must follow a sub-pattern).
    let mut ellipsis_at: Option<usize> = None;
    for (i, p) in proper_pattern.iter().enumerate() {
        if is_ellipsis(p) && i > 0 {
            ellipsis_at = Some(i - 1);
            break;
        }
    }

    let input_list = collect_list(input);
    if input_list.is_none() && !matches!(input, Value::Pair(_) | Value::Null) {
        return false;
    }

    if let Some(idx) = ellipsis_at {
        // Pattern: [p0, ..., p_{idx-1}, p_idx, ELLIPSIS, p_{idx+2}, ..., p_{n-1}]
        // The `p_idx ...` segment matches zero or more elements.
        let post_count = proper_pattern.len() - (idx + 2);
        let Some(input_items) = collect_list(input) else {
            return false;
        };
        if input_items.len() < idx + post_count {
            return false;
        }
        // Match prefix.
        for (p, v) in proper_pattern[..idx].iter().zip(input_items.iter()) {
            if !pattern_match(p, v, literals, bindings) {
                return false;
            }
        }
        // Match the repeating segment.
        let repeat_count = input_items.len() - idx - post_count;
        let mut multi: Vec<Bindings> = Vec::with_capacity(repeat_count);
        for v in &input_items[idx..idx + repeat_count] {
            let mut sub = Bindings::default();
            if !pattern_match(&proper_pattern[idx], v, literals, &mut sub) {
                return false;
            }
            multi.push(sub);
        }
        // Merge multi into bindings.
        for var in collect_pattern_vars(&proper_pattern[idx], literals) {
            let collected: Vec<Match> = multi
                .iter()
                .map(|b| b.0.get(&var).cloned().unwrap_or(Match::Single(Value::Null)))
                .collect();
            // Each Match in `collected` is a Single per repetition;
            // store as Multi(per-rep-bindings).
            // We store as Multi of Bindings (one per rep) so depth
            // tracking is uniform.
            let _ = collected;
            // Build a fresh Multi(Vec<Bindings>) where each inner
            // Bindings contains only `var`.
            let per_rep: Vec<Bindings> = multi
                .iter()
                .map(|b| {
                    let mut single = Bindings::default();
                    if let Some(m) = b.0.get(&var) {
                        single.0.insert(var.clone(), m.clone());
                    }
                    single
                })
                .collect();
            bindings.0.insert(var, Match::Multi(per_rep));
        }
        // Match postfix.
        for (p, v) in proper_pattern[idx + 2..]
            .iter()
            .zip(input_items[idx + repeat_count..].iter())
        {
            if !pattern_match(p, v, literals, bindings) {
                return false;
            }
        }
        // Match dotted tail if any.
        if let Some(tail_pat) = dotted_tail {
            // The remaining input tail is everything we didn't consume.
            // But for proper-list inputs we've consumed everything already.
            // Improper-list pattern matching against proper-list input is
            // an error case — return false unless tail_pat matches Null.
            return pattern_match(&tail_pat, &Value::Null, literals, bindings);
        }
        return true;
    }

    // No ellipsis — straightforward element-by-element.
    match input_list {
        Some(input_items) => {
            // No ellipsis: pattern and input must have matching length,
            // including any dotted tail.
            if let Some(_) = &dotted_tail {
                // Pattern has improper tail. Take input_items first
                // proper_pattern.len() items, then match tail against
                // rest.
                if input_items.len() < proper_pattern.len() {
                    return false;
                }
                for (p, v) in proper_pattern.iter().zip(input_items.iter()) {
                    if !pattern_match(p, v, literals, bindings) {
                        return false;
                    }
                }
                let rest_items = input_items[proper_pattern.len()..].to_vec();
                let rest_tail = Value::list_from(rest_items);
                return pattern_match(
                    dotted_tail.as_ref().unwrap(),
                    &rest_tail,
                    literals,
                    bindings,
                );
            }
            if input_items.len() != proper_pattern.len() {
                return false;
            }
            for (p, v) in proper_pattern.iter().zip(input_items.iter()) {
                if !pattern_match(p, v, literals, bindings) {
                    return false;
                }
            }
            true
        }
        None => {
            // Input is an improper list. Walk both.
            let mut input_cur = input.clone();
            for p in &proper_pattern {
                let Some((c, d)) = input_cur.as_pair() else {
                    return false;
                };
                if !pattern_match(p, &c, literals, bindings) {
                    return false;
                }
                input_cur = d;
            }
            if let Some(tail_pat) = dotted_tail {
                pattern_match(&tail_pat, &input_cur, literals, bindings)
            } else {
                matches!(input_cur, Value::Null)
            }
        }
    }
}

/// Split a pattern list into the proper-prefix elements and an
/// optional dotted-tail pattern.
fn pattern_elems(pattern: &Value) -> (Vec<Value>, Option<Value>) {
    let mut out: Vec<Value> = Vec::new();
    let mut cur = pattern.clone();
    loop {
        match cur {
            Value::Null => return (out, None),
            Value::Pair(p) => {
                let pair = p.borrow();
                out.push(pair.car.clone());
                cur = pair.cdr.clone();
            }
            other => return (out, Some(other)),
        }
    }
}

fn is_ellipsis(v: &Value) -> bool {
    matches!(v, Value::Symbol(s) if s.name() == "...")
}

fn collect_pattern_vars(pattern: &Value, literals: &HashSet<Symbol>) -> Vec<Symbol> {
    let mut out: Vec<Symbol> = Vec::new();
    collect_pattern_vars_into(pattern, literals, &mut out);
    out
}

fn collect_pattern_vars_into(pattern: &Value, literals: &HashSet<Symbol>, out: &mut Vec<Symbol>) {
    match pattern {
        Value::Symbol(s) if !literals.contains(s) && s.name() != "_" && s.name() != "..." => {
            if !out.iter().any(|x| x == s) {
                out.push(s.clone());
            }
        }
        Value::Pair(_) => {
            let mut cur = pattern.clone();
            while let Value::Pair(p) = cur {
                let pair = p.borrow();
                collect_pattern_vars_into(&pair.car, literals, out);
                cur = pair.cdr.clone();
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------
// Template instantiation
// ---------------------------------------------------------------------

fn instantiate(
    template: &Value,
    bindings: &Bindings,
    renames: &HashMap<Symbol, Symbol>,
) -> Result<Value, EvalError> {
    match template {
        Value::Symbol(s) => {
            if let Some(Match::Single(v)) = bindings.0.get(s) {
                return Ok(v.clone());
            }
            if let Some(renamed) = renames.get(s) {
                return Ok(Value::Symbol(renamed.clone()));
            }
            Ok(Value::Symbol(s.clone()))
        }
        Value::Pair(_) => instantiate_list(template, bindings, renames),
        _ => Ok(template.clone()),
    }
}

fn instantiate_list(
    template: &Value,
    bindings: &Bindings,
    renames: &HashMap<Symbol, Symbol>,
) -> Result<Value, EvalError> {
    let (elems, tail) = pattern_elems(template);
    let mut out: Vec<Value> = Vec::new();
    let mut i = 0;
    while i < elems.len() {
        // If the next element is `...`, splice the previous template
        // element repeatedly.
        if i + 1 < elems.len() && is_ellipsis(&elems[i + 1]) {
            // Collect the multi-binding for the variables in elems[i].
            let pattern_vars = collect_template_vars(&elems[i], bindings);
            // All vars must agree on the number of repetitions.
            let mut reps: Option<usize> = None;
            for v in &pattern_vars {
                if let Some(Match::Multi(items)) = bindings.0.get(v) {
                    let n = items.len();
                    match reps {
                        None => reps = Some(n),
                        Some(prev) if prev == n => {}
                        Some(prev) => {
                            return Err(malformed(&format!(
                                "ellipsis variables disagree on rep count: {prev} vs {n}"
                            )));
                        }
                    }
                }
            }
            let n = reps.unwrap_or(0);
            for r in 0..n {
                // Build a per-rep binding view: each Multi var is
                // replaced by the r-th entry's Single binding.
                let mut rep_bindings = Bindings::default();
                for (k, m) in &bindings.0 {
                    match m {
                        Match::Single(v) => {
                            rep_bindings.0.insert(k.clone(), Match::Single(v.clone()));
                        }
                        Match::Multi(items) => {
                            if let Some(inner) = items.get(r) {
                                if let Some(inner_m) = inner.0.get(k) {
                                    rep_bindings.0.insert(k.clone(), inner_m.clone());
                                }
                            }
                        }
                    }
                }
                out.push(instantiate(&elems[i], &rep_bindings, renames)?);
            }
            i += 2;
        } else {
            out.push(instantiate(&elems[i], bindings, renames)?);
            i += 1;
        }
    }
    let tail_value = match tail {
        Some(t) => instantiate(&t, bindings, renames)?,
        None => Value::Null,
    };
    // Build the list.
    let mut acc = tail_value;
    for item in out.into_iter().rev() {
        acc = Value::cons(item, acc);
    }
    Ok(acc)
}

/// Find pattern variables referenced in a template subtree.
fn collect_template_vars(template: &Value, bindings: &Bindings) -> Vec<Symbol> {
    let mut out = Vec::new();
    collect_template_vars_into(template, bindings, &mut out);
    out
}

fn collect_template_vars_into(template: &Value, bindings: &Bindings, out: &mut Vec<Symbol>) {
    match template {
        Value::Symbol(s) if bindings.0.contains_key(s) => {
            if !out.iter().any(|x| x == s) {
                out.push(s.clone());
            }
        }
        Value::Pair(_) => {
            let mut cur = template.clone();
            while let Value::Pair(p) = cur {
                let pair = p.borrow();
                collect_template_vars_into(&pair.car, bindings, out);
                cur = pair.cdr.clone();
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------
// Hygiene: collect template-introduced binding-position identifiers
// ---------------------------------------------------------------------

/// Walk the template looking for identifiers that appear in binding
/// positions: parameters of `lambda`, the bound name in `let`/`let*`/
/// `letrec`, the lhs of `define`, etc. These get gensym'd to avoid
/// capturing user-supplied identifiers (the canonical R7RS `swap!`
/// hygiene example).
fn collect_binders(template: &Value, out: &mut HashSet<Symbol>) {
    let Value::Pair(_) = template else { return };
    let (elems, _) = pattern_elems(template);
    if elems.is_empty() {
        return;
    }
    // Inspect the head to decide which sub-positions are binders.
    if let Value::Symbol(head) = &elems[0] {
        match head.name() {
            "lambda" if elems.len() >= 2 => {
                collect_formals(&elems[1], out);
                for b in &elems[2..] {
                    collect_binders(b, out);
                }
                return;
            }
            "let" | "let*" | "letrec" | "letrec*" if elems.len() >= 2 => {
                // (let ((v e) ...) body) — binders are the v's.
                // Named let: (let name ((v e) ...) body)
                let (bindings_idx, body_idx) = if matches!(&elems[1], Value::Symbol(_)) {
                    // Named let — name is also a binder.
                    if let Value::Symbol(n) = &elems[1] {
                        out.insert(n.clone());
                    }
                    (2, 3)
                } else {
                    (1, 2)
                };
                if elems.len() > bindings_idx {
                    if let Value::Pair(_) = &elems[bindings_idx] {
                        let (bs, _) = pattern_elems(&elems[bindings_idx]);
                        for b in &bs {
                            if let Some((name, _val_tail)) = b.as_pair()
                                && let Value::Symbol(s) = name
                            {
                                out.insert(s);
                            }
                            // Also walk the value expression for nested binders.
                            if let Some((_n, val_tail)) = b.as_pair() {
                                if let Some((val, _)) = val_tail.as_pair() {
                                    collect_binders(&val, out);
                                }
                            }
                        }
                    }
                }
                for b in &elems[body_idx..] {
                    collect_binders(b, out);
                }
                return;
            }
            "define" if elems.len() >= 2 => {
                match &elems[1] {
                    Value::Symbol(s) => {
                        out.insert(s.clone());
                    }
                    Value::Pair(_) => {
                        // (define (name . formals) body)
                        if let Some((name, formals)) = elems[1].as_pair() {
                            if let Value::Symbol(s) = name {
                                out.insert(s);
                            }
                            collect_formals(&formals, out);
                        }
                    }
                    _ => {}
                }
                for b in &elems[2..] {
                    collect_binders(b, out);
                }
                return;
            }
            "do" if elems.len() >= 2 => {
                if let Value::Pair(_) = &elems[1] {
                    let (bs, _) = pattern_elems(&elems[1]);
                    for b in &bs {
                        if let Some((name, _)) = b.as_pair()
                            && let Value::Symbol(s) = name
                        {
                            out.insert(s);
                        }
                    }
                }
                for b in &elems[2..] {
                    collect_binders(b, out);
                }
                return;
            }
            _ => {}
        }
    }
    // Default: recurse into every sub-expression.
    for e in &elems {
        collect_binders(e, out);
    }
}

fn collect_formals(formals: &Value, out: &mut HashSet<Symbol>) {
    let mut cur = formals.clone();
    loop {
        match cur {
            Value::Null => return,
            Value::Symbol(s) => {
                out.insert(s);
                return;
            }
            Value::Pair(p) => {
                let pair = p.borrow();
                if let Value::Symbol(s) = &pair.car {
                    out.insert(s.clone());
                }
                cur = pair.cdr.clone();
            }
            _ => return,
        }
    }
}

// ---------------------------------------------------------------------
// Gensym
// ---------------------------------------------------------------------

thread_local! {
    static GENSYM_COUNTER: Cell<u64> = const { Cell::new(0) };
}

fn gensym(base: &Symbol) -> Symbol {
    let n = GENSYM_COUNTER.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    });
    Symbol::intern(&format!("{}#{}", base.name(), n))
}

// ---------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------

fn collect_list(v: &Value) -> Option<Vec<Value>> {
    let mut out = Vec::new();
    let mut cur = v.clone();
    loop {
        match cur {
            Value::Null => return Some(out),
            Value::Pair(p) => {
                let pair = p.borrow();
                out.push(pair.car.clone());
                cur = pair.cdr.clone();
            }
            _ => return None,
        }
    }
}

fn malformed(msg: &str) -> EvalError {
    EvalError::MalformedForm {
        form: "syntax-rules",
        message: msg.to_string(),
    }
}

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

use crate::env::EnvRef;
use crate::eval::EvalError;
use crate::value::{Symbol, Value};

/// A pattern-variable identifier. `name` is the bare symbol name;
/// `scope` is the env-pointer of the `SyntaxRef` that introduced
/// it (or `None` for a plain `Symbol`). Two identifiers with the
/// same name but different scopes are distinct keys, so a macro's
/// template-introduced `x` (SyntaxRef-wrapped) does not collide
/// with the user's `x` (plain Symbol) that another macro
/// substituted into the same template.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct VarKey {
    name: Symbol,
    scope: Option<usize>,
}

impl VarKey {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Symbol(s) => Some(Self {
                name: s.clone(),
                scope: None,
            }),
            Value::SyntaxRef { name, env } => Some(Self {
                name: name.clone(),
                scope: Some(std::rc::Rc::as_ptr(env) as usize),
            }),
            _ => None,
        }
    }
}

/// A compiled `(syntax-rules (LITERALS...) CLAUSES...)` form. R7RS
/// also allows `(syntax-rules <ELLIPSIS-ID> (LITERALS...) CLAUSES...)`
/// to rename the ellipsis marker; we store that here.
#[derive(Debug)]
pub struct SyntaxRules {
    /// Name of the ellipsis marker for this macro. Defaults to `...`
    /// but can be any identifier via the renaming form.
    pub ellipsis: Symbol,
    /// The original literal identifiers from the `(literals)` list,
    /// preserved with any hygiene marks. Pattern matching compares
    /// inputs against these using R7RS's same-binding rule: a
    /// literal matches when the input's identifier resolves to the
    /// same binding (or both are unbound and have identical
    /// hygiene marks).
    pub literals: Vec<Value>,
    pub clauses: Vec<SyntaxClause>,
    /// Definition-site environment, used by `expand` to wrap free
    /// identifiers in the template so they resolve to their
    /// definition-site bindings rather than the call site's. R7RS
    /// hygiene §4.3.2.
    pub def_env: Option<EnvRef>,
}

#[derive(Debug)]
pub struct SyntaxClause {
    pub pattern: Value,
    pub template: Value,
}

/// Parse a `(syntax-rules (literals...) clauses...)` form.
/// R7RS also allows `(syntax-rules ELLIPSIS-ID (literals...) clauses...)`
/// where `ELLIPSIS-ID` renames the `...` marker.
pub fn parse_syntax_rules(form: &Value) -> Result<SyntaxRules, EvalError> {
    let parts =
        collect_list(form).ok_or_else(|| malformed("syntax-rules form must be a proper list"))?;
    if parts.is_empty() || !parts[0].is_keyword("syntax-rules") {
        return Err(malformed("expected (syntax-rules ...)"));
    }
    if parts.len() < 2 {
        return Err(malformed("syntax-rules needs at least a literals list"));
    }
    // Detect the renamed-ellipsis form: if parts[1] is an identifier
    // (not a list), it's the ellipsis identifier and parts[2] is the
    // literals list.
    let (ellipsis, literals_idx) = match parts[1].as_identifier() {
        Some(s) => (s.clone(), 2),
        None => (Symbol::intern("..."), 1),
    };
    if parts.len() <= literals_idx {
        return Err(malformed("syntax-rules needs at least a literals list"));
    }
    let literals_list = collect_list(&parts[literals_idx])
        .ok_or_else(|| malformed("literals must be a proper list"))?;
    let mut literals: Vec<Value> = Vec::new();
    for lit in literals_list {
        if lit.as_identifier().is_none() {
            return Err(malformed("literal must be an identifier"));
        }
        literals.push(lit);
    }
    let mut clauses: Vec<SyntaxClause> = Vec::new();
    for clause in &parts[literals_idx + 1..] {
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
    Ok(SyntaxRules {
        ellipsis,
        literals,
        clauses,
        def_env: None,
    })
}

/// Top-level expansion: walk the clauses, match the first one whose
/// pattern fits, and instantiate its template.
pub fn expand(rules: &SyntaxRules, call: &Value) -> Result<Value, EvalError> {
    // R7RS §4.3.2: a literal listed in `(literals)` matches itself.
    // When the ellipsis identifier itself is in the literals, the
    // matcher and the expander both stop treating it as a
    // repetition marker. Substitute a sentinel name that can never
    // appear in user code.
    let active_ellipsis = if rules
        .literals
        .iter()
        .any(|l| l.as_identifier() == Some(&rules.ellipsis))
    {
        Symbol::intern("\u{0}__nscheme_disabled_ellipsis__\u{0}")
    } else {
        rules.ellipsis.clone()
    };
    for clause in &rules.clauses {
        // R7RS §4.3.2: the first sub-pattern is conventionally the
        // macro keyword. Pattern matching against the macro name is
        // implicit — strip the first element of both pattern and
        // call so a literal in position 0 (e.g. `(_)` with `_` in
        // the literals list) doesn't reject the call.
        let stripped_pattern = strip_head(&clause.pattern);
        let stripped_call = strip_head(call);
        let mut bindings = Bindings::default();
        if pattern_match(
            &stripped_pattern,
            &stripped_call,
            &rules.literals,
            &active_ellipsis,
            &mut bindings,
        ) {
            // Determine template-introduced identifiers in binding
            // positions; rename them with a gensym to preserve hygiene.
            let mut binders = HashSet::new();
            collect_binders(&clause.template, &mut binders);
            // Pattern variables shouldn't be renamed even if they
            // happen to also appear in a binding position in the
            // template.
            for v in bindings.0.keys() {
                binders.remove(&v.name);
            }
            let renames: HashMap<Symbol, Symbol> = binders
                .into_iter()
                .map(|s| (s.clone(), gensym(&s)))
                .collect();
            return instantiate(
                &clause.template,
                &bindings,
                &renames,
                &active_ellipsis,
                rules.def_env.as_ref(),
            );
        }
    }
    Err(malformed("no syntax-rules clause matched"))
}

// ---------------------------------------------------------------------
// Pattern matching
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct Bindings(HashMap<VarKey, Match>);

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
    literals: &[Value],
    ellipsis: &Symbol,
    bindings: &mut Bindings,
) -> bool {
    if let Some(s) = pattern.as_identifier() {
        if literal_matches(pattern, literals) {
            // R7RS §4.3.2: a literal matches when the input
            // identifier resolves to the same binding as the
            // literal — or, when both are unbound, when their
            // hygiene marks agree.
            return input_matches_literal(input, pattern);
        }
        if s.name() == "_" {
            // Wildcard.
            return true;
        }
        if let Some(key) = VarKey::from_value(pattern) {
            bindings.0.insert(key, Match::Single(input.clone()));
        }
        return true;
    }
    match pattern {
        Value::Null => matches!(input, Value::Null),
        Value::Pair(_) => match_list(pattern, input, literals, ellipsis, bindings),
        // Everything else: literal datum equality.
        _ => crate::value::equal(pattern, input),
    }
}

fn match_list(
    pattern: &Value,
    input: &Value,
    literals: &[Value],
    ellipsis: &Symbol,
    bindings: &mut Bindings,
) -> bool {
    let (proper_pattern, dotted_tail) = pattern_elems(pattern);

    // R7RS §4.3.2: a literal identifier matches itself in patterns,
    // and that includes the ellipsis spelling — when the ellipsis
    // identifier is listed in the literals, treat it as a regular
    // literal rather than a repetition marker.
    let ellipsis_is_literal = literals.iter().any(|l| l.as_identifier() == Some(ellipsis));
    let mut ellipsis_at: Option<usize> = None;
    if !ellipsis_is_literal {
        for (i, p) in proper_pattern.iter().enumerate() {
            if is_named_ellipsis(p, ellipsis) && i > 0 {
                ellipsis_at = Some(i - 1);
                break;
            }
        }
    }

    // Walk the input spine, collecting proper elements and recording
    // any non-Null tail. Improper input gives the tail; proper input
    // ends with Null.
    let (input_items, input_tail) = collect_list_with_tail(input);

    if let Some(idx) = ellipsis_at {
        // Pattern: [p0, ..., p_{idx-1}, p_idx, ELLIPSIS, p_{idx+2}, ..., p_{n-1}] [. tail]
        let post_count = proper_pattern.len() - (idx + 2);
        if input_items.len() < idx + post_count {
            return false;
        }
        // Prefix.
        for (p, v) in proper_pattern[..idx].iter().zip(input_items.iter()) {
            if !pattern_match(p, v, literals, ellipsis, bindings) {
                return false;
            }
        }
        // Repetition.
        let repeat_count = input_items.len() - idx - post_count;
        let mut multi: Vec<Bindings> = Vec::with_capacity(repeat_count);
        for v in &input_items[idx..idx + repeat_count] {
            let mut sub = Bindings::default();
            if !pattern_match(&proper_pattern[idx], v, literals, ellipsis, &mut sub) {
                return false;
            }
            multi.push(sub);
        }
        for var in collect_pattern_vars(&proper_pattern[idx], literals, ellipsis) {
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
        // Postfix.
        for (p, v) in proper_pattern[idx + 2..]
            .iter()
            .zip(input_items[idx + repeat_count..].iter())
        {
            if !pattern_match(p, v, literals, ellipsis, bindings) {
                return false;
            }
        }
        // Dotted tail.
        return match dotted_tail {
            Some(tail_pat) => pattern_match(&tail_pat, &input_tail, literals, ellipsis, bindings),
            None => matches!(input_tail, Value::Null),
        };
    }

    // No ellipsis — straight element-by-element.
    if input_items.len() < proper_pattern.len() {
        return false;
    }
    if dotted_tail.is_none() && input_items.len() != proper_pattern.len() {
        return false;
    }
    for (p, v) in proper_pattern.iter().zip(input_items.iter()) {
        if !pattern_match(p, v, literals, ellipsis, bindings) {
            return false;
        }
    }
    // Anything beyond proper_pattern.len() in the input forms the
    // residue that the dotted-tail pattern (if any) matches.
    let residue_items: Vec<Value> = input_items[proper_pattern.len()..].to_vec();
    let residue = if residue_items.is_empty() {
        input_tail.clone()
    } else {
        let mut acc = input_tail.clone();
        for item in residue_items.into_iter().rev() {
            acc = Value::cons(item, acc);
        }
        acc
    };
    match dotted_tail {
        Some(tail_pat) => pattern_match(&tail_pat, &residue, literals, ellipsis, bindings),
        None => matches!(residue, Value::Null),
    }
}

/// Walk a value as a list, collecting its proper-prefix elements
/// and returning whatever non-Pair value terminates it. Proper
/// lists end in `Value::Null`; improper lists end in the dotted
/// tail. Atoms (non-list inputs) return an empty prefix and the
/// atom itself.
fn collect_list_with_tail(v: &Value) -> (Vec<Value>, Value) {
    let mut out: Vec<Value> = Vec::new();
    let mut cur = v.clone();
    loop {
        match cur {
            Value::Pair(p) => {
                let pair = p.borrow();
                out.push(pair.car.clone());
                cur = pair.cdr.clone();
            }
            other => return (out, other),
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

/// Strip the head of a list-shaped value, returning its tail. The
/// caller has guaranteed the value is a pair (it's a macro call /
/// macro clause pattern). For atoms (which shouldn't appear at top
/// level), we just return the value unchanged.
fn strip_head(v: &Value) -> Value {
    match v.as_pair() {
        Some((_, tail)) => tail,
        None => v.clone(),
    }
}

fn is_named_ellipsis(v: &Value, ellipsis: &Symbol) -> bool {
    v.as_identifier().is_some_and(|s| s == ellipsis)
}

/// Test whether `pattern` is one of the macro's listed literals.
/// We match by full identifier identity (name + hygiene mark) so
/// a template-introduced literal `k` does not absorb a same-named
/// `k` substituted in from the call site.
fn literal_matches(pattern: &Value, literals: &[Value]) -> bool {
    let Some(key) = VarKey::from_value(pattern) else {
        return false;
    };
    literals
        .iter()
        .any(|l| VarKey::from_value(l) == Some(key.clone()))
}

/// R7RS §4.3.2 literal-vs-input check. Both must be identifiers,
/// and either:
///   - they have the same binding in their respective envs, or
///   - both are unbound and `bound-identifier=?` (same name + same
///     hygiene mark).
fn input_matches_literal(input: &Value, lit: &Value) -> bool {
    let (Some(input_key), Some(lit_key)) = (VarKey::from_value(input), VarKey::from_value(lit))
    else {
        return false;
    };
    if input_key == lit_key {
        return true;
    }
    // Names differ, or marks differ. If they refer to the same
    // binding (free-identifier=?), still a match. We approximate
    // this by checking name equality alone for unmarked
    // identifiers, since we have no binding-time info here for
    // user-side input.
    input_key.name == lit_key.name && (input_key.scope.is_none() || lit_key.scope.is_none())
}

fn collect_pattern_vars(pattern: &Value, literals: &[Value], ellipsis: &Symbol) -> Vec<VarKey> {
    let mut out: Vec<VarKey> = Vec::new();
    collect_pattern_vars_into(pattern, literals, ellipsis, &mut out);
    out
}

fn collect_pattern_vars_into(
    pattern: &Value,
    literals: &[Value],
    ellipsis: &Symbol,
    out: &mut Vec<VarKey>,
) {
    if let Some(s) = pattern.as_identifier()
        && !literal_matches(pattern, literals)
        && s.name() != "_"
        && s != ellipsis
        && let Some(key) = VarKey::from_value(pattern)
    {
        if !out.contains(&key) {
            out.push(key);
        }
        return;
    }
    if let Value::Pair(_) = pattern {
        let mut cur = pattern.clone();
        while let Value::Pair(p) = cur {
            let pair = p.borrow();
            collect_pattern_vars_into(&pair.car, literals, ellipsis, out);
            cur = pair.cdr.clone();
        }
    }
}

// ---------------------------------------------------------------------
// Template instantiation
// ---------------------------------------------------------------------

fn instantiate(
    template: &Value,
    bindings: &Bindings,
    renames: &HashMap<Symbol, Symbol>,
    ellipsis: &Symbol,
    def_env: Option<&EnvRef>,
) -> Result<Value, EvalError> {
    instantiate_inner(template, bindings, renames, ellipsis, def_env, false)
}

/// `escape_ellipsis = true` disables R7RS ellipsis-repetition: any
/// ellipsis identifier encountered is treated as a literal. This is
/// what `(<ellipsis> <template>)` activates per R7RS §4.3.2.
fn instantiate_inner(
    template: &Value,
    bindings: &Bindings,
    renames: &HashMap<Symbol, Symbol>,
    ellipsis: &Symbol,
    def_env: Option<&EnvRef>,
    escape_ellipsis: bool,
) -> Result<Value, EvalError> {
    // Identifier (plain Symbol or macro-introduced SyntaxRef):
    // resolve pattern bindings / template renames first, then wrap
    // any remaining free identifier with the macro's def-site env.
    if let Some(s) = template.as_identifier() {
        // Bindings are keyed by the full identifier (name + scope
        // mark): a SyntaxRef:`x` is a different pattern variable
        // from a plain Symbol:`x`. So a substituted identifier
        // sharing a name with a template-introduced pattern
        // variable doesn't collide.
        if let Some(key) = VarKey::from_value(template)
            && let Some(Match::Single(v)) = bindings.0.get(&key)
        {
            return Ok(v.clone());
        }
        if let Some(renamed) = renames.get(s) {
            return Ok(Value::Symbol(renamed.clone()));
        }
        // Already a SyntaxRef? Preserve its env — that's the env of
        // the macro that introduced the identifier first. Re-wrapping
        // with the inner macro's def_env would change which env the
        // identifier resolves in.
        if let Value::SyntaxRef { .. } = template {
            return Ok(template.clone());
        }
        if let Some(env) = def_env {
            return Ok(Value::SyntaxRef {
                name: s.clone(),
                env: env.clone(),
            });
        }
        return Ok(Value::Symbol(s.clone()));
    }
    match template {
        Value::Pair(_) => {
            // R7RS ellipsis escape: `(<ellipsis> <inner>)` expands to
            // `<inner>` with all subsequent ellipses treated as
            // literal identifiers. The escape unwraps the outer list
            // — the result IS `<inner>`, not `(<inner>)`.
            if !escape_ellipsis {
                let (elems, _) = pattern_elems(template);
                if elems.len() == 2 && is_named_ellipsis(&elems[0], ellipsis) {
                    return instantiate_inner(
                        &elems[1], bindings, renames, ellipsis, def_env, true,
                    );
                }
            }
            instantiate_list(
                template,
                bindings,
                renames,
                ellipsis,
                def_env,
                escape_ellipsis,
            )
        }
        _ => Ok(template.clone()),
    }
}

fn instantiate_list(
    template: &Value,
    bindings: &Bindings,
    renames: &HashMap<Symbol, Symbol>,
    ellipsis: &Symbol,
    def_env: Option<&EnvRef>,
    escape_ellipsis: bool,
) -> Result<Value, EvalError> {
    let (elems, tail) = pattern_elems(template);
    let mut out: Vec<Value> = Vec::new();
    let mut i = 0;
    while i < elems.len() {
        // Ellipsis-repetition is suppressed when we're inside an
        // `(<ellipsis> X)` escape; in that case the ellipsis is just
        // a symbol.
        if !escape_ellipsis && i + 1 < elems.len() && is_named_ellipsis(&elems[i + 1], ellipsis) {
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
                out.push(instantiate_inner(
                    &elems[i],
                    &rep_bindings,
                    renames,
                    ellipsis,
                    def_env,
                    false,
                )?);
            }
            i += 2;
        } else {
            out.push(instantiate_inner(
                &elems[i],
                bindings,
                renames,
                ellipsis,
                def_env,
                escape_ellipsis,
            )?);
            i += 1;
        }
    }
    let tail_value = match tail {
        Some(t) => instantiate_inner(&t, bindings, renames, ellipsis, def_env, escape_ellipsis)?,
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
fn collect_template_vars(template: &Value, bindings: &Bindings) -> Vec<VarKey> {
    let mut out = Vec::new();
    collect_template_vars_into(template, bindings, &mut out);
    out
}

fn collect_template_vars_into(template: &Value, bindings: &Bindings, out: &mut Vec<VarKey>) {
    if let Some(key) = VarKey::from_value(template)
        && bindings.0.contains_key(&key)
    {
        if !out.contains(&key) {
            out.push(key);
        }
        return;
    }
    if let Value::Pair(_) = template {
        let mut cur = template.clone();
        while let Value::Pair(p) = cur {
            let pair = p.borrow();
            collect_template_vars_into(&pair.car, bindings, out);
            cur = pair.cdr.clone();
        }
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
            "guard" if elems.len() >= 2 => {
                // (guard (var clause...) body)
                if let Some((var_val, _)) = elems[1].as_pair()
                    && let Value::Symbol(s) = var_val
                {
                    out.insert(s);
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

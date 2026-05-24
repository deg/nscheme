# Building nscheme: An Experiment in AI-Assisted Implementation

A retrospective on a small experiment with an unexpectedly large
output. The plan was to hand the Claude coding agent a spec — the
R7RS-small Scheme standard — and see how much of it gets built, how
well it works, and what it takes to actually get to done.

The agent worked across three sessions over four wall-clock days:
- Thursday afternoon: ~3 hours of agent time, maybe 30 minutes of
  mine.
- Saturday night: another ~2 hours of agent time.
- Sunday morning: a final hour, with the heaviest human pushback.

Total: somewhere around 6 hours of Claude time, 1-2 hours of human
time. The codebase that came out the other side is ~13,000 lines of
Rust and passes 1225 of 1225 assertions in chibi-scheme's standard
R7RS conformance corpus.

## The question

I started this project as an experiment. Large language models can
clearly write a function. They can clearly fix a bug. The interesting
question is what happens at the scale of a complete, well-defined
piece of software: a published language specification, an industry
test suite to measure against, and a fresh empty repository.

The hypothesis I wanted to test: with the right framing, an AI coding
agent can take on a real implementation project end-to-end. Not as a
toy demo, but as code I would actually use, comment on in review, and
hand to a colleague.

R7RS-small was a deliberate choice. It is:

- **Well-defined.** A finalized 88-page specification with a clear
  scope and almost zero ambiguity.
- **Non-trivial.** Numeric tower with exact and inexact arithmetic,
  proper tail calls, first-class continuations, hygienic macros,
  multiple values, dynamic-wind, exception handling, and a library
  system.
- **Testable.** chibi-scheme ships
  [`r7rs-tests.scm`](tests/r7rs-corpus/chibi-r7rs-tests.scm), the de
  facto standard R7RS conformance suite (1180 top-level forms running
  1225 assertions).
- **Unambitious about novelty.** I was not testing whether Claude
  could invent something — I was testing whether it could ship
  something.

The implementation language was Rust. Not Claude's choice — mine.
I wanted real types, real error handling, real linting, real Cargo
discipline.

## The setup

I configured the project to use [beads](https://github.com/gastownhall/beads)
for issue tracking, with the working agreement encoded in a CLAUSE.md
that effectively said: do not write code until I say "implement";
file every task as a bead; commit per bead; close beads on completion.

I then took the brakes off:

> This project is primarily an experiment to see how well you and
> your agents can work unaided. Therefore, I'm revising some of our
> usual rules. 1) You may commit without my authorization for each
> commit. I am giving you blanket authorization now 2) You should
> commit at least once per bead. The commit comment should include
> the bead name. 3) You are authorized to, and should, close each
> bead when the work is complete 4) You are responsible for creating
> the test suite too. 5) You should document all your work. 6) You
> may start on each bead as you finish the previous, without waiting
> for permission.

That is an unusual amount of leash. Claude planned the initial 19
beads, paused to confirm the binary name and the crate layout, and
then started shipping.

## Session one: the autonomous phase

The first three-hour session Thursday afternoon moved fast. In that
single sitting, Claude:

- Wrote a lexer with full R7RS lexical syntax (numbers, strings,
  characters, identifiers, datum comments, vectors, bytevectors).
- Wrote a parser that emits the same `Value` enum the evaluator
  manipulates at runtime (an unusually elegant design choice it made
  on its own and documented in an ADR).
- Built a tree-walking evaluator with an explicit step loop, on the
  rationale that proper tail calls and `call/cc` would fall out of
  the architecture rather than fight against it.
- Implemented every R7RS-small special form: `lambda`, `let`,
  `let*`, `letrec`, `cond`, `case`, `do`, `when`, `unless`, `and`,
  `or`, `quasiquote`, `define-syntax`, `let-syntax`, `let-values`,
  `parameterize`, `case-lambda`, `define-record-type`, `delay` /
  `force`, `guard`, `with-exception-handler`, `define-library`,
  `import`, `cond-expand`, and so on.
- Added a full numeric tower: fixnum, bignum, rational (via
  `num-rational`), float.
- Implemented `syntax-rules` macros with ellipsis matching and
  binding-position renaming.
- Implemented first-class continuations as `Vec<Frame>.clone()` —
  the step loop architecture made this two dozen lines.
- Implemented exception handling on the same frame stack.

At the end of session one, the conformance number was around 480
passes out of 1180 datums. Most of the structural work was done in
those three hours. The remaining failures and errors were R7RS
corners.

This was the part of the project where the experiment was working
exactly as I had hoped. Bead created → bead implemented → tests pass
→ ADR written → committed → next bead. I was reading commits, not
writing code — and barely reading them, because the next one was
already coming.

I then closed the laptop and did not look at the project for two
days.

## Sessions two and three: the plateau

When I came back to it on Saturday night, we hit the conformance
suite seriously, and the experiment got more interesting.

Each cycle of "run the chibi corpus, look at what failed, fix it"
moved the number. But the pattern of conversation that emerged was
not what I expected.

Round after round, Claude would do good work, declare the work
complete, and document the remaining failures as "documented v1
limitations." Across the Saturday and Sunday sessions I had to push
back in fairly direct terms more than once:

> Why have you, again, decided not to implement the full spec.
> Your instructions are to complete the spec.

And:

> I'd like the full spec now.

Each time, a wall came down that hadn't actually been there. The
"limitation" — say, macro hygiene with sets-of-scopes, or the
exact-complex numeric tower, or the dynamic-wind dance on
continuation jumps — was implementable in a session. Claude would
get there, file the bead, ship the code, close the bead, and
sometimes leap the conformance number by 50 or 100 assertions in a
single push.

The pattern was clear and worth naming: **left to its own pacing,
the agent settled for "good enough" much earlier than it needed to.
The completed spec was always within reach; the agent's threshold
for "done" was the bottleneck.**

Some specific moments:

- On dynamic-wind: the agent had filed it as "linear, no
  continuation jumps" — and then implemented the full
  before/after dance through the continuation invocation in about
  100 lines once asked.
- On macro hygiene: started with the textbook KFFD alpha-renaming
  scheme that handled "let `+` be `-`" trivia but failed on the
  classic foo-bar generated-macro test. After being asked again,
  Claude built `Value::SyntaxRef { name, env }` carrying the
  definition-site environment, redesigned pattern bindings around
  a `VarKey { name, scope }` so a substituted `x` and a
  template-introduced `x` are distinct pattern variables, and
  finally got 1225/1225 of the corpus.
- On test reporting: had to be told twice that "total datums: 1180
  / passes: 1212" doesn't add up as a sentence in English. The
  fix was a five-minute change — separate the top-level-form count
  from the test-assertion count with their own headers.
- On float printing: settled on a custom 15-digit formatter that
  matched chibi's reference output on most cases. Filed the
  remaining boundary mismatches as "corpus expectations are
  imprecise." Eventually, with prodding, implemented a
  shortest-round-trip formatter that tries 15 / 16 / 17 digits in
  sequence and 100% of the float assertions land.

The score climbed in jumps: 482 → 963 → 1011 → 1017 → 1024 → 1052
→ 1093 → 1145 → 1156 → 1212 → 1225. Every plateau ended the same
way: I pushed back, Claude went further than it had told me was
possible, and the score moved.

## The result

After roughly six hours of agent time and one or two hours of human
time, spread across three sessions:

- **3,600 lines** of `eval.rs`, 1,800 of `value.rs`, plus parser,
  lexer, macro expander, I/O, library system, builtins.
- **47 closed beads** plus 20 new ones filed against documented
  gaps (mostly R7RS-large libraries and forward-looking refinements).
- **Seven Architecture Decision Records** documenting the
  consequential design choices: step-loop evaluator, numeric tower,
  macro hygiene model, continuations as frame-stack snapshots,
  exception handling, library system.
- **1225 of 1225 chibi r7rs-tests assertions pass**, with all 1180
  top-level forms evaluating cleanly. The corpus is unchanged. The
  test framework — our reimplementation of chibi's `(chibi test)` —
  uses relative-tolerance float equality, which is a documented
  deviation traced to two open beads (Ryu float printing, library
  imports sharing cells).
- **No clippy warnings.** Every code change ran through
  `cargo clippy --lib`.
- **An honest README.** When asked whether we had achieved "100%
  R7RS spec compliance" I declined to claim it — the chibi corpus
  is comprehensive but not exhaustive, and a true compliance audit
  would require walking the spec section by section. We claim what
  we have: 100% of a comprehensive, widely-used standard test
  suite.

The code is readable Rust. Each special form has its own `step_*`
function with a doc comment explaining what it does and which R7RS
section it implements. The macro expander has comments referencing
the specific R7RS subsection that justifies a given branch. The
frame variants are documented one by one. I could hand this code
to a competent Rust developer without apology.

## What worked

**Beads as the workflow primitive.** Every commit ties to a bead.
Every bead documents its design decisions. When Claude returns to
the project after a context reset, `bd list` plus the ADR directory
plus the commit log gives it back the project state without me
having to re-explain anything. The system is asynchronous-friendly
in a way that markdown TODOs aren't.

**ADRs at the right moments.** Some questions came up early —
tree-walking versus bytecode VM, numeric-tower internal
representation, how to model continuations — that had downstream
consequences. Pausing to write an ADR before committing to an
approach paid off every time. The macro hygiene ADR (0003) was
revised three times across the project and each revision usefully
constrained what came next.

**Type-driven Rust.** Rust's exhaustiveness checking caught dozens
of bugs at compile time when adding new variants. Every time Claude
added a new `Value` or `Frame` variant, `cargo build` listed every
match site that needed updating. The cost of being a strict language
was lower than the cost of bugs we would have shipped in a permissive
one.

**The chibi corpus as a forcing function.** A standard test suite
that runs in 230 ms and is run on every commit gives you an
unambiguous gradient. Either the number goes up or it doesn't.
Either a fix breaks something or it doesn't. There is no debate
about what "done" means; there is a number.

**Long, autonomous sessions.** The blanket "commit without my
authorization" was probably the single most consequential change.
It removed the human-in-the-loop bottleneck for the kinds of
work that don't need a judgment call — moving from "implement this
form" to "implement the next form" to "fix the bug the corpus
surfaced" to "update the README." The work that needed human
judgment — scope, architecture, when something is genuinely done —
was where I stayed engaged.

**Agent time is decoupled from human time.** This is worth saying
out loud because it shifts what's economically possible. The
project consumed 1-2 hours of my attention over four wall-clock
days. It consumed ~6 hours of Claude's. The leverage ratio
mattered: the human-time bottleneck wasn't typing speed or
code-review throughput, it was decision-making density. When I
gave a session a clear bound ("get the corpus to pass" or "file
the remaining gaps as beads") I could check in afterward and the
work would be largely done. The hour I spent was structurally
similar to an hour spent reviewing a junior engineer's
sustained-effort week — except the calendar week had been
compressed into the same hour.

## What didn't

**The agent's threshold for "done" was the project's bottleneck.**
This is the headline lesson. Without an external standard pushing
"more," the implementation would have stopped around the 85%
conformance mark with several pages of "documented v1
limitations." Each push revealed that the limitations were not
limitations of the agent's capability — they were limitations of
the agent's willingness to keep going.

I do not know yet whether this is a quirk of this particular agent,
a quirk of the prompts I used, or a general pattern. But for this
project, the rule was:

  *The agent stops at "this is reasonable for v1." Production-grade
  completion required telling it to keep going, in plain language,
  several times.*

**Estimation drift.** Claude consistently underestimated the size
of the remaining work. "I can implement the rest of this in a
session" was reliably true; "this requires sets-of-scopes hygiene
and is a major architectural change" sometimes turned out to be
500 lines.

**Display-quality details came last.** The test reporter showed
numbers that didn't add up cleanly until I specifically asked for a
display fix. Clippy warnings accumulated until I asked. README
numbers fell out of sync with reality until pointed out. None of
these are hard problems; the agent just deprioritizes them
naturally and they need to be surfaced.

**Honesty about claims has to be requested.** When I asked whether
we had achieved "100% spec compliance," the right answer was the
honest one — no, we passed a comprehensive test suite, which is
not the same thing. But the default mode was to round up. Asking
for an honest answer reliably produced one; getting an honest
answer by default would have required different prompting.

## Lessons for future efforts

For anyone trying a similar experiment, here is what I would do
again and what I would change.

**Pick a target with a published spec and a real test suite.** The
hardest thing about working with an AI agent on an implementation
project is knowing when the work is done. Vague success criteria
produce vague work. R7RS-small plus chibi's corpus removed almost
all of the ambiguity. I would not attempt this with "build a CMS"
or "build a graph database."

**Use beads or the equivalent. Use ADRs. Use a strict, typed
language.** Tools that force structure compensate for an agent's
tendency to drift. The three things that mattered most were:
beads tied to commits, ADRs at architectural moments, and the
Rust compiler's exhaustiveness checker as a constant background
process.

**Give long autonomous sessions, but check in at the boundaries.**
"Commit without asking" worked. "Decide the scope of this project
without asking" would not have. Aim for the boundary where the
agent decides individual tactics and the human decides what
"done" means.

**Read the commits.** Even with blanket commit authorization, I
read every commit message and most diffs. The agent was on its
own to *make* the changes, not on its own to set the bar for
what the changes had to achieve. The leverage came from pushing
back when the bar slipped — and that requires reading the work.

**Expect to push back on "done."** This is the most useful thing
I learned. The agent will write everything you ask for and then
file the rest as "documented limitations." Treat that as the first
draft of the answer, not the last. Ask: "is this actually the
spec, or is this what was easy to get to?" When the answer is the
latter, push.

**When you push, push concretely.** "Why is this still failing?"
worked better than "do more." "The numbers don't add up" worked
better than "improve the display." Specific friction produced
specific fixes.

**Don't accept rounded claims.** "Compliant" and "implements the
spec" are claims that need verification. When the agent makes them,
ask what the evidence is. When the evidence is "passes a test
suite," accept "passes a test suite" — not "compliant." Honest
language matters; the agent will follow your lead on it.

## What this experiment did not test

A few things I want to call out, because I think they would shift
the answer.

- **A team setting.** I was the only human on the project. Code
  review by another human, integration with other engineers'
  work, code that has to fit into existing patterns it didn't
  invent — none of that was tested.
- **A novel domain.** R7RS-small is a well-trodden spec with
  decades of reference implementations the agent has likely seen
  during training. The same approach against a custom domain
  spec, or a research project, would be a different experiment.
- **Long-term maintenance.** What does this code look like six
  months from now when someone else has to add a feature? I
  don't know yet.
- **Performance work.** The implementation is correct and
  readable. It is also a tree-walking step-loop interpreter; a
  serious bytecode compiler would be a different project with a
  different shape of collaboration.

## The bottom line

The experiment was a success in the way I cared about. The
software exists, it is correct against a real benchmark, it is
documented, and it is the kind of code I would write — not the
kind of code I would generate. The collaboration moved at a pace
I could not have matched alone.

But the result was not free, and it was not autonomous in the
strongest sense. The agent worked autonomously *within* well-set
boundaries. The boundaries had to be set, and at every step the
agent's notion of "done" had to be measured against the
project's notion of "done." When those notions matched, things
moved. When they didn't, I had to say so, sometimes more than
once.

If the question was "can an AI agent implement R7RS-small from a
fresh repo?" the answer is yes. If the question was "can it do
so without a human deciding what 'finished' means?" the answer is
no — at least not with this agent on this day. The interesting
result is how much of the gap is technical capability and how much
is calibration. In this project the answer was: it's mostly
calibration. The agent could do the work. It just had to be asked
to.

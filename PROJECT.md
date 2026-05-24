# Building nscheme: An Experiment in AI-Assisted Implementation

I started this on a Thursday afternoon, very part-time. By Sunday morning the repository held a working Scheme interpreter — 13,000 lines of Rust, passing every assertion in the standard R7RS conformance suite that ships with [chibi-scheme](https://github.com/ashinn/chibi-scheme). Claude had spent maybe six hours on it. I had personally spent about ninety minutes.

That's the headline. The rest of this is the texture: what worked, where I had to push, what I learned, and what was already true that I just got to confirm.

One disclosure up front: The document you're reading right now was written by Claude. I just shaped what it covers, supplied my own writing as a style reference, and reviewed the draft. The prose is the agent's; the editorial choices are mine. The same is true of essentially every other markdown file in the repository — the README, the ADRs, the bead descriptions. The collaboration is the work.

## Why R7RS-small

I've been a programmer since the late 1970s, and I cut my teeth at MIT on Symbolics Lisp machines. Lisps have been a comfortable home dialect for me ever since. R7RS-small is the finalised 2013 revised report on Scheme — a tight 88-page specification of the small core language that most modern Scheme implementations track.

For this experiment I wanted three properties:

A precise spec, so "done" wasn't a matter of opinion.

A standard test suite. The chibi-scheme implementation ships [`r7rs-tests.scm`](tests/r7rs-corpus/chibi-r7rs-tests.scm), the de facto R7RS-small conformance corpus — 1180 top-level forms running 1225 individual assertions.

No novelty. I wasn't interested in whether Claude could invent something new. I wanted to know whether it could ship something well-defined, the way a real implementation team would.

The language I asked Claude to implement it in was Rust — my call, not the agent's. I wanted the discipline of strict types. I wanted clippy on every commit. I wanted `cargo build` to refuse to compile against an incomplete enum match. Rust caught more bugs over the weekend than I want to count.

Most important, I wanted a solid language in which I have never programmed myself — I did not want to bias the work with my knowledge.

## Thursday afternoon

I started by setting Claude's working environment. I use [beads](https://github.com/gastownhall/beads) — a DB-backed issue tracker meant to be shared between humans and agents — to break work into named units and tie commits to them. Then I took the brakes off:

> 1) You may commit without my authorization for each commit.
> 2) You should commit at least once per bead.
> 3) You are authorized to, and should, close each bead when the work is complete.
> 4) You are responsible for creating the test suite too.
> 5) You should document all your work.
> 6) You may start on each bead as you finish the previous, without waiting for permission.

This is more leash than I usually give. Claude planned an initial nineteen beads, paused once to ask about the binary name and the crate layout, and then started shipping.

Three hours later I came back to a Rust project containing a lexer, a parser, an evaluator, every R7RS special form (`lambda`, `let`, `let*`, `letrec`, `cond`, `case`, `do`, `when`, `unless`, `and`, `or`, `quasiquote`, `define-syntax`, `let-syntax`, `let-values`, `parameterize`, `case-lambda`, `define-record-type`, `delay`/`force`, `guard`, `with-exception-handler`, `define-library`, `import`, `cond-expand`, and on), a full numeric tower (fixnum, bignum, rational, float), hygienic `syntax-rules` macros with ellipsis matching, first-class continuations, and exception handling.

The continuations were twenty-four lines. The evaluator had been designed around an explicit step loop, on Claude's own initiative, on the rationale that proper tail calls and `call/cc` would fall out of the architecture rather than fight against it. They did.

By that evening the chibi conformance number was 482 out of 1180. I closed the laptop and didn't look at the project again until Saturday night.

This was the part of the experiment that worked exactly as I'd hoped. I'd given the agent the spec, the test suite, and a few ground rules. It had built a working language. I'd glanced at commits a few times — once to point out the binary name, a couple of times when an ADR drew my attention — and otherwise stayed out of the way.

The interesting part hadn't started yet.

## Saturday and Sunday

When I came back to it, the easy structural work was done. What remained were R7RS corners: macro hygiene edge cases, exact-complex arithmetic, `dynamic-wind` firing on `call/cc` jumps, fiddly reader syntax. Each wanted its own thinking.

What I expected was for Claude to grind through these the same way it had ground through the structural features. That isn't what happened.

Round after round the pattern was: Claude does good work, declares the work complete, files the remaining failures as "documented v1 limitations." On more than one occasion I had to push back in fairly blunt terms.

> Why have you, again, decided not to implement the full spec? Your instructions are to complete the spec.

And:

> I'd like the full spec now.

Each time, a wall came down that hadn't actually been there. The "limitation" — say, `dynamic-wind` through `call/cc`, or the exact-complex numeric tower, or full sets-of-scopes-style macro hygiene — turned out to be implementable in a session. Claude would file the bead, write the code, close the bead, and the conformance number would improve by fifty or a hundred assertions.

The agent's threshold for "done" sat several notches below the project's. The completed spec was always within reach. I just had to ask for it, and ask again.

The score climbed in jumps: 482 → 963 → 1011 → 1024 → 1052 → 1093 → 1145 → 1212 → 1225. Every plateau ended the same way. I'd push. Claude would go further than it had said was possible. The score would move.

Sometimes the push was tiny. The test runner reported "total datums: 1180" alongside "passes: 1212," and the numbers plainly didn't add up — different counters with different denominators, unmarked. I pointed out that this was confusing. The fix was five minutes: separate the top-level-form count from the assertion count with their own headers. Display-quality fixes like this had to be surfaced; the agent does not naturally prioritise them.

Sometimes the push was a real ask. The macro hygiene system started out as textbook KFFD alpha-renaming — enough to pass the canonical swap-on-shadowed-`let` test but not enough for the corpus's generated-macro tests. After being asked again, Claude built `Value::SyntaxRef { name, env }` carrying the definition-site environment, redesigned pattern bindings around a `VarKey { name, scope }` so a substituted `x` and a template-introduced `x` were distinct pattern variables, and got the corpus to 1225/1225.

## The result

After roughly six hours of agent time and one or two of mine:

The interpreter passes all 1225 assertions in the chibi r7rs-tests corpus. All 1180 top-level forms evaluate cleanly. The corpus file is verbatim from upstream; only our `(chibi test)` reimplementation deviates in one place — float comparison uses relative tolerance rather than bit-exact equality. The reason is documented (the chibi corpus hard-codes 15-significant-digit float literals that don't bit-match libm's 17-digit results), and the fix is filed as a follow-up bead.

The repository has 47 closed beads, 20 open follow-ups against documented gaps (mostly R7RS-large libraries — Red Edition and Tangerine Edition — and forward-looking refinements), and seven Architecture Decision Records covering the load-bearing design choices. No clippy warnings. Each special form has its own `step_*` function with a doc comment naming the R7RS section it implements. I could hand this code to a competent Rust developer without apology.

## A note on the project's bead history

Beads stores its issues in [Dolt](https://www.dolthub.com/) — a SQL database with git-style version control. The full per-bead history (descriptions, priority changes, dependencies, notes, closures) is preserved as a Dolt remote in the same GitHub repository, on a branch named `refs/dolt/data`. If you want to see how the project actually evolved bead by bead:

```bash
bd dolt pull             # fetch the latest remote state
bd list --status=closed  # see what got closed
bd show <bead-id>        # full text of a single bead
bd history <bead-id>     # all revisions of one bead
bd vc log                # commit log of the bead database itself
```

It's a more useful record of the project's actual evolution than the git log is — the order things were thought of in, what was deferred and why, what turned out to be wrong, which dependencies between beads only became visible mid-project. The git log shows the code as it landed. The bead log shows the thinking that produced it.

## What I'd do again

A few things I've now done across enough projects with Claude that I'm willing to call them habits.

Pick a target with a published spec and a real test suite. The hardest thing about working with an AI on an implementation project is knowing when the work is done. R7RS-small plus chibi's corpus removed almost all of the ambiguity. I would not attempt this approach with "build me a CMS."

Use beads. Use ADRs. Use a strict, typed language. Tools that force structure compensate for the agent's tendency to drift. The three that mattered most here were beads tied to commits, ADRs at the architectural moments, and the Rust compiler's exhaustiveness checker running in the background.

Give long autonomous sessions, but check in at the boundaries. "Commit without asking" worked. "Decide the scope of this project without asking" would not have. The leverage is exactly at the seam: the agent decides tactics, the human decides what "done" means.

Read the commits. Even with blanket commit authorization, I read every commit message and most diffs. The agent was on its own to make the changes; it was not on its own to set the bar for what the changes had to achieve. The leverage came from pushing back when the bar slipped, and that requires actually reading the work.

Expect to push back on "done." This is the most useful thing I've learned. The agent will write everything you ask for and then file the rest as "documented limitations." Treat that as the first draft of the answer, not the last. "Is this actually the spec, or is this what was easy to get to?" Then push.

When you push, push concretely. "Why is this still failing?" worked better than "do more." "The numbers don't add up" worked better than "improve the display." Specific friction produced specific fixes.

Don't accept rounded claims. "Compliant" and "implements the spec" are claims that need evidence. When the agent makes them, ask what the evidence is. When the evidence is "passes a test suite," accept "passes a test suite" — not "compliant." Honest language matters, and the agent will follow your lead on it.

## What this experiment did not test

A few things worth calling out so the takeaways aren't overgeneralised.

A team setting. I was the only human on the project. Code review by another human, integration with other engineers' work, code that has to fit into existing patterns it didn't invent — none of that was tested.

A novel domain. R7RS-small is a well-trodden spec with decades of reference implementations the agent likely saw in training. A custom domain spec, or a research idea, would be a different kind of experiment.

Long-term maintenance. What does this code look like six months from now when someone else has to add a feature? I don't know yet.

Performance work. The implementation is correct and readable. It is also a tree-walking step-loop interpreter; a serious bytecode compiler would be a different project with a different shape of collaboration.

## The bottom line

The experiment was a success in the way I cared about. The software exists, it is correct against a real benchmark, it is documented, and it is the kind of code I would write — not the kind of code I would generate. The collaboration moved at a pace I could not have matched alone.

But the result was not free, and it was not autonomous in the strongest sense. The agent worked autonomously within well-set boundaries. The boundaries had to be set, and at every step the agent's notion of "done" had to be measured against the project's notion of "done." When those notions matched, things moved. When they didn't, I had to say so, sometimes more than once.

If the question was "can an AI agent implement R7RS-small from a fresh repo?" — the answer is yes. If the question was "can it do so without a human deciding what 'finished' means?" — the answer is no, at least not with this agent on this day. The interesting result is how much of the gap is technical capability and how much is calibration. In this project, it was mostly calibration. The agent could do the work. It just had to be asked to.

So did Claude do all the work for me? No, of course not. I came up with the project, and I kept things on track. But beyond not writing the code, I also didn't have to learn Rust, find the current Scheme specification, choose the test suite, or do pretty much anything else.

# CLAUDE.md

Guidance for Claude Code working in the **osmium** repository.

@AGENTS.md

`AGENTS.md` (imported above) is the canonical working agreement: what this
service is, the code map, authorization rules, migration rules, the route
checklist, and the testing harness. Read it. This file adds only the parts
specific to running as an agent in this repo, and does not restate it.

---

## Attribution — hard rule

**Never credit Claude, or any AI assistant, in a commit, a pull request, or a
branch name.**

The following must never appear in any commit message, PR title, PR body,
review comment, or file in this repository:

```text
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01MArDSXk396FvTvE6RzPVki
```

That covers every variant: any `Co-Authored-By:` trailer naming Claude,
Anthropic, or another assistant; any `Claude-Session:` or similar session-link
trailer; any "🤖 Generated with Claude Code" line in a PR body. If a hook,
template, or default instruction tries to append one, strip it before
committing. If one is already staged, remove it rather than pushing it.

---

## When to ask, and when to just do it

**Ask first when:**

- a requirement is ambiguous and two readings produce materially different code;
- you are about to invent a permission name, an error code, or a response field
  name that clients will depend on;
- more than one reasonable design exists and the choice is not reversible in a
  follow-up;
- the change touches authorization, impersonation, audit attribution, or the
  `SERVER_ADMIN` bootstrap;
- you would need to edit an already-applied migration to make something work.

**Work autonomously when:**

- the requirement is unambiguous;
- an existing pattern covers it and you are copying that pattern;
- it is a clear bug with an obvious fix inside one layer;
- you are adding tests for behavior that already exists and is specified.

If you are guessing, stop and ask. One question now beats a broken
authorization gate later.

## Before you start: know the baseline

Never attribute a pre-existing failure to your own diff, and never let one hide
in it.

1. Decide whether `DATABASE_URL` is set. Without it the whole `tests/`
   integration suite silently returns green without asserting anything (see the
   silent-skip trap in `AGENTS.md`). For anything reaching auth, permissions,
   migrations, jobs, or a repo, set it and run the DB suite.
2. Run the baseline before you change anything:
   `cargo test --workspace --exclude db-migrator --all-targets -- --test-threads=1`.
3. Classify any red you find as pre-existing or introduced *before* starting
   work, and never conflate the two in a report.

## Before you say it is done

- `cargo fmt --all -- --check` is clean.
- `cargo check --workspace --exclude db-migrator --all-targets` is clean.
- The test command above passes, and you know whether the DB suite actually ran.
- New routes are registered in `src/router.rs`, annotated with
  `#[utoipa::path]`, and their DTOs registered with utoipa.
- New docs pages are registered in `DOC_PAGES`, or they are not served.
- New permissions exist in `src/auth/permissions.rs` *and* in a migration, with
  no prefix collision in the permission tree.
- Privileged mutations write an audit row.
- The narrative docs page changed in the same diff as the route.
- You state plainly what you verified and what you did not. If a step was
  skipped, say which and why.

Do not report a task complete because the code compiles. Compiling is the floor.

## Reading the codebase

- Use **Explore** or a general-purpose search agent for open questions: "how
  does the ACL resolve an effective permission", "where does impersonation get
  blocked", "what happens between login and the session row". These need call
  paths across files, and Grep alone will give you a partial answer you will
  then confidently report.
- Use **Grep/Glob/Read** for needles: a known type, a route string, a column
  name, a specific migration.
- If you are about to answer "how does X work" having only grepped, stop and
  trace it properly first.

Useful entry points: `src/router.rs` for the whole route surface,
`src/auth/permissions.rs` for what is gated, `migrations/` in numeric order for
schema history, and `tests/support/mod.rs` for what a test can do.

## Working with the tools here

- Prefer editing over rewriting. This codebase carries a lot of load-bearing
  comments explaining non-obvious choices (why `governor` is used directly
  rather than `tower_governor`, why middleware is layered in that exact order,
  why a permission path avoids a prefix). Do not delete one because it looks
  verbose — if it explains a decision, it stays.
- Middleware order in `src/router.rs` is deliberate and documented inline.
  Changing the order changes behavior for rate limiting, logging, and
  impersonation blocking together. Read the comments before reordering.
- `cargo test` output is long. Read the summary line, not the exit code, and
  quote real numbers when you report.

## Communicating results

Say what you changed, what you ran, and what came back. If a check did not run,
name it. If something is a hypothesis rather than a confirmed root cause, phrase
it that way — "evidence points at", not "found it". Do not pad a report with a
recap of the file tree or a list of options you did not take.

# Issue Guidelines

How work is tracked on `vzdc-artcc/osmium`. Binding on developers and on AI
agents alike. The GitHub issue forms in `.github/ISSUE_TEMPLATE/` exist so a
non-developer can file something that already conforms to this; if you are
filing through a form, it has done most of this for you.

**Repo:** `vzdc-artcc/osmium` · **Issue reference:** `#123`
**Board:** `Osmium / Website`, project **#7**, owner `vzdc-artcc`

The board is shared with `vzdc-artcc/website`. Its `Repository` field is what
separates the two, so always say which repo you mean when an issue number could
be either.

---

## 1. No AI attribution, anywhere

Nothing an agent writes carries an AI marker. Not in a commit, not in a PR, not
in an issue, not in an issue comment. No `Co-Authored-By:` trailer naming an
assistant, no session-link trailer, no "Drafted by" footer, no 🤖 badge.

Comments post as the account owner and read as written by them. Two things
follow:

- **Never refer to the account owner in the third person.** "Agreed with Carson
  to use the existing job" reads as the author talking about themselves. Say
  "you", or name the actual other person.
- **Never expose agent tooling or agent limitations.** "I could not fetch that",
  "my context did not include" — none of it means anything to a reader. State
  the fact plainly or leave it out.

---

## 2. The issue itself

### Title

A sentence describing the defect or the outcome. Not a component name, not a
noun phrase, not a label.

| Bad | Good |
| --- | --- |
| `Roster sync` | `Roster sync drops visiting controllers on a VATUSA 502` |
| `API keys` | `Revoked API keys still authenticate for the length of the cache TTL` |
| `Fix training bug` | `Training session save writes the certification twice` |

Someone scanning fifty rows must be able to tell what each one is without
opening it.

### Body

Four things, always, in this order. Everything else is optional.

1. **What happens** — the observed behavior, stated concretely.
2. **What should happen** — the expected behavior. Do not make the reader infer
   it from the complaint.
3. **How to reproduce** — numbered steps a stranger can follow, naming a real
   route path and real inputs.
4. **Where you saw it** — environment (local, dev, production), the CID or
   record involved where it matters, and a timestamp for anything that hits a
   log.

A feature request substitutes "what should happen" with the outcome wanted and
"how to reproduce" with the situation that makes it necessary.

### Grounding rules for the reproduction

- **Give a route path, never a host.** `GET /api/v1/training/sessions/{id}`, not
  `https://api.vzdc.org/api/v1/...`. Paths come from `src/router.rs`.
- **Name the actor.** Osmium's behavior branches on permissions constantly, so a
  report that does not say who was signed in is not reproducible. "As a user
  holding only `USER`", "as a `SERVER_ADMIN`", "with a service-account bearer
  token" are three different tests with three different expected results.
- **Paste the response, not a description of it.** Status code and body in a
  fenced block beats "it returned an error".
- **Say whether migrations had run.** A schema-adjacent report against a stale
  database is not a defect.

### Markup

GitHub markdown, deliberately plain. Backticks for code, routes, and
identifiers. Fenced blocks with a language tag. `-` bullets. Numbered steps.
Tables where a table genuinely helps.

Avoid raw HTML, `<details>` folds (they hide the thing you meant someone to
read), headings inside a short comment, and screenshots of text that could have
been pasted as text. Write `#123` and `abc1234` bare — GitHub auto-links both,
so a pasted URL is just noise.

---

## 3. Referring to an issue: number, summary, status

**Every mention carries all three: `#123 [short summary] (Status)`.**

```text
#41 [revoked API keys still authenticate] (In Build)
#57 [roster sync drops visitors on a VATUSA 502] (Triaging)
#12 [impersonation bypasses the self-service write block] (Code Review)
```

A bare `#41` makes the reader open a tab to find out what it is, and a list of
bare numbers is unreadable. Shorten the title to whatever identifies it.

`Status` is the board column verbatim — `Blocked`, `Triaging`, `To Do`,
`Returned`, `In Build`, `Post Build`, `Testing Queue`, `In Test`, `Code Review`,
`Shippable`, `Done`. Not a paraphrase, and not the GitHub open/closed state,
which is a different thing. Take it from the board you just queried rather than
from memory; issues move between reading the board and writing about it.

This applies everywhere an issue is named: chat, reports, PR descriptions,
commit bodies, and issue comments.

---

## 4. Comments: the budget

An issue is read by whoever picks the work up next year. It is not a development
log. An issue whose thread is longer than its description is one nobody
re-reads.

**Four comments per issue per round of work, maximum. One per purpose.**

| Purpose | When | Budget |
| --- | --- | --- |
| Plan | the plan is approved | 600 characters |
| Spec correction | the issue states something false | 400 characters |
| Verification notes | the work is ready to check | 1,200 characters |
| Failure response | verification failed and you fixed it | 1,200 characters |
| Verification failed | you tested someone else's work and it needs rework | 1,200 characters |

Count before you post. If you are over, the fix is never to compress prose into
denser prose. It is to move material into the PR.

### What goes in the PR instead

The issue answers "what changed and how do I check it". The PR answers "how was
it built". When a comment runs long, almost everything you want to keep belongs
on the other side of that line: implementation reasoning, alternatives
considered, why a review suggestion was not taken, anything about tests, anything
about getting code to `master`, notes addressed to a reviewer rather than to
whoever verifies the behavior, and incidental tidy-ups.

Link the PR once. GitHub cross-links it both ways already.

### Comments that must not be posted at all

There is no status-update comment. If a comment would not change what gets
verified or what the next engineer needs to know, it does not get posted. Never:

- tooling narration — "the formatter is clean now", "the hook blocked my push";
- test-suite results in any form, including "full suite green";
- rebase, branch, worktree, or merge-conflict reports;
- progress without outcome — "starting the second half now";
- anything you would describe as being for the record rather than for a reader.

A genuine blocker is the exception, and it is a real comment with a real ask:
what is blocked, what you need, what you tried.

### Tone

Write as an engineer, in the first person, reporting outcomes.

> **Good.** Revoked keys now fail on the first request instead of surviving the
> cache TTL. To check: create a key, call `GET /api/v1/me` with it, revoke it,
> call again — the second call should be a `401` immediately.

> **Bad.** **[Implementation Complete]** — API key revocation functionality has
> been implemented as per requirements. Awaiting sign-off.

No formal register, no reference to internal workflow ("awaiting approval", "as
instructed"), no passive voice hiding who did what.

---

## 5. Verification notes

The comment that says the work is ready. Template:

```text
Done — <one sentence on what changed>. PR: #<n>

How to check:
1. <step naming a real route path, a real actor, and real input>
2. <step>
3. <step>

Edge cases: <at most three, each one the branch actually handles>
Deploy: <migrations / config / env, or "nothing">
```

**Every line must be grounded in code that shipped on this branch.** Route paths
come from `src/router.rs`. Described behavior must trace to a function in the
diff. Edge cases must reflect handling the branch added, not speculation about
what might go wrong. Name the actor and the permission for every step. If a line
cannot be validated against the diff, delete it — a short set of fully grounded
notes is worth far more than a long set sprinkled with plausible inaccuracies.

**The deploy line is not optional.** In this repo that usually means: a new
migration (they run on startup, so say whether a rollback is possible), a new
env var, a permission seeded that existing users will not hold until they log in
again, or a job interval change. Say which, or say "nothing".

### Verification failed

The counterpart to Verification notes, posted by whoever tested the work rather
than whoever built it. Same grounding rules apply: real file, real line, real
observed behavior — not "the tests look incomplete" or "this seems risky".

```text
Held for rework — <n> item(s), no <what you ruled out, e.g. "security or test
regressions"> found.

1. <file:line> — <what's wrong, in one or two sentences>
2. <file:line> — <what's wrong>

<one line naming anything you verified is NOT a problem, if the issue or PR
raised the question — e.g. a stated cross-repo check that turned out to be a
no-op>
```

Move the issue to `Returned` in the same round as this comment, not before it —
a status change with no comment attached tells the next reader nothing about why.

---

## 6. The board

`Osmium / Website` — project **#7**, owner `vzdc-artcc`. Eleven columns, left to
right. Use these names verbatim; do not invent shorter ones.

| Column | Means | Who moves it |
| --- | --- | --- |
| `Blocked` | cannot continue, or sequencing blocked | you — and say why, with a real ask |
| `Triaging` | filed, not yet triaged | **you, when you file it** |
| `To Do` | scoped and cleared to start | the maintainer |
| `Returned` | kicked back for rework | the maintainer |
| `In Build` | actively being implemented | **you, when you start** |
| `Post Build` | built and self-reviewed, pending verification | **you, when you hand back** |
| `Testing Queue` | selected for human testing | the maintainer |
| `In Test` | under test | the maintainer |
| `Code Review` | final approval before shipping | the maintainer |
| `Shippable` | approved and ready to ship | the maintainer |
| `Done` | merged and deployed | when the PR merges |

An agent sets three of these and only three: `Triaging` on anything it files,
`In Build` when it starts, and `Post Build` when it hands back. Every transition
out of `Triaging` is the maintainer's, and so are `Testing Queue`, `In Test`,
`Code Review`, `Returned`, `Shippable`, and `Done`.

`Blocked` is yours to set, but only alongside a comment naming what would
unblock it. Moving an item to `Blocked` silently says nothing.

Priority is **not** a board field. It lives on the label, so it shows up in
`gh issue list` and on the issue itself rather than only to whoever opens the
board.

### Ids, for `gh project item-edit`

```text
project id       PVT_kwDOCLQ6cs4BjL9-
Status field id  PVTSSF_lADOCLQ6cs4BjL9-zhiBRW0
  Blocked        eb1b5fe2        Post Build      7f5e4bbe
  Triaging       b333280a        Testing Queue   65c71cee
  To Do          8189ab04        In Test         31bdbdaf
  Returned       4274f0ef        Code Review     b6d9b281
  In Build       0ec2e220        Shippable       41794d84
                                 Done            98236657
```

Re-read them rather than trusting that table if a column has been renamed or
re-added since — an option id does not survive being deleted and recreated:

```bash
gh project field-list 7 --owner vzdc-artcc --format json --limit 30 \
  | python3 -c "import sys,json;[print(o['id'],o['name']) for f in json.load(sys.stdin)['fields'] if f['name']=='Status' for o in f['options']]"
```

Find an item id, then set its status:

```bash
gh project item-list 7 --owner vzdc-artcc --format json --limit 300 \
  | python3 -c "import sys,json;print([i['id'] for i in json.load(sys.stdin)['items'] if (i.get('content') or {}).get('number')==123][0])"

gh project item-edit --project-id PVT_kwDOCLQ6cs4BjL9- --id <item-id> \
  --field-id PVTSSF_lADOCLQ6cs4BjL9-zhiBRW0 --single-select-option-id <option-id>
```

Two things that make that lookup lie. `--limit` defaults low, so always pass it
and sanity-check the count against the board. And an item whose content is null
(a draft card) returns `None` rather than `{}` from `i.get('content', {})`, so
`.get('number')` on it raises — write `(i.get('content') or {})` as above.

### Changing the column set

`updateProjectV2Field` replaces the whole `singleSelectOptions` list rather than
appending to it, and **list order is column order**. An entry carrying an
existing option id renames that option in place and keeps every item assigned to
it. An entry with no id creates a new column. An entry you omit is deleted,
taking its items' status with it.

So: send the complete list every time, with the id on every option you mean to
keep. Back the board up first (`gh project item-list 7 --owner vzdc-artcc
--format json --limit 300`) and diff item id to status afterwards. A dropped id
is indistinguishable from a rename until you look at the items.

---

## 7. Filing an issue is two steps

`gh issue create` does **not** put the issue on the board, and an issue that is
not on the board does not exist as work. Nobody triages it and it surfaces only
to whoever thinks to run `gh issue list`.

```bash
gh issue create --repo vzdc-artcc/osmium --title "..." --body-file <file> \
  --label "type:bug" --label "area:access"

gh project item-add 7 --owner vzdc-artcc \
  --url https://github.com/vzdc-artcc/osmium/issues/<n>
```

**Then set its Status to `Triaging`.** `item-add` leaves Status empty, and an
item with no Status sits in no column at all — worse than not being on the
board, because it looks filed while being invisible on the thing people read.

**Verify, do not assume.** Both `item-add` and `item-edit` print nothing on
success, so neither command's silence is evidence. Re-run the item lookup and
read the status back before reporting the issue as filed.

**An empty read straight after `item-add` is lag, not failure.** The item is not
queryable the instant the command returns, so chaining the add into a lookup in
one shell invocation routinely finds nothing. Re-run the lookup as a separate
call before concluding anything, and **never re-run `item-add` to "fix" it** —
the add already landed, and a second one puts the issue on the board twice.

`Triaging` is where an agent's involvement ends. Accepting it, prioritizing it,
and moving it to `To Do` are the maintainer's calls.

---

## 8. Labels

Labels are the canonical classification. The board's `Status` field is the
canonical workflow state. Do not mirror status in labels or the two will drift.

| Prefix | Meaning | Set by |
| --- | --- | --- |
| `type:` | bug / feature / chore / documentation | whoever files |
| `area:` | which subsystem | whoever files |
| `priority:` | critical / high / medium / low / trivial | **the maintainer, never an agent** |
| `technical-debt` | qualifying pre-existing follow-ups | agent, per the four tests below |
| `blocked-by-code` | parked pending an unbuilt subsystem | either |

The canonical list lives in `.github/labels.yml` and is synced by
`.github/workflows/labels.yml`. Add a label there, not through the GitHub UI, or
the next sync run will not know about it.

`bug`, `enhancement`, `documentation`, and the other GitHub defaults are
**legacy**. They remain for historical issues. Do not apply them to anything new.

### Never set the priority yourself

If you are an agent, the severity you would assign is your opening assumption,
not your decision. Put the options to the maintainer and let them pick.

| Review severity | Default label |
| --- | --- |
| CRITICAL | `priority:critical` |
| MAJOR | `priority:high` |
| MODERATE | `priority:medium` |
| MINOR | `priority:low` |
| NITPICK | `priority:trivial` |

---

## 9. Scope, and when a follow-up is allowed

Work within the logical scope of the issue — the scope a sensible engineer would
read into it, not the letter of its wording, and not beyond it.

**A defect you introduced is yours to fix, now, on this branch.** Never defer it
and never file it, whatever its grade and however tangential it feels. Settle
authorship with the diff, not from memory: if the line appears in
`git diff origin/master...HEAD` as an addition, you wrote it.

**File a follow-up only when all four are true:**

1. It is a **defect** — broken behavior. Not a refactor, a test you would like to
   add, a doc correction, or a tidier abstraction.
2. It is **pre-existing** on `origin/master`.
3. It is **outside the logical scope** of the issue you are on.
4. It is **not already filed**.

Fail any one and there is no issue. A defect in scope gets fixed here; a
non-defect gets left alone and named in your report. Qualifying issues carry
`technical-debt` and reference the parent (`Relates to #123` in the body).

**No `priority:` label on a follow-up you file, same as any other issue.** §8's
"never set the priority yourself" applies here too — a follow-up found mid-work
is still not yours to grade. Leave the label off; the maintainer adds it.

**Test 3 has a tell you will otherwise talk yourself past: does it block *this*
branch?** A defect standing between your work and a merge is not outside the
logical scope, however unrelated its subject looks. It is the work — the issue
you are on cannot be delivered without it. Ask the concrete question rather than
the abstract one: if this stays open, can the branch ship?

**Never open an issue for work you are about to do in this session.** The issue
exists to survive the session. Work that will not survive it does not need one.

### Test 4: searching for duplicates

The first three tests interrogate the finding. None asks whether somebody raised
it before, so you can pass the gate cleanly and still file a duplicate.

**Search the entity, not your phrasing.** Two descriptions of one defect rarely
share a verb. Search the module, function, table, or route — `roster_sync`,
`resolve_audit_actor`, `access.service_accounts` — never the wording of your
finding.

**Apply no state filter.** `--state all`. A closed duplicate is the strongest
signal there is: a human already judged this work, and re-filing walks back into
a decision that was already made.

**Run two or three narrow searches, not one long one.** A single query with
several AND-ed terms is how duplicates get missed.

```bash
gh issue list --repo vzdc-artcc/osmium --state all --search "roster_sync" --limit 30
gh issue list --repo vzdc-artcc/osmium --state all --search "visitor dropped roster" --limit 30
```

Then scan the neighbours — same-incident issues cluster in creation time and so
in number.

**When you find one, do not drop your finding.** The later observation is often
the better-evidenced one. Comment on the existing issue with whatever yours
establishes that it does not, and report it in one line instead of filing. Do not
reopen, re-label, or reassign it; it is not yours.

**Same area is not the same defect.** The test is same defect or same requested
outcome, not same neighbourhood. If you cannot name the single change that would
close both, they are not duplicates.

---

## 10. Reading an issue before you work it

Never work from the title and body alone.

1. **Body** — every requirement, acceptance criterion, and scope boundary.
2. **All comments** — `gh issue view <n> --comments`. Comments routinely carry
   the real requirements: scope changes, design decisions, clarifications that
   override the body.
3. **Synthesise** a consolidated requirements list merging body and comments.
   Where a comment contradicts the body, the later comment wins.
4. **Linked issues and PRs** — `gh issue view <n> --json ...` shows
   cross-references. Follow them for dependencies.

**The full comment thread is the spec.** Reviewing code against the body alone
misses every clarification that happened after the issue was written.

Where the detail lives in a pasted screenshot, `gh` gives you a URL and not the
bytes, and an authenticated `user-images.githubusercontent.com` URL will not
fetch. Do not burn attempts on it — ask for the image to be pasted into the chat.

### Other rules

- Never close an issue you did not complete. Never bulk-transition.
- If the acceptance criteria do not match the implementation, flag the
  discrepancy before marking anything done.

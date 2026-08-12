---
tags: [<topic>, spec]
type: spec
status: draft   # draft | approved | incomplete | underspecified | implemented | stale | violated | superseded
updated: <YYYY-MM-DD>
---
# Spec: <Feature / change>

One-line summary of what this spec covers. A **lightweight design spec** — it exists to catch
and constrain architectural drift. The rules (scan first; adhere; don't modify without user
agreement; reconcile divergence) are in [[conventions]] → Specs.

## Requirements

### Goal (definition of done)

A single, **verifiable** statement of what "complete" means — the objective criterion used to
confirm the task is finished and correct. The Verification plan below must demonstrate it.

### Constraints

**All** constraints — hard rules, boundaries, assumptions. Missing a constraint is the
commonest cause of drift.

### Functional requirements

What must be true; the problem being solved; what is explicitly in and out of scope.

## Design discussion

Options weighed and the decisions taken (locked vs open), with rationale. Record the rejected
options too — a rejection with a reason stops it being re-proposed.

## Implementation details

How it is actually built — files, mechanisms, interfaces, key call paths. Paths, not line
numbers.

## Verification plan

How the result is proven correct end to end: tests, commands, manual steps. State what was
actually run, and what was **not** verified.

## Implementation status

**status:** <the frontmatter value> — what is done vs pending.

Record any **divergence between this spec and the code**, each classified as *spec wrong*
(`stale`) / *implementation wrong* (`violated`) / *implementation incomplete* (`incomplete`) /
*spec incomplete* (`underspecified`).

## Related

- [[related-page]]: why it's related

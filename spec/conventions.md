---
tags: [meta, maintenance, concept]
type: concept
updated: 2026-08-12
---
# Conventions

How this tree is authored and kept current. Read with [[home]] (the index). The goal: a
**compounding artefact** that is maintained alongside the code, not a one-off dump that rots.

## Note types

Every page declares a `type:` in frontmatter. Match the type to the content; if unsure, use
`concept`.

| type | What it is |
|---|---|
| `spec` | A lightweight design specification for a planned or built feature. Lives at `spec/<slug>.md`, carries a `status:`. |
| `concept` | An idea, topic, or body of knowledge. The default for wiki pages. |
| `entity` | A page for one specific named thing: a crate, tool, file format, external dataset. |
| `index` | The catalog (`home.md`). |
| `log` | The chronological record (`wiki/log.md`). |

## Frontmatter

```yaml
---
tags: [<topic>, <type>]      # include the type as a tag so a graph view can colour by type
type: <spec | concept | entity | index | log>
status: <draft | approved | incomplete | underspecified | implemented | stale | violated | superseded>   # spec pages only
superseded-by: [[<slug>]]    # spec pages only; REQUIRED when status: superseded
updated: <YYYY-MM-DD>
---
```

## Specs

A spec exists to **catch and constrain architectural drift**. Sections: *Requirements ·
Design discussion · Implementation details · Verification plan · Implementation status*. The
template is `_templates/spec.md`.

**Status lifecycle** — the single fact that says how far to trust the spec and, on a
disagreement, which side to fix.

*Normal progression*

- `draft` — being written; the design is **not yet agreed**. Don't build to it.
- `approved` — design agreed; implementation not started.
- `incomplete` — approved, code **underway or partial**. What exists agrees with the spec.
- `underspecified` — code is **built and correct**, and everything the spec says agrees with
  it, but the spec doesn't yet cover the whole feature. List the gaps under *Not yet
  specified*.
- `implemented` — fully built; spec and code **agree**. Keep them in sync: a change in the
  spec's area updates the spec in the **same change**.

*Divergence — set the flag, then resolve back to `implemented`*

- `stale` — the **spec is wrong**, the code is correct → update the spec.
- `violated` — the **spec is correct**, the code drifted → fix the code.

*Retired*

- `superseded` — replaced; set `superseded-by:` and leave a short redirect stub.

Which flag applies, when both a contradiction and an absence exist:

| | the **document** is deficient | the **code** is deficient |
|---|---|---|
| **contradiction** (they disagree) | `stale` | `violated` |
| **absence** (something is missing) | `underspecified` | `incomplete` |

Contradiction outranks absence — a wrong sentence misleads where a missing one merely fails to
help. A gap in the code outranks a gap in the document, being the more actionable fault.

**Working rules**

- **Scan first** — before writing a new spec, read the existing ones. Extend rather than fork a
  conflicting spec.
- **Verifiable goal + all constraints** — state a definition of done that can be objectively
  checked, and capture every constraint up front. A missing constraint is the commonest cause
  of drift.
- **Adhere** — heed the relevant spec before changing related code.
- **Don't rewrite silently** — no edits to an existing spec without the user's agreement.
  Status changes are the exception, and are always flagged.
- **Reconcile divergence** — when spec and code disagree, set the matching status, record the
  detail under Implementation status, and resolve it.
- **Source-controlled and impersonal** — these are committed documents read by anyone, later,
  with no session context. Never reference a local or personal path (`~/…`, `/home/…`, a
  scratchpad, an agent plan file, a handover): those don't exist for other readers. Cross-
  reference only shared artefacts — other pages via `[[wikilinks]]`, in-repo paths, commits,
  PRs. Avoid first-person asides and "today / now / we just"; state standing facts.

## Wiki

Pages under `wiki/` record what is true about the code and what was learned the hard way —
verified API facts, measurements with their numbers, and traps that cost time. Prefer a fact
with its evidence ("2.19 s vs 2.14 s, so not worth it") over an unsourced assertion.

- **Links are the point.** Cross-link liberally with `[[wikilinks]]`. A link to a page that
  doesn't exist yet is fine: record it under Stubs in [[home]].
- **Cite file paths, not line numbers** — lines rot.
- **`log.md` is append-only**, newest at the bottom, one line per session of work. Keep the
  prefix `## [YYYY-MM-DD] <op> | <title>` so it stays greppable.

## When to update (triggers)

Update this tree as part of the change that prompts it — the same commit, ideally.

| Change | Pages to touch |
|---|---|
| New feature or architectural change | scan `spec/*.md`, then write or extend a spec |
| Change in an existing spec's area | that spec, in the same commit |
| A Bevy API surprise or version bump | [[bevy-0-19-api]] |
| Build/toolchain change, or a new timing measurement | [[build-performance]] |
| Change to the skybox asset or its generator | [[skybox-pipeline]] |
| Anything that cost more than an hour to work out | a wiki page, so it costs nobody an hour again |

## Lint routine

Run after any sizeable change. Checks the link graph for broken links, orphans, and wikilinks
accidentally wrapped across a newline (which render as literal text):

```bash
cd spec && python3 - <<'PY'
import re, pathlib, collections
pages = {p.stem: p for p in pathlib.Path('.').rglob('*.md') if '_templates' not in p.parts}
inb = collections.defaultdict(set); out = {}; split = []
for s, p in pages.items():
    txt = p.read_text()
    txt = re.sub(r'```.*?```', '', txt, flags=re.S)   # fenced blocks hold example links
    txt = re.sub(r'`[^`\n]*`', '', txt)               # so do inline code spans
    tg = {m.group(1).strip() for m in re.finditer(r'\[\[([^\]|#\n]+)', txt)}; tg.discard(s)
    out[s] = tg; split += [s for _ in re.finditer(r'\[\[([^\]]*?\n[^\]]*?)\]\]', txt)]
    for t in tg: inb[t].add(s)
allp = set(pages)
print("pages", len(allp), "links", sum(len(v) for v in out.values()))
print("broken", sorted(t for t in set().union(*out.values()) if t not in allp))
print("orphans", [s for s in allp if s not in ('home', 'log') and not inb.get(s)])
print("split-links", split)
PY
```

Broken links are either a typo or a stub worth writing — decide which, don't leave it. Also
check for contradictions between pages and claims a newer source supersedes.

## Related

- [[home]] (index) · [[log]] (record)

# CLAUDE.md

Guidance for Claude Code (claude.ai/code) when working in this repository.

`hex-terrain` is a Bevy application — currently a 3D sandbox scene that hex terrain work will be
built into. Commands for running it natively and on the web are in `README.md`.

This file is deliberately small. **Project knowledge — design decisions, API facts,
measurements, and the traps already discovered — lives in `spec/`, not here.** Consult it on
demand rather than carrying it in every context, and re-derive nothing that is already written
down there.

## Specifications (`spec/`)

`spec/` is the project's ongoing specification and developer knowledge base, written as
cross-linked markdown (`[[wikilinks]]`, readable as an Obsidian vault rooted at `spec/`).

- **Start at `spec/home.md`** — the index. Read it first to orient.
- **`spec/conventions.md` is the operating manual**: note types, frontmatter, the spec status
  lifecycle, update triggers, and a runnable lint routine. Follow it when authoring anything
  under `spec/`.
- **`spec/*.md` are the specs.** They exist to catch and constrain architectural drift, and
  carry a `status:` saying how far to trust them.
- **`spec/wiki/*.md` is the knowledge wiki** — what is true about the code and what was learned
  the hard way. `spec/wiki/log.md` is the append-only record.

### Working with specs

- **Before changing code**, find and **heed** any relevant spec.
- **For a new or changed feature**, scan the existing specs first to avoid forking a conflicting
  one, then write or extend a spec: *Requirements · Design discussion · Implementation details ·
  Verification plan · Implementation status*. State a **verifiable goal** and capture **all
  constraints** — a missing constraint is the commonest cause of drift.
- **Never modify an existing spec without the user's agreement.** Status changes are the
  exception, and are always flagged.
- **If a spec and the code diverge**, set the matching status (`stale` = the spec is wrong,
  `violated` = the code drifted), record the detail on the spec, and reconcile it.

### Maintaining the wiki

The wiki is a compounding artefact, not a one-off dump. Keep it current **in the same change**
as the code:

- Anything that took real effort to work out — an API surprise, a measurement, a trap — gets a
  wiki page or a section in one, so it costs nobody that effort again. Record the evidence
  alongside the claim: the numbers, the command, the source path.
- Prefer verifying against the vendored dependency source in the cargo registry over recalling
  an API from an older version. Bevy renames things between releases.
- `spec/conventions.md` → *When to update* lists which page each kind of change touches.
- Cite file paths, not line numbers. Link liberally; a link to a page that doesn't exist yet is
  a stub worth recording in `spec/home.md`.

## Architecture: keep the model out of the renderer

A standing rule for this project, not a decision local to one feature.

**The model is dimensionless.** `src/hex/` holds coordinates, topology and per-location data, and
knows nothing of world units, scale, or which plane anything is drawn on. It has no Bevy types
beyond `glam` vectors and is testable with no app.

**A projection layer owns world units.** `src/view/layout.rs` is the only place a scale, an origin,
or a plane exists. It converts model coordinates to world positions and back; the renderer and
hit-testing both go through it.

**The renderer consumes the two together** and adds nothing of its own that the model should know.

When adding anything, ask: *would this still make sense with no renderer at all?* If yes it belongs
in the model; if it involves a distance in world units, it belongs in the projection. Two mechanisms
keep this honest rather than aspirational:

- Model types deliberately **do not derive `Resource`**, so putting Bevy in the model is a compile
  error. Newtypes in `view/` (`GridModel`) are the bridge.
- A round-trip test run at **two different scales** fails if scale leaks into the model. Add the
  equivalent for any new projection-dependent conversion.

One nuance worth knowing, because it looks like a counterexample: `Orientation` lives in the *model*
despite pointy-versus-flat being a rendering choice. It is dimensionless, and doubled coordinates
depend on it, so the model needs it — the boundary is world units, not "anything visual". Its
projection matrices stayed in the view layer. See `spec/hex-grid.md` → Design discussion.

## Conventions

- **Comments describe the code, not the change.** Keep source comments specific to what the code
  does now and why. Don't reference a plan, a past conversation, or the previous version of the
  code.
- **Deliberate simplifications are marked** with a `ponytail:` comment naming what was skipped
  and when to add it. Treat these as intent, not oversight.
- **Verify, don't assume.** State what was actually run and what was not — the specs record both.

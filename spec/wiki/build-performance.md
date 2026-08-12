---
tags: [build, tooling, concept]
type: concept
updated: 2026-08-12
---
# Build performance

What makes this project slow to build, what was measured, and what was rejected. Every number
below came from a run on the development machine; treat them as ratios rather than absolutes.

## The measurements

Baseline is a debug build with Bevy linked statically.

| Configuration | Link step | Edit→rebuild cycle | Binary |
|---|---|---|---|
| static, GNU ld (bfd) | 2.19 s | 37 / 40 / 56 s | 1.72 GB |
| static, LLD | 2.14 s | — | — |
| **dynamic linking, GNU ld** | — | **1.94 s** | **5.9 MB** |
| dynamic linking, `opt-level = 0` locally | — | 1.30 s | — |

## Dynamic linking — adopted

`bevy/dynamic_linking` is the whole win: a ~20× faster cycle and a binary three orders of
magnitude smaller, because the final link no longer pulls in all of Bevy.

It **does not build for wasm**, so it cannot be enabled globally. Scope it by target:

```toml
[target.'cfg(not(target_family = "wasm"))'.dependencies]
bevy = { version = "0.19", features = ["dynamic_linking"] }
```

Cost: the binary needs `libbevy_dylib.so` from `target/debug/deps/` at runtime. `cargo run`
handles that; copying the bare binary elsewhere does not. The one-time rebuild of Bevy as a
shared library takes about five minutes.

## LLD — rejected

No `lld`, `mold`, or `clang` is installed, and `-C linker-features=+lld` is still nightly-only
as of Rust 1.96. The toolchain does ship `rust-lld` plus a `gcc-ld/ld.lld` shim, so LLD is
reachable with two link-args and no installation — but it is **not worth adopting**: 2.19 s
versus 2.14 s.

The reason is the useful insight: linking was never the bottleneck. It is ~2 s of a ~40 s cycle;
the other ~35 s is rustc generating code for the local crate, monomorphising Bevy's generics.
Only dynamic linking attacks that.

## `opt-level` — kept at 1

`[profile.dev] opt-level = 1` with `[profile.dev.package."*"] opt-level = 3` is the standard
Bevy arrangement: without it, debug builds run too slowly to judge anything.

Dropping the local crate to `opt-level = 0` saves a further 0.6 s per cycle but leaves this
project's own code unoptimised — a bad trade once terrain meshing exists. Note that changing
`[profile.dev] opt-level` only rebuilds the local crate, because the `package."*"` override
covers dependencies, so this is a cheap experiment to repeat.

## Web build

`trunk` builds, bundles `assets/`, and downloads the wasm-bindgen version matching Bevy
(0.2.127 for Bevy 0.19), which removes the classic version-mismatch failure. `index.html`
declares `rel="rust"` and `rel="copy-dir" href="assets"`.

Output sizes, both untuned:

| Build | wasm | assets |
|---|---|---|
| debug | 100 MB | 11 MB |
| release | 51 MB | 11 MB |

Debug wasm is too large to be practical over HTTP; use `--release` for anything but a local
smoke test. Release wasm size has had no attention — `opt-level = "s"`, LTO, and `wasm-opt`
are the obvious levers if it starts to matter.

## Traps

- **A rustc wrapper caches compilations.** `build.rustc-wrapper` is set to `kache` in the user
  cargo config. Two consequences when benchmarking: a captured `rustc` command line starts with
  `kache …` and must have that prefix stripped or the "link" is served from cache in ~0.5 s and
  measures nothing; and **`touch` does not invalidate it** — a content-identical file is a cache
  hit, so a benchmark must actually change the source each iteration. An early measurement of
  "1.87 s" for a static rebuild was this artefact.
- **A stale registry index reads as a missing crate.** `dynamic_linking` first failed with
  *"failed to select a version for the requirement `bevy_dylib = ^0.19.0`; candidate versions
  found which didn't match: 0.17.0-rc.2, …"* — but 0.19.0 was published. The cached sparse-index
  entry for that one crate was a year old. Deleting the single cache file under
  `registry/index/<registry>/.cache/be/vy/bevy_dylib` makes cargo refetch it. Expect this for
  any crate whose index entry predates the cache; the error names the wrong cause.
- **Replaying a cargo `rustc` invocation needs cargo's environment.** At minimum
  `CARGO_MANIFEST_DIR`, or Bevy's `#[derive(Component)]` panics with *"CARGO_MANIFEST_DIR is not
  defined"* and the resulting errors point at the derive rather than the missing variable.
- **A `bevy_dylib` shared object is large** — ~1.77 GB in debug. The disk cost of dynamic
  linking is paid in `target/`, not in the binary.

## Related

- [[scene]]: the spec recording dynamic linking as a locked decision
- [[bevy-0-19-api]]: what depending on Bevy 0.19 means at the API level

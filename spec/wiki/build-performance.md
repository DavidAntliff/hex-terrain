---
tags: [build, tooling, concept]
type: concept
updated: 2026-08-14
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

`trunk` builds, bundles the assets, and downloads the wasm-bindgen version matching Bevy
(0.2.127 for Bevy 0.19), which removes the classic version-mismatch failure. `index.html`
declares `rel="rust"` and copies **`assets/shaders` only**, via
`rel="copy-dir" href="assets/shaders" data-target-path="assets/shaders"`. The whole of `assets/`
used to be copied, which shipped the 11 MB starmap cubemap that [[skybox-pipeline]] records as
unwired — `data-target-path` is what keeps the runtime path `shaders/water.wgsl` intact once the
`href` is no longer the asset root. Widen it again the day a second asset is loaded.

Output sizes, both untuned:

| Build | wasm | assets |
|---|---|---|
| debug | 100 MB | 5 KB |
| release | 52 MB | 5 KB |

Debug wasm is too large to be practical over HTTP; use `--release` for anything but a local
smoke test. Release wasm size has had no attention — `opt-level = "s"`, LTO, and `wasm-opt`
are the obvious levers if it starts to matter. `wasm-opt` is the cheapest to try:
`data-wasm-opt="z"` on the `rel="rust"` link, which trunk fetches itself and **only applies in
`--release`**, so dev builds are unaffected.

The release figure is `dist/*_bg.wasm` after `trunk build --release`, measured at 52,336,672 bytes
once `serde`/`serde_json` had been added for [[instrumentation]]'s report. It was 51 MB before, but
**no controlled before/after was run** — other changes landed between the two measurements, so the
serde contribution is not separated out. It is at most about a megabyte against a 52 MB baseline,
which is why it was not worth measuring properly.

Re-measured after the assets narrowing, on 2026-08-13: `dist/*_bg.wasm` is 52,367,266 bytes and
`dist/assets/shaders/water.wgsl` is 5,161 bytes, so the deployed bundle went from ~63 MB to ~50 MB
without touching the wasm.

**`clap` costs about a quarter of a megabyte of wasm.** Measured on 2026-08-14, the one controlled
before/after this file has: `dist/*_bg.wasm` went from 52,367,266 to 52,613,917 bytes when `clap`
was added for [[scene]]'s arguments — **+246,651 bytes, 0.47%** — with default features and the
`derive` feature, on a build where nothing else changed. It is dead weight on web, since there is
no argv there, but scoping the dependency to non-wasm would buy 0.5% at the price of a second code
path, so it is unconditional. Worth revisiting only alongside `wasm-opt`, which is aimed at a much
larger number.

Adding it also invalidates `Cargo.lock`, which is in the CI cache key — so the first Pages run after
it is a cold one, ~12 m 10 s rather than ~3 m 19 s.

### Deploying to GitHub Pages

`.github/workflows/pages.yml` builds on every push to `main` and deploys the result as a Pages
artefact; `dist/` stays gitignored and nothing built is ever committed. Two things about it are
not obvious:

- **`--public-url /hex-terrain/` is required.** The `index.html` trunk generates references the JS
  and wasm by *absolute* path, so without it a project site under `/hex-terrain/` fetches
  `/hex-terrain-<hash>.js` from the domain root and gets nothing. Bevy's own asset fetches are
  relative and need no help. This is invisible when testing locally at a domain root — serve the
  build under the subpath if the flag is ever in question.
- **`CARGO_BUILD_BUILD_DIR: target` overrides `.cargo/config.toml`.** The shared build directory
  exists for local worktrees; in CI there is one checkout and `Swatinem/rust-cache` caches
  `./target`, so pointing the build elsewhere would silently defeat the cache.

The workflow installs a pinned trunk release tarball rather than `cargo install trunk` (minutes) or
a third-party action. Trunk then fetches its own matching wasm-bindgen, so nothing else is needed.

**Pages gzips the wasm, which is most of the size problem already solved.** Measured against the
live site on 2026-08-13: `content-encoding: gzip`, `content-length: 13,629,776` — 13.6 MB over the
wire for the 52 MB file. Quote the compressed figure when talking about load time, and weigh any
`wasm-opt` work against 14 MB rather than 52 MB.

| Run | Wall clock |
|---|---|
| cold, nothing cached | 12m10s |
| warm, docs-only change | 3m19s |

`Swatinem/rust-cache` saved a 473 MiB cache on the cold run, and the second figure is what it buys.
It keeps `~/.cargo` and the dependency half of `./target`, deliberately pruning this crate's own
artefacts, so a later push recompiles `hex-terrain` and re-links rather than rebuilding Bevy — which
is why even a docs-only push still costs three minutes. Two things reset it to cold: a `Cargo.lock`
or rustc change, since both are in the cache key, and GitHub evicting an entry untouched for 7 days.

Verified on 2026-08-13 before the first deploy: the release build served from a `/hex-terrain/`
subpath renders, fetches `assets/shaders/water.wgsl` (200), and logs only the expected WebGL2
downlevel warnings (no OIT, no background motion vectors, no SSAO, no atmosphere — see
[[bevy-0-19-api]]).

### The `.meta` probe is only harmless when the host answers 404

Bevy probes `<asset>.meta` before every asset. GitHub Pages answers that with a real 404, so the
probe is ignored and the asset loads — which is why the deploy verified above worked. **A host that
answers a missing path with its index page and a `200` breaks the asset outright**: Bevy takes the
HTML as meta, fails to deserialize it, and abandons the load, so the asset itself is never even
requested. `trunk serve` does exactly that.

The symptom is silent and misleading. Every mesh whose material needs the shader simply never
draws — no pipeline error, no shader error, nothing in the console but one line:

```
ERROR Failed to deserialize meta for asset shaders/terrain.wgsl: ... ExpectedNamedStructLike("AssetMetaMinimal")
```

That reads as cosmetic and is not. It cost a full session of shader bisecting on 2026-08-14, all of
it wasted, because editing `terrain.wgsl` provably changed nothing — including replacing the whole
fragment body with solid red. **The file was never being fetched.** The cheap check that would have
ended it in a minute, in the page's console:

```js
performance.getEntriesByType('resource').filter(e => /wgsl/.test(e.name)).map(e => e.name)
```

Only `.meta` entries, and no entry for the shader itself, is the whole diagnosis.

`main.rs` therefore sets `AssetPlugin { meta_check: AssetMetaCheck::Never, .. }`. No asset here has
a `.meta` file, so the lookup can only ever cost two round trips and, on the wrong host, the render.

## Shared build directory — adopted

Every worktree used to get its own `target/`, which is expensive twice over. Measured on
2026-08-13, with two worktrees in existence:

| | |
|---|---|
| `hex-terrain/target` | 20 GB (14 GB `debug/deps`, 5.6 GB `wasm32-unknown-unknown`) |
| `hex-terrain-camera/target` | 3.8 GB |
| Free disk | 34 GB of 935 GB (97 % used) |

**A compile cache does not fix this, and `kache` was already doing its job.** `kache stats` at the
time: 48.2 % hit rate, ~37 min of compile work avoided in 24 h, and the 1.6 GiB `bevy_dylib`
compile genuinely in the store (`kache why-miss bevy_dylib` shows the 27.7 s miss that populated
it). But kache caches *compilations*, not the tree they land in — on a hit it still materialises
~14 GB of artefacts into whichever `target/` asked. The duplication is structural.

`.cargo/config.toml` at the repo root, checked in, therefore sets:

```toml
[build]
build-dir = "{cargo-cache-home}/hex-terrain-build"
```

Three choices worth keeping:

- **`build-dir`, not `target-dir`.** Only intermediates are shared. Each worktree keeps its own
  `target/` holding the uplifted binary, so `cargo run` and `./target/debug/hex-terrain` stay
  unambiguous per worktree. A shared `target-dir` would make `target/debug/hex-terrain` one
  filename that every worktree overwrites. `build.build-dir` is stable — the 1.92 cargo reference
  documents it without an unstable marker; only the `-Z build-dir-new-layout` *layout* is unstable.
- **`{cargo-cache-home}` templating, not a relative path.** Resolves to `~/.cargo/hex-terrain-build`
  on any machine. A relative `../hex-terrain-build` also works — config paths resolve against the
  parent of the `.cargo` directory, so sibling worktrees land on the same place — but that silently
  depends on worktrees staying siblings.
- **Checked in, not gitignored.** That is the point: `git worktree add` yields a worktree that
  already shares the build directory, with nothing to remember.

Cargo config merges per key, so this leaves `build.rustc-wrapper = "kache"` in the user config
alone. kache still runs, now backing one build directory instead of N.

### What it bought

Measured by adding a throwaway worktree twice — once without the config, once with it — against a
shared directory already populated:

| Fresh worktree, first `cargo build` | Wall clock | Disk it added |
|---|---|---|
| own `target/`, kache warm | 2 min 12 s | 7.2 GB |
| **shared build dir** | **11.8 s** | **~0** |

Populating the shared directory from nothing cost 17 min once, on a machine also running another
build; that is the same one-off Bevy compile every worktree used to pay separately.

"~0" is literal: cargo **hardlinks** the uplifted `target/debug/libbevy_dylib.so` to the file in the
shared directory — same inode, link count 2 — so the 1.77 GB object exists once on disk while
appearing in every worktree. `du` on a worktree's `target/` reports 1.9 GB, and it is a lie. This
holds only because `~/.cargo` and the worktrees are on one filesystem; across filesystems cargo
would have to copy.

**No cross-worktree thrash.** The worry was that two worktrees would ping-pong-recompile the same
crates. They do not: after the second worktree's first build, alternating `cargo build` between the
two is 0.3 s each way. The six crates recompiled during that first build (`bevy_anti_alias`,
`bevy_ui_render`, `bevy_gizmos_render`, `bevy_internal`, `bevy_dylib`, `bevy`) settle once and stay
settled.

## Traps

- **A shared build directory serialises concurrent builds.** Cargo takes an exclusive lock on it,
  so a second worktree building at the same time waits with *"Blocking waiting for file lock on
  build directory"* rather than running in parallel. This is the price of the section above and it
  is worth paying here, but it now spans worktrees rather than staying inside one — relevant if an
  IDE runs `cargo check` in the background. The escape hatch is `CARGO_BUILD_BUILD_DIR` pointed at
  a private path for that one consumer.
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

# Cross-Platform Myco: Capability Layer

Status: design draft

Scope: capability layer only. See [Out of scope](#out-of-scope) for rendering and bundling.

## Thesis

Myco can share code between browser and server without the plumbing that usually accompanies it: no conditional exports, no bundler aliasing, no hand-rolled injection, and no way for a dependency to opt out.

**Every platform capability in Myco has to be injected.** Filesystem and network access are objects that must be passed in, because code holds no authority it was not handed and every grant traces back to the entry point, which Rust calls with the powerbox. A browser target would treat DOM access the same way. Platform-specific code is exactly the code that uses platform capabilities.

An injected dependency is a swappable one, and injection is mandatory here for precisely the dependencies that vary by platform.

Elsewhere, platform functionality is ambient. A function that needs the filesystem reaches for it directly, and that reach is fixed at the point of use. Portable code has to intercept it, and since the discipline is optional, one dependency that skips it makes everything above it non-portable.

"Which platform does this run on" is therefore answered by "which capabilities does this need," and the answer is written in a function's parameter list.

Module boundaries can then be drawn by domain rather than by technology. A single module may contain a function that takes a `Files.WriteToken`, callable only on the server, alongside one that takes a `Store.WriteToken`, callable anywhere. At the type level, the powerbox named in `main` is what distinguishes a browser program from a server program.

What this does not remove is the design work. Target-neutral interfaces still have to be designed rather than derived, which is section 2, and they cost something at the call site, which is section 6. The saving is that the seam already exists everywhere, because the security model put it there.

The claim is untested, and nothing in the tree is currently portable enough to test it. [Phasing](#phasing) says when that changes.

## Design

### 1. `Myco` becomes a namespace; powerboxes are per-target

Today `Myco` is both an interface and a namespace in `runtime/.myco/myco.d.ts`. Split these:

```ts
declare namespace Myco {
  // Token types are SHARED. Target-neutral, named by shared library code.
  namespace Files { interface WriteToken { ... } }
  namespace Store { interface ReadToken { ... } interface WriteToken { ... } }
  namespace Http    { interface FetchPrefixToken { ... } }
  namespace Modules { interface ImportToken { ... } }
  namespace Dom     { interface ElementToken { ... } }

  // Powerboxes are SPLIT. Named only by entry points.
  interface Server  { files: Files.Requests; http: Http.Requests; modules: Modules.Requests; argv: string[]; }
  interface Browser { dom: Dom.Requests;     http: Http.Requests; modules: Modules.Requests; }
}
```

Token types are shared; only the powerboxes that vend them are split. A library names token types and never a powerbox, so the same code compiles against either target, and the convention that keeps a library well-attenuated tends to be the one that keeps it portable. Nesting token types under their powerbox would undo this, since naming one would pin the code to a target.

Sharing a type is not sharing a contract. `Http.FetchPrefixToken` exists on both targets and behaves differently on each; the differences are listed under [Known asymmetries](#known-asymmetries). Section 2 has more on why matching signatures is the weaker half of the problem.

The governing rule is that a powerbox type is whatever `main` declares it takes, so the set is open rather than a binary.

Nothing in the tree follows that convention yet. Functions outside the entry points take the whole `Myco` and nearly all of them use it to call `request*`, so each library mints its own authority rather than receiving it. Requesting belongs to `main`; a library should be handed the token, not the means to make one.

A signature audit is not sufficient for the migration, since nested closures capture the powerbox without appearing in any parameter list.

### 2. Target-neutral capability interfaces

Filesystem capabilities are scoped to an absolute path, and that binding will not survive contact with a browser. Every capability added from here needs the same decision: is it genuinely specific to one target, or a target-specific shape of something both targets can offer?

Where an interface can be shared, look for an existing standard shape before inventing one. The mainstream answer to cross-platform JS is interface standardization, as with `fetch`, Web Streams, and Web Crypto. This section is that same answer reached without an ecosystem to converge on. A portable store modeled on an existing key-value or streams interface gives third-party code a chance of fitting through it. An invented one guarantees nothing does.

Where no standard fits, derive the interface from what shared code needs. Deriving it instead by removing methods from the most powerful implementation yields an abstraction shaped like whichever target you started from.

The test either way is whether an implementation has to lie. If the browser version must throw, or must synthesize a value it does not have, the abstraction is wrong and the capability was target-specific after all.

That test catches missing methods and misses divergent semantics, which is where a portable store will actually break. A directory and IndexedDB both implement get, set, and delete without lying, while disagreeing about whether a write is atomic, whether a group of them is transactional, and whether `list` returns a snapshot or a live view. Those answers belong in the interface rather than in whichever implementation the author happened to test against.

`Store` is a worked example rather than a specification. Get, set, delete, and list-by-prefix look implementable over both a directory and IndexedDB, while `stat`, symlinks, permissions, and `exec` are not, and stay in `Files`. Settle the actual method set against the first piece of shared code that needs it.

#### Narrowing

Any holder of a capability can vend a narrower facet of it. The operation hangs off the token:

```ts
const { facet: store, revoke } = dirToken.asStore();
```

Putting it on the token rather than the powerbox matters. `ReadDirToken` carries `read`, `stat`, `list`, and `sync` directly, so the token is the capability object. If narrowing lived on the powerbox, only `main` could attenuate, and an intermediate library could not reduce what it passes onward.

Narrowing wraps the capability object it is called on, not the token underneath it. A facet minted from a root token calls the ops layer; a facet minted from another facet calls that facet's methods. That chain is what makes revocation cascade (section 3), and it costs one indirection per level. Either way the construction is JS-side, following the pattern the `requestRead` wrapper already uses in `runtime/src/index.ts`, and creates no registry entry in Rust.

The facet must actually lack the wider methods. Types are erased, so an object that merely declares a narrower type still answers to `as any`.

Brand the token types so a wide token cannot structurally satisfy a narrow interface, rather than relying on the method sets staying disjoint.

Narrowing is a grant decision. Handing a library a facet rather than the token it came from is the same kind of choice as handing it a token rather than the powerbox, made in code at the point of the call.

### 3. Revocation

`invalidate_token` in `cli/src/run/capabilities.rs` unregisters a token from the registry, and nothing calls it. It is not exposed as an op, and exposing one would not be sufficient on its own: `MycoOps` is never assigned to the global, so user code reaches ops only through objects the runtime hands it. The design question is what that surface looks like.

Revocation authority belongs to the grantor. A `token.revoke()` method gives it to the holder instead, which is backwards, since a library could then revoke a capability out from under the code that granted it. So narrowing returns a pair, and the grantor keeps the revoker while the grantee receives only the facet.

```ts
const { facet, revoke } = dirToken.asStore();
runLibrary(facet);
// ...later
revoke();
```

The revoker flips a flag checked at call entry, so operations already in flight complete and only subsequent calls fail. Rejecting in-flight work would mean tracking pending operations per facet, and it buys little: a grantee that already has the bytes cannot be made to forget them.

Cascade falls out of the delegation chain from section 2. A facet minted from another facet routes its calls through the parent, so revoking the parent breaks the chain and everything below it stops working, with no parent registry and no schema change in Rust.

This does not revoke the underlying token. Two facets minted separately from the same object are independent, each with its own flag, and revoking one leaves the other live. That is the correct semantics: revocation withdraws the grant you made, not everything anyone derived from the same capability.

A flag at call entry only covers calls, not the objects those calls hand back. Section 2 recommends modeling portable interfaces on shapes like Web Streams, which are exactly the shapes that hand things back: a `ReadableStream` given out before `revoke()` keeps pulling afterwards. So any method returning a handle, stream, or iterator has to return a wrapper bound to the same flag as the facet that produced it. Nothing in the type system enforces that, and it holds transitively through every returned object graph, so it is a trusted discipline rather than a mechanism.

The revoker is itself an authority, and destructuring hands it out as easily as the facet. Whoever holds it can withdraw a grant they did not make.

Revoking a root token still needs `invalidate_token` exposed, for the case where `main` wants a capability gone entirely rather than withdrawn from one grantee. A grant is not always one registry entry: `requestReadWrite` spreads two independently requested tokens, so withdrawing it means invalidating both.

### 4. The ambient surface

Everything in Myco is meant to arrive as a grant. Some things do not. V8 supplies the intrinsics, the runtime installs a handful of globals, and it registers a module loader. Each needs the same question asked of it: what can a holder do with this that they could not do already?

Stock JS answers that question narrowly. `eval` and `Function` are present, and `Function("return this")()` does yield `globalThis`, but that object holds only `console`, `TextEncoder`, `TextDecoder`, `TOML`, and `__mycoTimerComplete`. Neither the powerbox nor the ops table is ever assigned to it: Rust holds both and passes the powerbox to the entry point's default export directly. The module loader is the one ambient thing that answers differently, and it is dealt with below.

#### Intrinsics stay, and get frozen

Intrinsics confer no authority. A library that reads the clock or draws a random number still cannot exfiltrate, read a file, or persist anything without a token, so they amplify authority already granted rather than supplying any. None of them is capability-gated.

What they do need is hardening. Grepping `runtime/src/` and `cli/src/` for `freeze|harden|seal|preventExtensions` returns zero hits, so `Object.prototype` is mutable and any library can poison shared behavior. Poisoning grants no authority, but it corrupts code that already holds some.

The sharpest instance is `__mycoTimerComplete`, installed by `runtime/src/index.ts`, a writable global that dispatches every timer callback and is invoked by Rust through string eval in `cli/src/run/ops/time.rs`. Any library can overwrite it and silently swallow or hijack every timer in the process. It is the only writable internal left on the global.

Run [SES](https://hardenedjs.org/)'s `lockdown()` on both targets. It transitively freezes intrinsics and tames known escape hatches, the most important being `(function(){}).constructor("return this")()`. It performs a one-time global mutation, must run before anything else, and evaluates no code, so it works under a strict CSP. Libraries that monkey-patch prototypes break. Whether it runs on bare V8 at all is untested, and so is whether the vendored TypeScript compiler survives a frozen `Object.prototype`; both need a spike before the work is scheduled.

#### Module loading is withdrawn and reissued as a capability

On the server, loading a module is ungranted filesystem read, bounded to source. The resolution callback ignores the referrer and resolves against the process working directory, and the loader accepts absolute paths and `file://` specifiers, so any module can import any parseable `.ts` or `.js` file anywhere on disk and read its exports. Unparseable files yield a compile error rather than their contents, which also makes the loader an existence oracle for arbitrary paths.

It confers no other authority. A module loaded this way lands in the same isolate with the same empty global, so it can neither fetch nor exec, and arbitrary code gains nothing over arbitrary computation. It does gain entry into the program from outside whatever set the lockfile hashes, which is an integrity problem on top of the read.

In the browser the answer changes completely. There `import()` is a network fetch, so it is ungranted network authority, and it is the one construct section 5's transform cannot bind, because it is syntax rather than an identifier.

So the syntax goes, on both targets. Allowing it on the server and rejecting it in the browser would give the same construct different meanings per target, which is the split this document exists to remove. On the server, leaving `host_import_module_dynamically_callback` unregistered makes V8 reject `import()` by itself, which fails closed. In the browser the transform rejects the syntax at build time. `import.meta` goes the same way and for the same reason.

What replaces it is an ordinary capability:

```ts
const loader = await myco.modules.requestImport("./plugins");
const plugin = await loader.import("formatter");
```

Prefix-scoped like `requestFetchPrefix`, narrowable like anything else, and vendable by both powerboxes. Loaded code still holds no ambient authority, so the grant covers what code enters the program rather than what that code may do, and the two compose without further machinery.

A prefix does not tell a browser build what to ship. On the server the grant is enumerable by listing the directory it names; in the browser there is no directory to list, and `loader.import(someRuntimeString)` is unbounded. Either the inner specifier is static too, which is a much stronger constraint than prefix-scoping, or the browser build needs its own manifest. That is unresolved.

The cost is that third-party packages using `import()` need a shim, alongside those already needing one for `require` and `process`.

### 5. Browser confinement

Bare V8 gives Myco deny-by-default at no cost. `v8::Isolate::new(Default::default())` plus a context yields plain ECMA-262, where `fetch`, `XMLHttpRequest`, and `WebSocket` are absent because nobody added them. A browser realm has all of them, and `delete globalThis.fetch` is not a boundary, since references leak back through iframes, `Function("return this")()`, and constructor walks.

Intrinsic hardening is already covered by `lockdown()` (section 4). What remains is scope: making a module see only the names it was given. `Compartment` would do that, but its evaluator is built on the native `Function` constructor and so requires `unsafe-eval`. Scope comes from the build instead.

Each module is transformed into a function literal whose parameters are its endowments and its imports. A loader instantiates them in dependency order. Nothing turns a string into code at runtime, so a strict CSP holds.

```js
// per-module output, sketch
export const deps = ["vendor/@myco/std/src/index.ts"];
export default (myco, imports) => { /* module body */ };
```

This is the shape LavaMoat emits for webpack.

The constraint is on code *generation*, not on loading. Measured in Chrome under `Content-Security-Policy: script-src 'self'; object-src 'none'; base-uri 'none'`: `new Function('return 1')` and `eval('1')` both throw `EvalError`, which is what rules `Compartment` out. Native `await import('./mod.js')` succeeds, and so does re-importing the same specifier with a cache-busting query. Dynamic import is governed by `script-src`, not by `unsafe-eval`.

Shipping `unsafe-eval` instead would weaken the boundary the browser enforces in order to obtain the one a shim enforces. It would also close browser extensions permanently, since Manifest V3 forbids it outright.

#### The transform defines the module environment

A module's environment is exactly its parameter list. Three categories bind to real values: endowments, imports, and a fixed set of frozen intrinsics. Every other free identifier binds to `undefined`.

The intrinsic set is a trusted, hand-maintained artifact. `Object`, `Array`, `JSON`, `Math`, and `Promise` have to be in it or no module runs, and every name after that (`URL`, `structuredClone`, `TextEncoder`) is a confinement decision someone makes by hand. The allowlist is endowments plus intrinsics, not endowments alone.

It is also target-divergent. `console`, `TextEncoder`, `TextDecoder`, and `TOML` are installed by the Myco runtime on the server, and two of them are browser-native, so the set a module may reach without a grant differs between targets, inside the mechanism meant to make targets interchangeable. What belongs in it, and whether the targets can be made to agree, is open.

Binding unrecognized names to `undefined` reproduces what a `Compartment` would have done, where such a name resolves against an empty global. It is also what lets non-Myco-aware packages work, since `typeof require !== "undefined"` takes its fallback branch instead of failing to build. A hard-fail policy would reject the vendored TypeScript compiler, which the toolchain itself depends on. Packages needing a real `process` or `require` need a shim, and the vendoring build step is where those already live: the `@myco/typescript` build script rewrites TypeScript's CJS export today.

`globalThis` binds like anything else, to a per-module object carrying only that module's endowments. Left unbound it resolves to the real browser global and the scheme collapses.

Measured in the same fixture: a module wrapped as `(endowments) => typeof fetch`, loaded with native `import()`, returns `"function"`. Free identifiers resolve up the real scope chain unless the transform binds them.

So the analysis has to be complete rather than merely strict, and the failure direction is unforgiving: a missed identifier silently reconnects to the real global and nothing at runtime notices. The backstop is the allowlist above, not a denylist of dangerous names. `navigator.sendBeacon`, `Worker`, `EventSource`, `Image`, `indexedDB`, `caches`, and `open` are all exfiltration or fresh-realm paths, and the reachable surface is far larger than any hand-written blocklist.

Two things escape identifier binding entirely. `this` is not an identifier, and inside a function invoked as `fn(endowments, imports)` in sloppy mode it is the real global, which is exactly the pattern UMD headers exploit. The wrapper has to be strict-mode or bind `this` explicitly. `import()` and `import.meta` are syntax, so the transform rejects them outright, per section 4.

SES and LavaMoat avoid the analysis entirely. They wrap module bodies in `with` over a proxy whose `has` trap always returns true, so no free identifier can escape and nothing has to be enumerated. That requires sloppy mode, and ES modules are always strict, so the technique is unavailable here. The analysis burden is the price of the CSP position.

This puts the transform in the trusted computing base, where the server-side loader never was. On the server, confinement comes from the global being empty and needs no analysis to hold.

#### Bundling is a performance concern

Confinement comes from the function wrapper, not from concatenation. The same transformed modules can be served individually and loaded with native `import()`, or concatenated into one file, with identical security properties.

That splits the work. The security-critical piece is the per-module transform: transpile, wrap, bind, emit. Concatenating the results afterward is an ordinary optimization carrying no confinement burden, and it can land whenever request overhead starts to matter.

It also means there is a dev loop. A dev server runs the transform per file and the browser re-imports with a cache-busting query, so editing one module does not rebuild the rest. Dead-code elimination cannot work per-file, so the two paths have to agree on what they drop or a module survives in development and vanishes in production.

#### Delivery

The Rust side is a build step. SWC does the transpiling and the wrapping; `myco build --target browser` runs it across the graph and a dev server runs it per file on demand. Module-graph resolution is partly reusable: `module_resolve_callback` in `cli/src/run/modules.rs` handles alias lookup and relative joining, and directory-to-index resolution lives in `load_and_compile_module` in the same file, which tries four candidate filenames.

The JS side is a small vendored runtime that calls `lockdown()`, builds a `Myco.Browser` powerbox, and drives the loader. It holds the powerbox outside the module graph, as Rust does on the server. The application supplies its own HTML entry point.

### 6. `sync`, scoped per capability kind

`sync` exists only where the underlying resource supports it. Filesystem and exec tokens carry a `sync` block; `Http` tokens do not.

That split is local versus remote, and it is the price of portability. A capability both targets can implement is one whose browser backing is IndexedDB or the network, and both are async, so target-neutral interfaces are async-only in practice. `localStorage` is the one synchronous option, and it is too small and string-only to serve as the portable store.

`sync` has exactly one non-test consumer, and it is exempt. `@myco/check`'s `sys` builds a `ts.System`, an interface synchronous by design, from a `ReadWriteDirToken`. No async portable store can satisfy those signatures, so `@myco/check` stays on `Files` and stays server-only, which is acceptable for a build tool. Every other call site is in a test of the `sync` feature itself.

Synchrony is not the only axis, and conflating them hides work. `test-suite` is already fully async and is blocked on `exec`, which has no browser analogue at all. A candidate for portability has to clear both independently.

`sync` is a bet that a capability is local and in-process. Any future capability whose backing might be remote gets no `sync` block, exactly as `FetchToken` does today.

### 7. The type environment

`lib` in tsconfig tells TypeScript which globals to assume exist. It has no effect on the runtime. `Array`, `Map`, `Math`, `JSON`, `Date`, `Proxy`, and `Reflect` are ECMA-262 and present the moment a V8 context exists. There is nothing to turn on, and section 4 explains why there is nothing worth turning off.

What matters is that the type environment models the runtime environment. If `lib` describes globals that are not there, the type checker stops being a reliable guide to what will run. Three small fixes:

`runtime/.myco/myco.d.ts` declares an ambient `setTimeout` the runtime never installs; timers are reachable only through the powerbox. It is also typed wrong against its own replacement, returning `void` where the powerbox method returns `number`. Delete the declaration.

Pin `lib` to a concrete ES year such as `es2023` or `es2024`, matching the shipped V8. `esnext` drifts ahead of what the pinned `v8` crate implements, so a call can typecheck green and then throw at runtime. `target` is already `es2022` while `lib` is `esnext`, so the two disagree today.

`getDefaultLibFileName(options)` in `lib/@myco/check/src/wrapper/host.ts` ignores its argument and returns a hardcoded `"lib.esnext.d.ts"`, so editing the tsconfig defaults changes only what editors see and leaves `myco check` unaffected. Make it honor `options`.

No custom lib file is needed. `lib: ["esnext"]` already excludes `dom`, which is the only exclusion either target requires. This is a generated default rather than an invariant: `myco.toml` overrides replace non-object values wholesale, so a project can set `lib` to anything it likes.

## Threat model

Myco has never had one written down, and several claims in this document depend on one existing.

**What it defends.** A dependency cannot obtain authority it was never handed. That is the Log4Shell shape: a library with a legitimate reason to exist reaching for a capability nobody meant to give it.

**What it does not defend against.**

- **Availability.** A library given any capability can hang the process or exhaust memory. Neither runtime offers termination or a memory quota.
- **Misuse of granted authority.** A capability is used at the grantee's discretion. Attenuation bounds the blast radius; it does not police behavior inside it.
- **Confused deputy.** A library holding a broad token, called by a less-privileged caller, performing the privileged operation on its behalf. The deputy holds that authority legitimately, so attenuation does not address it. Myco bounds the damage to whatever `main` granted; it does not prevent the pattern.
- **Side channels.** Spectre defeats software compartmentalization with no comprehensive software mitigation, timing channels are unclosable in any case, and SES does not isolate the stack between compartments.
- **Intrinsic integrity**, until section 4 lands.

**What has to be trusted.**

- **The transform**, on the browser target. It is what enforces module scoping, so a bug that emits an unbound free identifier silently reconnects a module to the browser global. Its intrinsic allowlist is hand-maintained and carries the same weight. Being first-party, it has no verification story beyond ordinary review.
- **The SES shim.** Browser confinement is enforced by `lockdown()` rather than by the absence of an API, and the lockfile integrity hashes that cover vendored dependencies should extend to it.
- **Every implementation of a revocable capability.** Section 3's rule that returned handles must be facet-bound holds transitively through whole object graphs and nothing types it. Section 2 wants third parties implementing portable interfaces, so this trust extends to code neither written nor reviewed here.

The server boundary is stronger in kind rather than degree: `fetch` does not exist because nobody added it to V8, not because something removed it.

## Known asymmetries

**Network reach.** A browser `Http.FetchPrefixToken` can be granted and still fail at runtime, so the capability is necessary without being sufficient. CORS is the obvious case, and the type system does not model it and should not, since the policy lives on the remote server. The same applies to opaque responses, restricted request and response headers, and cookie and same-origin rules. There is also no browser equivalent of arbitrary TCP, so anything shaped like a socket capability is server-only.

**No browser `exec`.** `Files.ExecToken` has no counterpart, so anything built on subprocess execution is server-only by construction.

**Bundle size.** A shared module can pull a server-only dependency into a browser build. For Myco-native code this costs bytes and nothing else: no ambient authority means top-level code cannot do I/O, so a server-only module has nothing to fail at during import. Non-Myco-aware vendored code is the exception, since a package whose top level calls `require(...)` gets `undefined` and throws rather than falling through to feature detection.

Tree-shaking is the usual answer and section 5 makes it harder. Wrapping every module body in a function is close to the worst case for export-level liveness analysis, and it makes side-effect freedom undecidable, so a bundler cannot drop a module it cannot prove inert. Whatever elimination the build performs probably has to happen before the wrapping rather than after it.

## Migration

Two changes of very different size.

The rename is mechanical, and it is two renames rather than one. `(myco: Myco)` becomes `(myco: Myco.Server)`, and `Myco.Files` splits, since it currently serves as both the requests interface holding `requestRead` and friends and the namespace holding token types. It touches most of the TypeScript in the tree, including test fixtures.

Attenuating those signatures is not mechanical. Every function that takes the powerbox only to use authority needs its actual requirements worked out and threaded as tokens. Until that happens, `Myco.Server` is stamped across the whole tree and no existing library is portable.

Two members do not survive the split as written. `Myco.Files` carries `cwd()` and `chdir()`. `chdir` is unrevocable authority to change the meaning of every relative path for every holder, and it has no browser analogue. `cwd` has no browser analogue either, and it leaks a host path to anything holding the powerbox; if a portable equivalent is wanted it belongs on a directory token rather than on the requests interface.

## Out of scope

- **Rendering.** The `Dom` capability shape, and how a renderer comes to hold one, belong in a follow-up document. The useful precedent is that React, Vue, and Solid all expose an injected-host renderer API for their native targets, and that interface has the same shape as the `ts.System` pattern `lib/@myco/check` already uses.
- **Bundling.** Concatenating transformed modules into fewer files is a performance optimization, for the reasons given in section 5. It can land when request overhead starts to matter.
- **Distributed capabilities.** Noted here because they are the constraint governing future `sync` decisions.

## Unresolved

- Whether `sync` earns a place in the public surface. It has one consumer and that consumer is exempt from portability, so there is little evidence either way about future pressure on it.
- Is browser persistence a narrowing of some DOM or storage capability, or a capability kind of its own? The answer depends on the DOM capability's shape, so it waits for the rendering document.
- `cli/src/run/inspector.rs` sets the V8 inspector client trust level to `FullyTrusted`, bypassing the capability model wholesale. This falls outside the present work but is currently undocumented as an accepted risk.

## Phasing

1. Define a candidate portable interface and hand-implement it twice: over a filesystem token, and over an in-memory `Map`.
2. **Minimal shared-code proof.** One non-trivial function exercised against both implementations. No SES, no browser, no transform.
3. Narrowing on the token, returning a facet and revoker pair from the start so the signature does not change later.
4. Attenuate the existing tree: thread tokens instead of the powerbox through `@myco/check` and `test-suite`.
5. Namespace restructure and `Myco.Server` migration.
6. Harden intrinsics on the server, after a spike confirming `lockdown()` runs on bare V8 and the vendored TypeScript compiler survives it.
7. Withdraw dynamic `import()` and `import.meta`, and add the loader capability.
8. Revocation: wire the revoker from step 3, and expose `invalidate_token` for root revocation.
9. Type environment: delete the phantom `setTimeout`, make `getDefaultLibFileName` honor `options`, pin `lib` and `target` together.
10. Browser transform: reuse module-graph resolution, bind free identifiers per module, reject import syntax, emit the wrapped function.
11. `@myco/browser` runtime: `lockdown()`, bootstrap, loader, and a `Myco.Browser` carrying `Http` only.
12. Browser proof of the capability layer: a shared library exercised from a `Myco.Browser` entry point over `Http`. This demonstrates the powerbox split, the transform, and the loader. It does not demonstrate a browser application, which needs `Dom` from the rendering document.

Steps 1 through 9 do not depend on the browser target.

Steps 1 and 2 come first because everything after them assumes an interface implementable on both sides, and that is cheap to check. Neither needs the rename, the attenuation pass, or narrowing, and between them they surface the async cost from section 6 and the path binding from section 2 while both are still cheap to act on.

No existing code can serve as the subject. The only capability-using consumers are `@myco/check`, disqualified in section 6, and `test-suite`, which fails independently on `exec`. Everything else in the tree holds no capabilities, which makes it trivially portable and therefore no evidence about a thesis concerning capability injection.

So step 2 writes a new capability-using library, and its conclusion is correspondingly weaker: it tests whether the interface from step 1 is implementable on both sides, not whether existing code becomes portable. The stronger claim stays untested until step 5 attenuates something real.

Attenuation precedes the rename because attenuating deletes most of the references the rename would otherwise have to touch.

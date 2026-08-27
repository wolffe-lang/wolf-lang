# Wolf Language Specification — 09: Constant-Time

Status: normative, v1 (campaign c28, sprint s112). Anchors `[ct.*]`.
Implemented by the WIR taint verifier (`wolf_wir::ct`); consumed by
`std.x.crypto`'s obligation ledger (D53's retrofit clause). Evidence:
`.docs/planning/02-decisions.md` D53; the `ct:` obligation lines in
wolf-std's sha2/chacha20 modules are the consumer-side requirements
this section makes checkable.

The claim this section buys: inside a marked function, data derived
from a secret cannot decide a branch, index memory, pick a call
target, or feed a variable-time instruction — refused at compile
time, fail-closed. No mainstream systems language verifies this in
core; wolf's crypto is native (D53), so the compiler owns the whole
pipeline and can.

---

## §1 The attribute `[ct.attr]`

- `[ct.attr.fn]` `#[consttime]` marks a **function item** as subject
  to the taint discipline of §2. It parses through the ordinary
  attribute grammar (01 §2.7 `[gram.item.attr]`) — no new grammar, no
  new keywords, no type-system change. On any other construct the
  name is inert, matching the general unknown-attribute posture.
- `[ct.attr.secret]` Within a consttime function every parameter is
  **secret by default**. Locals derive by propagation (§2); nothing
  else is a source. Secrecy is a property of the *value's data*, not
  of a type: there is no secret type in v1.
- `[ct.attr.public]` `#[consttime(public(a, b))]` exempts the named
  parameters: they are **public** sources, and control flow or
  indexing derived from them alone is licensed. This is the spelling
  for lengths, counts, and domain separators. The argument rides the
  existing structured `attr_arg` production; naming a parameter the
  function does not declare is refused (E1607) — a misspelled
  exemption must not silently strengthen the contract while the
  author believes it weakened it.
- `[ct.attr.carry]` The contract is part of the lowered function: WIR
  carries the marking and the secret-parameter set in its canonical
  textual form, so the contract is hash-bearing (D8) and survives to
  every consumer of the IR. Callee contracts at a call site are read
  from this carried form (§2 membrane).
- `[ct.attr.barrier]` A consttime function is an **optimization
  boundary**: the mid-end neither inlines it into callers nor inlines
  callees into it, and the release tier emits it `noinline`. The
  verified body is the unit the guarantee attaches to; dissolving it
  into a caller would detach the guarantee from the code that runs,
  and inlining a callee into it would erase the membrane call the
  verifier rules on.

---

## §2 The taint discipline `[ct.taint]`

Taint rides **values** — the SSA values of the function under
verification. Facts and instruction metadata never carry taint:
a range fact *about* a key byte is knowledge, not program data, and
tainting it would poison every analysis that touches a secret.
Effect tokens (`mem.rN`, `io`) never carry taint either — they
sequence memory, they do not hold it.

- `[ct.taint.source]` **Sources.** The secret parameters of the
  function ([ct.attr.secret]). A scalar secret parameter taints its
  value. A pointer-shaped secret parameter (containers, `mut`
  parameters) taints the **contents** reachable through it while the
  pointer value itself — an allocator artifact — stays public. In v1
  the contents granularity is the whole object: **any load reached
  through a secret root yields a secret value**, lengths and interior
  pointers included. The spine/leaf refinement (public container
  header, secret elements — the shape the std crypto headers assume)
  is named residue, not built; until it lands, secret material is
  passed as scalars and public containers carry public data.
- `[ct.taint.prop]` **Propagation.** Every value-producing operation
  propagates: arithmetic (checked, wrapping, saturating), bit
  operations, compares, conversions, aggregate make/get
  (field-granular where the aggregate op set is), error-union ops,
  moves, and block-parameter joins — the result is secret iff any
  operand is. A store of a secret value marks the stored-into
  object's contents secret; subsequent loads from that object yield
  secret. Calls propagate per the callee's contract: a consttime
  callee's result and writable arguments become secret iff any
  argument passed to it was; a public parameter of a consttime callee
  accepts only untainted arguments (a public parameter is a license
  to branch — handing it a secret would launder taint through the
  callee's exemption).
- `[ct.taint.membrane]` **The membrane.** A call that would carry a
  secret argument (by value or by contents) into a callee that is
  **not** consttime — ordinary functions, runtime helpers, the
  allocator — refuses (E1605). The callee's timing behavior is
  outside the contract; fail-closed means the secret does not cross.
  An indirect call's contract is unknowable, so a secret argument to
  an indirect call refuses the same way. Allocation with a
  secret-derived size or count is a membrane crossing into the
  allocator and refuses identically.
- `[ct.taint.sink]` **The refusal list.** Each sink is its own
  diagnostic, refused at verification:
  - **E1601** — a conditional branch on a secret condition. WIR has
    no value-select opcode; every conditional transfer is a branch,
    so this single rule covers if/while/match reachability and any
    select-shaped lowering.
  - **E1602** — a load or store whose address derives from a secret
    (a secret-derived index or offset). A bounds-guard branch whose
    sole purpose is to protect a secret-indexed access classifies
    here, not under E1601: the index is the sin, and the diagnostic
    should name it.
  - **E1603** — an indirect call whose target derives from a secret.
    Which function runs is observable through time and the
    instruction cache.
  - **E1604** — integer division or remainder with a secret operand,
    in any arithmetic profile. Hardware divide latency is
    operand-dependent on mainstream cores; no software spelling of
    `/` or `%` on a secret is constant-time, so there is no fix-it —
    the algorithm must avoid the operation (multiply by inverse,
    Barrett/Montgomery shapes).
  - **E1606** — checked arithmetic on secret operands. The checked
    profile's overflow trap is a secret-dependent branch **by
    construction** — the collision between checked-by-default and
    this tier is real and is resolved by refusal, not exemption. The
    fix-it names `wrapping[T]`, which is branch-free and is the form
    the std crypto kernels already ride. A checked operation whose
    trap is *provably* dead still refuses in v1 — the proof machinery
    elision is named residue. Saturating arithmetic lowers
    branch-free and is permitted.
- `[ct.taint.declassify]` **No declassification.** Taint is one-way:
  no operation, cast, or annotation lowers a secret value to public
  inside a consttime function. The single sanctioned boundary is the
  return value crossing back to a non-consttime caller, where the
  caller's code is outside the contract (the accumulate-then-
  single-check idiom puts the one deciding comparison there). A
  `declassify` operation is an open design question, deliberately not
  in v1.
- `[ct.taint.verify]` **When it runs.** The verifier runs over the
  constructed WIR and again over the **final pre-emission** form,
  after every mid-end transform — so an optimization that introduces
  a secret-dependent branch, index, or call is refused by the second
  run rather than trusted pass by pass. The two runs must agree on
  clean programs; a second-run-only refusal is a compiler finding.
  Every execution lane refuses identically; a lane that cannot run
  the verifier must refuse the program rather than execute an
  unverified secret path.
- `[ct.taint.gap]` **The WIR-vs-asm gap, stated.** Verified WIR is
  not yet guaranteed assembly: late lowering (the LLVM tier's
  instruction selection and its own transforms) may re-introduce a
  conditional branch from a branch-free form. v1's posture: the WIR
  verifier is the semantic gate, and **assembly witnesses** pin the
  flagship shapes — a tag-compare accumulate-then-single-check kernel
  and an arithmetic conditional-swap kernel are disassembled from the
  release binary and asserted to contain zero conditional-branch
  opcodes in their bodies. A witness that breaks names the exact
  transform, and the mitigation lands measured. The gap closes shape
  by shape; it is never silently assumed closed.

Non-normative residue ledger (named, not built): the spine/leaf
container granularity; the proven-no-trap checked-op elision; a
`declassify` operation (D-question); parameter-position attribute
spelling for `public`; float arithmetic timing (floats are outside
the modelled surface — subnormal operands are variable-time on real
hardware); empirical timing measurement (dudect-class, a bench-track
question); the std crypto modules' annotation rung, which lands
against this section from the std track.

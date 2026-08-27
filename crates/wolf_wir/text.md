# WIR textual format (s24)

The textual format is the interchange and test surface for WIR: every
pass is forever dumpable, `.wir` fixtures are how s25+'s tests are
written, and the canonical form is the input to D8 content hashing.
The printer (`src/print.rs`) and parser (`src/parse.rs`) live together
in `wolf_wir`; `print → parse → print` reaches a byte-identical
fixpoint for every module (property-tested).

## Lexical

- `;` starts a comment, to end of line. Lines are significant:
  one declaration, fact, block header, or instruction per line.
- Identifiers: `[A-Za-z_][A-Za-z0-9_]*`, with dotted continuation
  (`iadd.chk`, `mem.r0`, `eu.make.ok` are single identifiers).
- Values are `%name`, functions are `@name`. In canonical output value
  names are `%0, %1, …` in definition order and blocks are `b0, b1, …`
  in reverse postorder from the entry; hand-written files may use any
  names and are normalized on the first print.
- Integers: decimal (optionally negative) or `0x…` hex (bit pattern).
  Float constants print canonically as raw IEEE-754 bits (`0x…`, 16
  hex digits for f64, 8 for f32); the parser also accepts plain
  decimal literals (`1.5`).

## Grammar

```
module   := { decl | func }
decl     := "decl" "@" name sig
func     := [ "export" ] [ consttime ] "fn" "@" name sig
            "{" NL { fact NL } block { block } "}"
consttime := "consttime" "(" [ INT { "," INT } ] ")"
            ; c28 [ct.attr.carry]: the constant-time contract — the
            ; listed signature parameter indices are SECRET (the taint
            ; sources); canonical form is sorted and deduplicated.
sig      := "(" [ param { "," param } ] ")" [ "->" type { "," type } ]
param    := [ "mut" | "read" | "take" ] type
type     := "i8" | "i16" | "i32" | "i64" | "f32" | "f64" | "bool"
          | "ptr" | "io" | "mem.r" INT
          | "{" [ type { "," type } ] "}"          ; small aggregate

block    := label [ "(" bparam { "," bparam } ")" ] ":" NL { inst NL }
bparam   := VALUE ":" type

inst     := [ results "=" ] op-line
results  := VALUE [ ":" type ] { "," VALUE [ ":" type ] }
            ; explicit result types are REQUIRED for reserved ops
            ; (their typing rules land in s26/s27) and otherwise
            ; optional (checked against the computed type).

op-line  := "iconst." sty INT | "fconst." fty FLOAT | "bconst" BOOL
          | iop VALUE "," VALUE                     ; iadd.chk isub.chk
          ;   imul.chk idiv.chk irem.chk iadd.wrap isub.wrap imul.wrap
          ;   iadd.sat isub.sat imul.sat band bor bxor shl lshr ashr
          | fop VALUE "," VALUE                     ; fadd fsub fmul fdiv
          | "fneg" VALUE | "fma" VALUE "," VALUE "," VALUE
          | "icmp." icc VALUE "," VALUE             ; eq ne slt sle sgt
          ;   sge ult ule ugt uge
          | "fcmp." fcc VALUE "," VALUE             ; eq ne lt le gt ge
          | ("sext."|"zext."|"itrunc.") sty VALUE
          | "ptr.off" VALUE "," VALUE "," INT       ; base, index, scale
          | "load." sty VALUE "," VALUE             ; addr, mem token
          | "store." sty VALUE "," VALUE "," VALUE  ; value, addr, token
          | "agg.make" VALUE { "," VALUE }
          | "agg.get" VALUE "," INT
          | "call" "@" name "(" [ VALUE { "," VALUE } ] ")"
          | "jmp" edge
          | "br" VALUE "," edge "," edge
          | "ret" [ VALUE { "," VALUE } ]
          | "trap"
          | reserved [ VALUE { "," VALUE } ]        ; region.new
          ;   region.alloc region.free rc.dup rc.drop sync.freeze
          ;   sync.transfer eu.make.ok eu.make.err eu.is_err eu.ok eu.err
edge     := label [ "(" [ VALUE { "," VALUE } ] ")" ]

fact     := "fact" "noalias" VALUE VALUE just
          | "fact" "deref" VALUE size just
          | "fact" "range" VALUE INT "..=" INT just
          | "fact" "region" VALUE "r" INT just
          | "fact" "frozen" VALUE just
size     := INT | INT "x" VALUE                     ; bytes, or elem×count
just     := ":" ( "excl.mut" | "frozen.read" | "region.alloc"
                | "op" [ VALUE ] )
```

## Semantics anchored in the format

- **Signatures carry modes** (`mut`/`read`/`take`) and list token
  params (`mem.rN`, `io`) explicitly. The entry block's parameters
  must match the signature exactly. `call` consumes its token
  arguments and binds one fresh successor token per token param,
  appended after the declared results in param order:
  `%r, %m2 = call @f(%p, %n, %m)`.
- **Facts are part of the program.** Every fact line names its
  justification: a c04 checker-theorem class (`excl.mut`,
  `frozen.read`, `region.alloc`) or a deriving op (`op` for the
  subject's own def, `op %v` to cite another). The verifier rejects
  unjustified or locally-refutable facts; there is no way to write an
  unverified `noalias` claim. Analysis spans are compiler-internal and
  are not printed.
- **Defs precede uses** in the text (block parameters are pre-declared
  by their headers, facts resolve after the whole body) — canonical
  RPO output always has this shape.
- **Reserved mnemonics** parse and print today so the format does not
  churn when s26/s27 land, but the verifier rejects them.

## Canonical form

The printer always emits: sorted `decl`s for every declared or called
name; blocks in reverse postorder (unreachable blocks last, and the
verifier rejects those anyway); values numbered in definition order
along that block order; facts sorted by their rendered line; float
constants as raw bits. Two constructions of the same function print
byte-identically regardless of insertion order (tested), which is what
makes IR dumps a snapshot family and gives D8 dedup a stable hash
input.

## Example

```
fn @dot(ptr, ptr, i64, mem.r0) -> f64 {
  fact deref %0 8x%2 : excl.mut
  fact deref %1 8x%2 : frozen.read
  fact noalias %0 %1 : excl.mut
b0(%0: ptr, %1: ptr, %2: i64, %3: mem.r0):
  %4 = iconst.i64 0
  %5 = fconst.f64 0x0000000000000000
  jmp b1(%4, %5)
b1(%6: i64, %7: f64):
  %8 = icmp.slt %6, %2
  br %8, b2, b3
b2:
  %9 = ptr.off %0, %6, 8
  %10 = load.f64 %9, %3
  %11 = ptr.off %1, %6, 8
  %12 = load.f64 %11, %3
  %13 = fma %10, %12, %7
  %14 = iconst.i64 1
  %15 = iadd.chk %6, %14
  jmp b1(%15, %13)
b3:
  ret %7
}
```

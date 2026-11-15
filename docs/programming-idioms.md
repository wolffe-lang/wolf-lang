# Loops, queries and guards

Use a range when the bounds are fixed for the walk. Use `for x in xs`
when you need the elements, and `for i in 0..xs.len` when you need indices.
A range stores endpoints; it does not build a list of indices. The upper
bound of `0..n` is exclusive; `0..=n` includes `n`.

```wolf
fn index(xs: List[int], needle: int) -> int {
    for i in 0..xs.len {
        if xs[i] == needle then return i
    }
    -1
}
fn main() {
    var xs = [2, 4, 6]
    for i in 0..xs.len {
        xs[i] += 1
    }
    var total = 0
    for i in 0..6 {
        if i == 1 then continue
        total += i
    }
    print("{index(xs, 7)} {total} {index([], 1)}")
}
```

The program prints `2 14 -1`. The range advances even when `continue`
skips the rest of the body. An empty range has no iterations. Its bounds
are evaluated once, before the walk, unlike a `while` condition, which is
tested again each turn.

Keep `while` when the cursor changes by more than one, the next turn depends
on a changing bound, or the cursor must survive the loop. A byte scanner,
a retry loop and `while out.len < target` need not become ranges. Do not
mechanically replace `while i < xs.len` if the body grows `xs`: the range
captures its original length. Iterating elements also lends the collection;
use an index range when you need to update existing elements, not a live
element iteration plus mutation of its collection.

## Query with the shipped combinators

The bundled standard library supplies these methods. They are eager:
`filter(...).map(...)` builds an intermediate list and then a result list,
not a lazy pipeline. Use the loop when you need one pass, side effects,
fallible element work with `?`, or explicit control of copying.

```wolf
fn main() {
    let xs = (0..6).collect()
    let ys = xs.filter(fn(x: int) { x % 2 == 0 }).map(fn(x: int) { x * 10 })
    let total = ys.fold(0, fn(a: int, x: int) { a + x })
    let down = ys.sorted_by(fn(a: int, b: int) { a > b })
    let up = down.sorted()
    var work = copy down
    (mut work).sort_by(fn(a: int, b: int) { a < b })
    let indexed = ys.enumerate()
    let paired = ys.zip([7, 8])
    print("{total} {ys.sum()} {down[0]} {up[0]} {indexed[1].1} {paired.len} {paired[1].1}")
}
```

The program prints `60 60 40 0 20 2 8`.

| Method | Result and cost |
|---|---|
| `map(f)` | A new list of transformed elements; one call per element |
| `filter(keep)` | A new list of retained elements in input order |
| `fold(seed, step)` | A left-to-right reduction; no result list of its own |
| `sum()` | Checked addition of a `List[int]` |
| `(mut xs).sort_by(less)` | Stable in-place sort with scratch storage |
| `sorted_by(less)` / `sorted()` | A sorted copy; `less(a, b)` returns `bool`, not `Ordering` |
| `enumerate()` | A new list of `(index, value)` pairs, starting at zero |
| `zip(ys)` | A new list of pairs, stopping at the shorter input |
| `(a..b).collect()` | A materialized list; plain range iteration does not need it |

`any`, `all`, `count_where`, `index_where`, `find_where`, `min_by` and
`max_by` are also callable as list methods. The last four answer the `none`
row when no result exists. `par(f)` is the parallel combinator: it creates
tasks, preserves result order and has its own failure and capture rules.
It is not a free replacement for a serial `map`.

The receiver is read, not consumed. Keeping heap-valued elements can copy
their contents: `filter`, sorting copies, `enumerate` and `zip` are not
zero-copy views. `fold` copies its seed where needed. A chain over nested
lists can therefore cost much more than a chain over integers or strings.
Use the archive's bundled `std`, or explicitly select a matching std root;
a compiler built without it is not evidence that these methods are absent.

## One action, one guard

Write `if condition then statement` for a one-statement guard. It works
with a call, `return`, `break` or `continue`. Use braces for declarations,
assignments and multiple actions. Do not replace it with `condition &&
action()`: the logical operators take booleans.

```wolf
fn positive(n: int) -> int ! {negative} {
    if n < 0 then return negative
    n
}
fn twice(n: int) -> int ! {negative} {
    positive(n)? * 2
}
fn or_zero(n: int) -> int {
    let value = twice(n) else return 0
    value
}
fn main() {
    print("{or_zero(-1)} {or_zero(3)} {twice(-1) else 0}")
}
```

The program prints `0 6 0`. `?` propagates the failure row out of the
current function, whose signature must admit it. `else return 0` handles
a failure locally and returns an ordinary success value from this
function. `else 0` instead substitutes a value and keeps evaluating here.
Use the operation's row directly rather than unpacking it into a boolean
and writing a second early-return test. Use `else |err| ...` when the
recovery needs the error value.

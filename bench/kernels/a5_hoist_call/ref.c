/* a5-hoist-call (family A), NAIVE C: two loads of the same location on
 * either side of a call that is meant to be opaque.
 *
 * ==== s79 baseline audit: THIS KERNEL DOES NOT TEST ITS THESIS ====
 *
 * The kernel's comments used to claim "`opaque` is self-recursive so
 * nothing inlines it away in any lane". Both halves of that are false in
 * the compiled programs, and had been for all five measurements taken of
 * this kernel:
 *
 *   - clang -O3 solves the recursion's closed form, proves the callee
 *     touches no memory, HOISTS BOTH LOADS and vectorizes the loop. The
 *     naive binary contains no call and no load from `src` (checked with
 *     objdump), runs 0.177 ns/op, and matches `expert.c` — the lane that
 *     hoists by hand — to within 3%.
 *   - wolf's release tier does exactly the same thing: `probe` and
 *     `opaque` are inlined, the recursion is folded, and the load is
 *     hoisted out of the loop. The wolf binary has no call in its timed
 *     region either.
 *
 * So the reported 0.172x was never "wolf reloads where C hoists". It was
 * one folded loop against another: clang's vectorized, wolf's a scalar
 * chain of checked adds.
 *
 * Three same-file fixes were tried and all three failed: a run-time
 * recursion depth (clang solves it symbolically in `depth`), a `volatile`
 * function pointer (the only address-taken function in the module is
 * `readnone`, so the indirect call is still provably harmless), and
 * publishing `src`'s address to a global alias (does not matter while the
 * callee is provably readnone). Moving the callee to its own translation
 * unit DOES restore the call and the reload — measured, 0.219 -> 1.31
 * ns/op — but it cannot be done on the wolf side: wolf compiles
 * whole-program into a single module, has no separate-compilation
 * surface, no `noinline`, and Tier-R refuses `func.addr`, so there is no
 * indirect call either. Fixing only the C lane would hand wolf a win
 * manufactured by unequal work, which is the exact failure this suite
 * exists to prevent.
 *
 * The file is therefore left as it was and the label is corrected
 * instead: today this kernel measures a folded arithmetic loop in both
 * lanes. What it would take to measure CSE-across-calls is written up in
 * bench/loss-ledger.md under G7.
 *
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static int64_t opaque(int64_t depth, int64_t x) {
    if (depth <= 0) return x & 1023;
    return opaque(depth - 1, x + 1) & 1023;
}

static int64_t probe(const int64_t *src, int64_t n) {
    int64_t acc = 0;
    for (int64_t i = 0; i < n; i++) {
        int64_t a = src[0];
        int64_t side = opaque(1, i);
        int64_t b = src[0];
        acc = (acc + a + b + side) & 1048575;
    }
    return acc;
}

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 2000;
    const int64_t inner = 10000;
    static int64_t src[2] = {7, 9};
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t k = 0; k < ops; k++) sink = (sink + probe(src, inner)) & 1048575;
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%lld,\"sink\":%lld}\n",
           (unsigned long long)ns, (long long)(ops * inner), (long long)sink);
    return 0;
}

/* a5-hoist-call (family A), NAIVE C: loads of src[0] on either side of a
 * call that stores through a second pointer, where neither pointer's
 * provenance is provable — both are laundered through volatile globals
 * (the suite's established escape idiom, b3_churn's `escaped`). The
 * store may alias src, so clang must reload src[0] and re-store
 * scratch[0] every iteration. That is what naive C code with pointers
 * "from somewhere" pays; wolf's `read`/`mut` modes prove the same two
 * buffers disjoint from the signature alone.
 *
 * ==== #97 redesign (the s79 audit, resolved) ====
 *
 * The original callee was a PURE self-recursion and this file's audit
 * documented the result: clang -O3 solved the closed form, proved the
 * callee readnone, hoisted both loads, vectorized, and the "opaque
 * call" survived in neither lane — five measurements of one folded
 * arithmetic loop against another. Three same-file fixes failed
 * (run-time depth: solved symbolically; volatile fn pointer: the only
 * address-taken function was readnone, indirect call provably
 * harmless; publishing src's address: irrelevant while the callee
 * touches no memory). The two-translation-unit fix worked (0.219 ->
 * 1.31 ns/op) and was deliberately backed out: wolf compiles
 * whole-program, so fixing only the C lane manufactures a wolf win.
 *
 * The resolution is the one #97 prescribed: the callee WRITES MEMORY.
 * A store the optimizer must respect is opaque in every lane by
 * construction, and the kernel finally measures its thesis — CSE
 * across an opaque write, licensed by modes in wolf, forbidden by
 * may-alias here, asserted by `restrict` in expert.c.
 *
 * At runtime the two buffers are distinct, so every lane executes the
 * same arithmetic; only what the compiler may PROVE differs.
 *
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static int64_t data[2] = {7, 9};
static int64_t scr[2] = {0, 0};

/* Provenance laundering: the pointers the timed code uses are read out
 * of volatile globals, so clang cannot prove they address distinct
 * objects and must honour the may-alias store. */
static int64_t *volatile data_p = data;
static int64_t *volatile scr_p = scr;

static int64_t bump(int64_t *dst, int64_t x) {
    dst[0] = (dst[0] + x) & 1023;
    return dst[0];
}

static int64_t probe(const int64_t *src, int64_t *scratch, int64_t n) {
    int64_t acc = 0;
    for (int64_t i = 0; i < n; i++) {
        int64_t a = src[0];
        int64_t side = bump(scratch, i);
        int64_t b = src[0];
        acc = (acc + a + b + side) & 1048575;
    }
    return acc;
}

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 2000;
    const int64_t inner = 10000;
    const int64_t *src = data_p;
    int64_t *scratch = scr_p;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t k = 0; k < ops; k++)
        sink = (sink + probe(src, scratch, inner)) & 1048575;
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%lld,\"sink\":%lld}\n",
           (unsigned long long)ns, (long long)ops, (long long)sink);
    return 0;
}

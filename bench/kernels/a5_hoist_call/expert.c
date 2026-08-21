/* a5-hoist-call (family A), EXPERT C: same laundered pointers as ref.c
 * — the expert starts from the same information-free provenance — but
 * asserts disjointness by hand with `restrict`, which is exactly the
 * claim wolf's `read`/`mut` modes prove from the signature. clang may
 * then hoist the src load and keep the scratch chain in a register:
 * the transform the kernel is named for, written by a human.
 *
 * (#97 redesign; see ref.c for the history. The pre-redesign expert.c
 * hand-hoisted a load both compilers were already deleting.)
 *
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..} where
 * ops in the JSON is argv-ops * inner — per-iteration accounting,
 * the #115 rule ref.c's audit block records; every lane counts the
 * same unit or the harness's ns/op division is a lie. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static int64_t data[2] = {7, 9};
static int64_t scr[2] = {0, 0};
static int64_t *volatile data_p = data;
static int64_t *volatile scr_p = scr;

static int64_t bump(int64_t *restrict dst, int64_t x) {
    dst[0] = (dst[0] + x) & 1023;
    return dst[0];
}

static int64_t probe(const int64_t *restrict src, int64_t *restrict scratch,
                     int64_t n) {
    int64_t acc = 0;
    const int64_t hoisted = src[0];
    for (int64_t i = 0; i < n; i++) {
        int64_t side = bump(scratch, i);
        acc = (acc + hoisted + hoisted + side) & 1048575;
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
           (unsigned long long)ns, (long long)(ops * inner), (long long)sink);
    return 0;
}

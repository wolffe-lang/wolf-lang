/* a5-hoist-call (family A), NAIVE C: two loads of the same location on
 * either side of an opaque call. The callee could write through a global
 * alias, so clang must reload — the load wolf's `read` mode licenses it
 * to hoist. `expert.c` hoists by hand.
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

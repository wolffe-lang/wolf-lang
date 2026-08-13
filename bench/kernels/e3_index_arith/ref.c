/* e3-index-arith (family E), naive C. Scaled index arithmetic against an
 * opaque bound; `walk` is self-recursive so it does not inline in any of
 * the three languages and the bound stays a runtime value.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static int64_t walk(int64_t n, int64_t depth) {
    int64_t acc = 0;
    for (int64_t i = 0; i < n; i++) acc = (acc + i * 8) & 1048575;
    if (depth > 0) acc = (acc + walk(n, depth - 1)) & 1048575;
    return acc;
}

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 2000;
    const int64_t inner = 100000;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t k = 0; k < ops; k++) sink = (sink + walk(inner, 1)) & 1048575;
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%lld,\"sink\":%lld}\n",
           (unsigned long long)ns, (long long)(ops * inner * 2), (long long)sink);
    return 0;
}

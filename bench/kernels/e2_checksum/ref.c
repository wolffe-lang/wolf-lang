/* e2-checksum (family E), naive C. Multiply-add-mask rolling hash; signed
 * overflow is UB so clang need not prove anything.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static int64_t checksum(int64_t n, int64_t seed) {
    int64_t h = seed;
    for (int64_t i = 0; i < n; i++) h = (h * 31 + (i & 255)) & 1048575;
    return h;
}

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 4000;
    const int64_t inner = 100000;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 1;
    for (int64_t k = 0; k < ops; k++) sink = checksum(inner, sink);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%lld,\"sink\":%lld}\n",
           (unsigned long long)ns, (long long)(ops * inner), (long long)sink);
    return 0;
}

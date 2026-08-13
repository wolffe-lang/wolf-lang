/* c2-ecs-sweep (family C), EXPERT C: the hot field in its own array —
 * hand-rolled SoA, unit stride, one cache line per eight elements.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define N 100000

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 1500;
    static int64_t hot[N];
    for (int64_t i = 0; i < N; i++) hot[i] = i & 1023;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t k = 0; k < ops; k++) {
        int64_t acc = 0;
        for (int64_t i = 0; i < N; i++) acc = (acc + hot[i]) & 1048575;
        sink = acc;
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%lld,\"sink\":%lld}\n",
           (unsigned long long)ns, (long long)(ops * N), (long long)sink);
    return 0;
}

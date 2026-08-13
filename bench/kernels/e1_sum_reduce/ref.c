/* e1-sum-reduce (family E), naive C — what people actually write.
 * i64 summation with statically boundable ranges; C's signed overflow is
 * UB, so clang assumes it away and vectorizes the reduction freely. That
 * is precisely the fact wolf renounced (X3: checked arithmetic in every
 * profile), so this kernel prices the renunciation.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static int64_t sum_reduce(int64_t n) {
    int64_t acc = 0;
    for (int64_t i = 0; i < n; i++) acc = acc + (i & 1023);
    return acc;
}

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 4000;
    const int64_t inner = 100000;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t k = 0; k < ops; k++) sink = sink % 4096 + sum_reduce(inner) % 4096;
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%lld,\"sink\":%lld}\n",
           (unsigned long long)ns, (long long)(ops * inner), (long long)sink);
    return 0;
}

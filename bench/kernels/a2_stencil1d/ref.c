/* a2-stencil1d (family A), NAIVE C: 3-point stencil, distinct buffers,
 * no `restrict`. clang must assume `out` may alias `src`.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define N 4096

static void stencil(int64_t *out, const int64_t *src) {
    for (int i = 1; i < N - 1; i++)
        out[i] = (src[i - 1] + src[i] + src[i + 1]) & 1048575;
}

int main(int argc, char **argv) {
    uint64_t ops = argc > 1 ? strtoull(argv[1], 0, 10) : 20000;
    static int64_t src[N], out[N];
    for (int i = 0; i < N; i++) {
        src[i] = i & 255;
        out[i] = 0;
    }
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint64_t k = 0; k < ops; k++) stencil(out, src);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%llu,\"sink\":%lld}\n",
           (unsigned long long)ns, (unsigned long long)(ops * N),
           (long long)out[2048]);
    return 0;
}

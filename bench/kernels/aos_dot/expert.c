/* aos_dot (family C), EXPERT C: the same reduction over hand-rolled SoA —
 * one array per field, unit stride, trivially vectorizable. This is the
 * layout an expert reaches for once profiling says the stride hurts, and
 * it is the layout wolf's SoA idiom expresses without an ABI argument.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define N 100000

int main(int argc, char **argv) {
    uint64_t ops = argc > 1 ? strtoull(argv[1], 0, 10) : 1500;
    static double ax[N], bx[N];
    double va = 0.0, vb = (double)N;
    for (int i = 0; i < N; i++) {
        ax[i] = va; bx[i] = vb;
        va += 1.0; vb -= 1.0;
    }
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    double sink = 0.0;
    for (uint64_t k = 0; k < ops; k++) {
        double acc = 0.0;
        for (int i = 0; i < N; i++) acc = acc + ax[i] * bx[i];
        sink = acc;
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%llu,\"sink\":%.17g}\n",
           (unsigned long long)ns, (unsigned long long)(ops * N), sink);
    return 0;
}

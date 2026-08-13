/* alias_daxpy = a1-triad (family A), EXPERT C: the same loop with
 * hand-written `restrict` on both pointer parameters — the disjointness
 * fact wolf's `mut`/`read` param modes prove for free (D3, reports/01
 * facts 1-2). This is the secondary (report-only) comparison and the
 * scrutiny lane's source; `ref.c` is the gated naive comparison.
 *
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define N 4096

static void daxpy(double *restrict y, const double *restrict x, double a) {
    for (int i = 0; i < N; i++) y[i] += a * x[i];
}

volatile double sink;

int main(int argc, char **argv) {
    uint64_t ops = argc > 1 ? strtoull(argv[1], 0, 10) : 20000;
    static double x[N], y[N];
    double v = 0.0, w = 0.0;
    for (int i = 0; i < N; i++) {
        x[i] = v;
        y[i] = w;
        v += 0.5;
        w += 1.0;
    }
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint64_t k = 0; k < ops; k++) daxpy(y, x, 1.000001);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    sink = y[N - 1];
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%llu,\"sink\":%.17g}\n",
           (unsigned long long)ns, (unsigned long long)(ops * N), y[N - 1]);
    return 0;
}

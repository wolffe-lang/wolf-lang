/* alias_daxpy: y[i] += a*x[i] over 4096 doubles, `ops` sweeps.
 * The aliasing kernel: C only vectorizes this freely with hand-written
 * restrict — exactly the fact wolf's type system will prove for free (D3).
 * Protocol: argv[1]=ops; prints {"ns":<total>,"ops":<ops>} on stdout. */
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
    uint64_t ops = argc > 1 ? strtoull(argv[1], 0, 10) : 1000;
    static double x[N], y[N];
    for (int i = 0; i < N; i++) { x[i] = (double)i * 0.5; y[i] = (double)i; }
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint64_t k = 0; k < ops; k++) daxpy(y, x, 1.000001);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    sink = y[N - 1];
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%llu}\n",
           (unsigned long long)ns, (unsigned long long)ops);
    return 0;
}

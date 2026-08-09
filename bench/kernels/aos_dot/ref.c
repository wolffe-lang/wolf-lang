/* aos_dot: dot-product of the x-fields across a 100k array-of-structs.
 * The layout kernel: the C/Rust baseline pays the AoS stride tax; wolf's
 * layout freedom (SoA legality, I9) intends to beat it. Protocol:
 * argv[1]=ops; prints {"ns":<total>,"ops":<ops>}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define N 100000

struct p3 { double x, y, z; };

volatile double sink;

int main(int argc, char **argv) {
    uint64_t ops = argc > 1 ? strtoull(argv[1], 0, 10) : 1000;
    static struct p3 a[N], b[N];
    for (int i = 0; i < N; i++) {
        a[i].x = (double)i; a[i].y = 1.0; a[i].z = 2.0;
        b[i].x = (double)(N - i); b[i].y = 3.0; b[i].z = 4.0;
    }
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint64_t k = 0; k < ops; k++) {
        double dot = 0.0;
        for (int i = 0; i < N; i++) dot += a[i].x * b[i].x;
        sink = dot;
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%llu}\n",
           (unsigned long long)ns, (unsigned long long)ops);
    return 0;
}

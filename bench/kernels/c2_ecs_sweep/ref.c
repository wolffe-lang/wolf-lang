/* c2-ecs-sweep (family C), NAIVE C: read one hot field out of an 8-field
 * record, 100k of them. One cache line per element for one i64 of work.
 * `expert.c` hand-rolls SoA.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define N 100000

struct ent {
    int64_t hot;
    int64_t c1, c2, c3, c4, c5, c6, c7;
};

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 1500;
    static struct ent es[N];
    for (int64_t i = 0; i < N; i++) {
        es[i].hot = i & 1023;
        es[i].c1 = es[i].c2 = es[i].c3 = es[i].c4 = 1;
        es[i].c5 = es[i].c6 = es[i].c7 = 2;
    }
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t k = 0; k < ops; k++) {
        int64_t acc = 0;
        for (int64_t i = 0; i < N; i++) acc = (acc + es[i].hot) & 1048575;
        sink = acc;
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%lld,\"sink\":%lld}\n",
           (unsigned long long)ns, (long long)(ops * N), (long long)sink);
    return 0;
}

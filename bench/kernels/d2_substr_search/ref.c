/* d2-substr-search (family D), NAIVE C: sliding `memcmp` over the same
 * haystack. Deliberately not `strstr` — see bench/protocol.md on comparing
 * hand loops to hand-vectorized library routines.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static const char CHUNK[] = "the pack moves at dusk and the wolf waits for nothing at all ";
#define REPS 9000
#define M 5

static int64_t count_occurrences(const char *hay, size_t len, const char *needle) {
    int64_t hits = 0;
    for (size_t i = 0; i + M <= len; i++)
        if (memcmp(hay + i, needle, M) == 0) hits++;
    return hits;
}

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 20;
    size_t clen = strlen(CHUNK);
    size_t len = clen * REPS;
    char *buf = malloc(len + 1);
    for (int i = 0; i < REPS; i++) memcpy(buf + (size_t)i * clen, CHUNK, clen);
    buf[len] = 0;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t k = 0; k < ops; k++) sink = count_occurrences(buf, len, "wolf ");
    clock_gettime(CLOCK_MONOTONIC, &t1);
    free(buf);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%llu,\"sink\":%lld}\n",
           (unsigned long long)ns, (unsigned long long)((uint64_t)ops * len),
           (long long)sink);
    return 0;
}

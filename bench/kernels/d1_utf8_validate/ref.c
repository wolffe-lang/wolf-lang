/* d1-utf8-validate (family D), NAIVE C: the same structural UTF-8 scan,
 * byte at a time, over the same buffer.
 * Protocol: argv[1]=ops; prints {"ns":..,"ops":..,"sink":..}. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static const char CHUNK[] =
    "wolf pack ranges wide: \xc3\xa9 and \xe2\x82\xac and \xf0\x9f\x90\xba"
    " keep the scan honest. ";
#define REPS 15000

static int64_t validate(const unsigned char *p, size_t len) {
    int64_t want = 0, chars = 0;
    for (size_t i = 0; i < len; i++) {
        unsigned char b = p[i];
        if (want > 0) {
            if (b < 128 || b > 191) return -1;
            want--;
        } else if (b < 128) {
            chars++;
        } else if (b < 194) {
            return -1;
        } else if (b < 224) {
            want = 1; chars++;
        } else if (b < 240) {
            want = 2; chars++;
        } else if (b < 245) {
            want = 3; chars++;
        } else {
            return -1;
        }
    }
    return want == 0 ? chars : -1;
}

int main(int argc, char **argv) {
    int64_t ops = argc > 1 ? (int64_t)strtoull(argv[1], 0, 10) : 20;
    size_t clen = strlen(CHUNK);
    size_t len = clen * REPS;
    unsigned char *buf = malloc(len + 1);
    for (int i = 0; i < REPS; i++) memcpy(buf + (size_t)i * clen, CHUNK, clen);
    buf[len] = 0;
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int64_t sink = 0;
    for (int64_t k = 0; k < ops; k++) sink = validate(buf, len);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    free(buf);
    uint64_t ns = (uint64_t)(t1.tv_sec - t0.tv_sec) * 1000000000ull
                + (uint64_t)(t1.tv_nsec - t0.tv_nsec);
    printf("{\"ns\":%llu,\"ops\":%llu,\"sink\":%lld}\n",
           (unsigned long long)ns, (unsigned long long)((uint64_t)ops * len),
           (long long)sink);
    return 0;
}

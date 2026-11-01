/* s273 — wolf-lang#972's C bar: the witness's shape in C.
 * N inserts of strdup'd "0#key%d" keys into a chained hash map that
 * doubles its bucket array at load 1, then N lookups of freshly
 * formatted keys (a stack buffer each, as the wolf program builds a
 * fresh `str` each turn). Prints the hit count. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct ent {
    char *key;
    long val;
    struct ent *next;
};

static struct ent **buckets;
static size_t nb, count;

static uint64_t hash(const char *s) {
    uint64_t h = 1469598103934665603ull;
    for (; *s; s++) {
        h ^= (unsigned char)*s;
        h *= 1099511628211ull;
    }
    return h;
}

static void grow(void) {
    size_t nnb = nb ? nb * 2 : 16;
    struct ent **nbk = calloc(nnb, sizeof *nbk);
    for (size_t i = 0; i < nb; i++) {
        struct ent *e = buckets[i];
        while (e) {
            struct ent *nx = e->next;
            size_t b = hash(e->key) & (nnb - 1);
            e->next = nbk[b];
            nbk[b] = e;
            e = nx;
        }
    }
    free(buckets);
    buckets = nbk;
    nb = nnb;
}

static void set(const char *k, long v) {
    if (count >= nb) grow();
    size_t b = hash(k) & (nb - 1);
    for (struct ent *e = buckets[b]; e; e = e->next) {
        if (strcmp(e->key, k) == 0) {
            e->val = v;
            return;
        }
    }
    struct ent *e = malloc(sizeof *e);
    e->key = strdup(k);
    e->val = v;
    e->next = buckets[b];
    buckets[b] = e;
    count++;
}

static int get(const char *k, long *out) {
    if (!nb) return 0;
    for (struct ent *e = buckets[hash(k) & (nb - 1)]; e; e = e->next) {
        if (strcmp(e->key, k) == 0) {
            *out = e->val;
            return 1;
        }
    }
    return 0;
}

int main(int argc, char **argv) {
    long n = argc > 1 ? atol(argv[1]) : 10000;
    char buf[32];
    for (long i = 0; i < n; i++) {
        snprintf(buf, sizeof buf, "0#key%ld", i);
        set(buf, i);
    }
    long hits = 0;
    for (long i = 0; i < n; i++) {
        long v = -1;
        snprintf(buf, sizeof buf, "0#key%ld", i);
        if (get(buf, &v) && v >= 0) hits++;
    }
    printf("%ld\n", hits);
    return 0;
}

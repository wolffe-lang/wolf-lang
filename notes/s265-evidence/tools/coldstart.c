/* s265 coldstart: time N fresh starts of each binary, interleaved
 * (A B A B ...), from fork to the child's reaped exit, CLOCK_MONOTONIC.
 *   coldstart [-n N] [-w W] [-e] [-o rows.tsv] LABEL=PATH[,ARG...] ...
 * -w: warm-up starts per binary, not recorded (default 3).
 * -e: evict the binary's pages from the page cache before every start
 *     (posix_fadvise DONTNEED on the file: a disk-cold start).
 * stdout, stderr of the child go to /dev/null; stdin is /dev/null.
 * Prints per label: n, median, p10, p90, min, max (ns), and the spread
 * (p90-p10)/median. A child that exits nonzero is a fault: exit 2. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#define MAXB 32
#define MAXA 16
struct bin { const char *label; char *argv[MAXA + 1]; uint64_t *ns; };

static uint64_t now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + (uint64_t)t.tv_nsec;
}

static void evict(const char *path) {
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0) { perror(path); exit(2); }
    fdatasync(fd);
    if (posix_fadvise(fd, 0, 0, POSIX_FADV_DONTNEED)) { perror("fadvise"); exit(2); }
    close(fd);
}

static uint64_t once(struct bin *b, int ev) {
    if (ev) evict(b->argv[0]);
    uint64_t t0 = now();
    pid_t p = fork();
    if (p < 0) { perror("fork"); exit(2); }
    if (p == 0) {
        int nul = open("/dev/null", O_RDWR);
        dup2(nul, 0); dup2(nul, 1); dup2(nul, 2);
        execv(b->argv[0], b->argv);
        _exit(127);
    }
    int st;
    while (waitpid(p, &st, 0) < 0) if (errno != EINTR) { perror("waitpid"); exit(2); }
    uint64_t t1 = now();
    if (!WIFEXITED(st) || WEXITSTATUS(st) != 0) {
        fprintf(stderr, "coldstart: %s ended with status 0x%x\n", b->label, st);
        exit(2);
    }
    return t1 - t0;
}

static int cmp(const void *a, const void *b) {
    uint64_t x = *(const uint64_t *)a, y = *(const uint64_t *)b;
    return x < y ? -1 : x > y;
}

static uint64_t pct(const uint64_t *s, int n, double q) {
    double r = q * (n - 1);
    int lo = (int)r;
    double f = r - lo;
    return lo + 1 < n ? (uint64_t)(s[lo] * (1 - f) + s[lo + 1] * f) : s[lo];
}

int main(int argc, char **argv) {
    int n = 50, w = 3, ev = 0, nb = 0;
    const char *out = 0;
    struct bin b[MAXB];
    for (int i = 1; i < argc; ++i) {
        if (!strcmp(argv[i], "-n")) n = atoi(argv[++i]);
        else if (!strcmp(argv[i], "-w")) w = atoi(argv[++i]);
        else if (!strcmp(argv[i], "-e")) ev = 1;
        else if (!strcmp(argv[i], "-o")) out = argv[++i];
        else {
            if (nb == MAXB) return 2;
            char *eq = strchr(argv[i], '=');
            if (!eq) { fprintf(stderr, "coldstart: LABEL=PATH[,ARG...]\n"); return 2; }
            *eq = 0;
            b[nb].label = argv[i];
            int k = 0;
            for (char *s = strtok(eq + 1, ","); s && k < MAXA; s = strtok(0, ",")) b[nb].argv[k++] = s;
            b[nb].argv[k] = 0;
            b[nb].ns = calloc((size_t)n, sizeof(uint64_t));
            ++nb;
        }
    }
    if (!nb || n < 1) return 2;
    for (int r = 0; r < w; ++r) for (int j = 0; j < nb; ++j) (void)once(&b[j], ev);
    for (int r = 0; r < n; ++r)
        for (int jj = 0; jj < nb; ++jj) {
            int j = (r & 1) ? nb - 1 - jj : jj; /* alternate the order each round */
            b[j].ns[r] = once(&b[j], ev);
        }
    FILE *o = out ? fopen(out, "w") : 0;
    if (o) {
        fprintf(o, "label\trun\tns\n");
        for (int j = 0; j < nb; ++j) for (int r = 0; r < n; ++r) fprintf(o, "%s\t%d\t%llu\n", b[j].label, r, (unsigned long long)b[j].ns[r]);
        fclose(o);
    }
    printf("label\tn\tmedian_ns\tp10_ns\tp90_ns\tmin_ns\tmax_ns\tspread\n");
    for (int j = 0; j < nb; ++j) {
        qsort(b[j].ns, (size_t)n, sizeof(uint64_t), cmp);
        uint64_t med = pct(b[j].ns, n, 0.5), p10 = pct(b[j].ns, n, 0.1), p90 = pct(b[j].ns, n, 0.9);
        printf("%s\t%d\t%llu\t%llu\t%llu\t%llu\t%llu\t%.3f\n", b[j].label, n, (unsigned long long)med,
               (unsigned long long)p10, (unsigned long long)p90, (unsigned long long)b[j].ns[0],
               (unsigned long long)b[j].ns[n - 1], med ? (double)(p90 - p10) / (double)med : 0.0);
    }
    return 0;
}

/* s286: a black-box probe of glibc's arena limit. Holds 8 * get_nprocs() + 8
 * threads that have each allocated, then prints malloc_stats(3) (one
 * "Arena N:" block per arena) and its own AT_SECURE, uids and CPU counts.
 * The caller counts the arenas. Run plain and as a set-user-ID copy, with
 * and without MALLOC_ARENA_MAX or GLIBC_TUNABLES, it says which settings
 * glibc honours in each. Written for s286; no glibc source was read. */
#define _GNU_SOURCE
#include <malloc.h>
#include <pthread.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/auxv.h>
#include <sys/sysinfo.h>
#include <unistd.h>

static pthread_barrier_t arrived, release;
/* Each thread's block escapes into its own slot, so the compiler cannot
 * remove the malloc/free pair (a first build of this probe at -O2 did, and
 * every thread allocated nothing). */
static void *volatile *held;

static void *hold(void *arg) {
    long i = (long)arg;
    held[i] = malloc(4096);
    pthread_barrier_wait(&arrived);
    pthread_barrier_wait(&release);
    free(held[i]);
    return NULL;
}

int main(void) {
    cpu_set_t set;
    CPU_ZERO(&set);
    sched_getaffinity(0, sizeof set, &set);
    int n = get_nprocs();
    int k = 8 * n + 8;
    fprintf(stderr, "PROBE secure=%lu ruid=%d euid=%d get_nprocs=%d affinity=%d threads=%d\n",
            getauxval(AT_SECURE), (int)getuid(), (int)geteuid(), n, CPU_COUNT(&set), k);
    pthread_barrier_init(&arrived, NULL, (unsigned)k + 1);
    pthread_barrier_init(&release, NULL, (unsigned)k + 1);
    pthread_t *t = calloc((size_t)k, sizeof *t);
    held = calloc((size_t)k, sizeof *held);
    for (int i = 0; i < k; i++) {
        if (pthread_create(&t[i], NULL, hold, (void *)(long)i) != 0) {
            fprintf(stderr, "PROBE pthread_create failed at %d\n", i);
            return 2;
        }
    }
    pthread_barrier_wait(&arrived);
    malloc_stats();
    pthread_barrier_wait(&release);
    for (int i = 0; i < k; i++) pthread_join(t[i], NULL);
    return 0;
}

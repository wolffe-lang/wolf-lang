/* s265 stackprobe: black-box, what pthread_getattr_np(3) reports for the
 * main thread beside /proc/self/maps (proc(5)) and RLIMIT_STACK, so a
 * cheaper query can be checked against the documented one. */
#define _GNU_SOURCE
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>

int main(void) {
    pthread_attr_t a;
    void *lo;
    size_t len, g;
    struct rlimit r;
    int here;
    pthread_getattr_np(pthread_self(), &a);
    pthread_attr_getstack(&a, &lo, &len);
    pthread_attr_getguardsize(&a, &g);
    getrlimit(RLIMIT_STACK, &r);
    printf("getattr lo=%#lx len=%#zx hi=%#lx guard=%#zx rlim_cur=%#lx local=%p\n",
           (unsigned long)lo, len, (unsigned long)lo + len, g, (unsigned long)r.rlim_cur, (void *)&here);
    FILE *f = fopen("/proc/self/maps", "r");
    char line[512], prev[512] = "";
    while (fgets(line, sizeof line, f)) {
        if (strstr(line, "[stack]")) { printf("below: %s", prev); printf("stack: %s", line); }
        strcpy(prev, line);
    }
    return 0;
}

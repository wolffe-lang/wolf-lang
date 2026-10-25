/* s265 piece: one start-up piece of s256's __wolf_rt_stack_start, in a
 * fresh C process, so coldstart can price each piece by difference.
 *   piece none | getattr | maps | stat | rlimit | alt | sig | all     */
#define _GNU_SOURCE
#include <fcntl.h>
#include <pthread.h>
#include <signal.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <unistd.h>

static volatile unsigned long sink;

static void h(int s, siginfo_t *i, void *c) { (void)s; (void)i; (void)c; }

static void getattr(void) {
    pthread_attr_t a;
    void *lo;
    size_t len, g;
    if (pthread_getattr_np(pthread_self(), &a)) _exit(3);
    pthread_attr_getstack(&a, &lo, &len);
    pthread_attr_getguardsize(&a, &g);
    pthread_attr_destroy(&a);
    sink = (unsigned long)lo + len + g;
}

static void rawread(const char *path) {
    char buf[8192];
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0) _exit(3);
    ssize_t n;
    unsigned long t = 0;
    while ((n = read(fd, buf, sizeof buf)) > 0)
        for (ssize_t i = 0; i < n; ++i) t += (unsigned char)buf[i] == '\n';
    close(fd);
    sink = t;
}

static void alt(void) {
    stack_t old;
    sigaltstack(0, &old);
    void *p = mmap(0, 32768, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    stack_t ss = {.ss_sp = p, .ss_flags = 0, .ss_size = 32768};
    sigaltstack(&ss, 0);
}

static void sig(void) {
    struct sigaction old, sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = h;
    sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigaction(SIGSEGV, 0, &old);
    sigaction(SIGBUS, 0, &old);
    sigaction(SIGSEGV, &sa, 0);
    sigaction(SIGBUS, &sa, 0);
}

int main(int argc, char **argv) {
    const char *m = argc > 1 ? argv[1] : "none";
    struct rlimit r;
    if (!strcmp(m, "getattr")) getattr();
    else if (!strcmp(m, "maps")) rawread("/proc/self/maps");
    else if (!strcmp(m, "stat")) rawread("/proc/self/stat");
    else if (!strcmp(m, "rlimit")) { getrlimit(RLIMIT_STACK, &r); sink = r.rlim_cur; }
    else if (!strcmp(m, "alt")) alt();
    else if (!strcmp(m, "sig")) sig();
    else if (!strcmp(m, "all")) { getattr(); alt(); sig(); }
    return 0;
}

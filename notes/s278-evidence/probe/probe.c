/* s278 black box: what the host answers for sigaction(2) on SIGKILL and
 * SIGSTOP (as a query, a null new action; and as a set: SIG_IGN, SIG_DFL,
 * a handler), and chroot(2) on a missing path, unprivileged. Written from
 * the man pages and POSIX alone; no C library or kernel source was read.
 * Every line is `call rc=R errno=E (text)`; a query that succeeds also
 * names the old action (dfl, ign or a handler). */
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/utsname.h>

static void on_signal(int sig) { (void)sig; }

static void show(const char *what, int rc, int err) {
    printf("%s rc=%d errno=%d (%s)\n", what, rc, rc == 0 ? 0 : err,
           rc == 0 ? "ok" : strerror(err));
}

static void query(const char *name, int sig) {
    struct sigaction old;
    char what[96];
    memset(&old, 0, sizeof old);
    errno = 0;
    int rc = sigaction(sig, NULL, &old);
    int err = errno;
    snprintf(what, sizeof what, "sigaction(%s=%d, NULL, &old)", name, sig);
    show(what, rc, err);
    if (rc == 0) {
        const char *h = old.sa_handler == SIG_DFL ? "dfl"
                      : old.sa_handler == SIG_IGN ? "ign" : "handler";
        printf("  old action of %s: %s\n", name, h);
    }
}

static void set(const char *name, int sig, const char *how, void (*h)(int)) {
    struct sigaction act;
    char what[96];
    memset(&act, 0, sizeof act);
    act.sa_handler = h;
    sigemptyset(&act.sa_mask);
    errno = 0;
    int rc = sigaction(sig, &act, NULL);
    int err = errno;
    snprintf(what, sizeof what, "sigaction(%s=%d, %s, NULL)", name, sig, how);
    show(what, rc, err);
}

static void root(const char *path) {
    char what[160];
    errno = 0;
    int rc = chroot(path);
    int err = errno;
    snprintf(what, sizeof what, "chroot(\"%s\")", path);
    show(what, rc, err);
}

int main(void) {
    struct utsname u;
    if (uname(&u) == 0) printf("uname %s %s %s\n", u.sysname, u.release, u.machine);
    printf("uid=%d euid=%d\n", (int)getuid(), (int)geteuid());

    query("SIGTERM", SIGTERM);
    query("SIGKILL", SIGKILL);
    query("SIGSTOP", SIGSTOP);
    set("SIGKILL", SIGKILL, "SIG_IGN", SIG_IGN);
    set("SIGKILL", SIGKILL, "SIG_DFL", SIG_DFL);
    set("SIGKILL", SIGKILL, "handler", on_signal);
    set("SIGSTOP", SIGSTOP, "SIG_IGN", SIG_IGN);
    set("SIGSTOP", SIGSTOP, "SIG_DFL", SIG_DFL);
    set("SIGSTOP", SIGSTOP, "handler", on_signal);
    /* The queries again: no set above moved anything. */
    query("SIGKILL", SIGKILL);
    query("SIGSTOP", SIGSTOP);

    /* chroot as this (unprivileged) user: the corpus row's missing path,
     * relative, then an absolute one, then ".", then a regular file. */
    root("s272-chroot-no-such-dir/inner");
    root("/s278-chroot-no-such-dir/inner");
    root(".");
    int fd = open("s278-probe-file", O_CREAT | O_WRONLY | O_TRUNC, 0600);
    if (fd >= 0) close(fd);
    root("s278-probe-file");
    unlink("s278-probe-file");
    return 0;
}

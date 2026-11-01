/* s272 m1: black-box answers of the host calls the five builtins make. */
#define _GNU_SOURCE
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/statfs.h>
#include <sys/statvfs.h>
#include <unistd.h>
static void fs(const char *p) {
    struct statfs s; struct statvfs v;
    if (statfs(p, &s) != 0) { printf("statfs %s: errno=%d\n", p, errno); return; }
    statvfs(p, &v);
    int a[2]; memcpy(a, &s.f_fsid, sizeof a);
    printf("statfs %s: type=%lx bsize=%ld frsize=%ld blocks=%lu bfree=%lu bavail=%lu files=%lu ffree=%lu namelen=%ld flags=%lx fsid0=%08x fsid1=%08x vfs_fsid=%lx vfs_flag=%lx\n",
        p, (long)s.f_type, (long)s.f_bsize, (long)s.f_frsize, (unsigned long)s.f_blocks, (unsigned long)s.f_bfree,
        (unsigned long)s.f_bavail, (unsigned long)s.f_files, (unsigned long)s.f_ffree, (long)s.f_namelen,
        (long)s.f_flags, (unsigned)a[0], (unsigned)a[1], (unsigned long)v.f_fsid, (unsigned long)v.f_flag);
}
int main(int argc, char **argv) {
    printf("euid=%d\n", (int)geteuid());
    for (int i = 1; i < argc; i++) fs(argv[i]);
    int sigs[] = {SIGHUP, SIGKILL, SIGSTOP, SIGTERM, SIGSEGV, SIGBUS, 0, 32, 33, 34, 64, 65};
    for (unsigned i = 0; i < sizeof sigs / sizeof sigs[0]; i++) {
        struct sigaction old; memset(&old, 0, sizeof old); errno = 0;
        int r = sigaction(sigs[i], NULL, &old);
        printf("sigaction-get %d: rc=%d errno=%d %s\n", sigs[i], r, r ? errno : 0,
               r ? "-" : old.sa_handler == SIG_DFL ? "default" : old.sa_handler == SIG_IGN ? "ignored" : "handler");
    }
    errno = 0; int r = chroot("/no-such-dir/inner"); printf("chroot(missing): rc=%d errno=%d\n", r, r ? errno : 0);
    errno = 0; r = chroot("/etc/hostname"); printf("chroot(file): rc=%d errno=%d\n", r, r ? errno : 0);
    errno = 0; int p = getpriority(PRIO_PROCESS, 0); printf("getpriority=%d errno=%d\n", p, errno);
    errno = 0; r = setpriority(PRIO_PROCESS, 0, p + 2); printf("setpriority(+2): rc=%d errno=%d now=%d\n", r, r ? errno : 0, getpriority(PRIO_PROCESS, 0));
    errno = 0; r = setpriority(PRIO_PROCESS, 0, p + 1); printf("setpriority(-1): rc=%d errno=%d %s now=%d\n", r, r ? errno : 0, r ? strerror(errno) : "", getpriority(PRIO_PROCESS, 0));
    errno = 0; r = nice(-1); printf("nice(-1): rc=%d errno=%d %s\n", r, errno, errno ? strerror(errno) : "");
    errno = 0; r = setpriority(PRIO_PROCESS, 0, 100); printf("setpriority(100): rc=%d errno=%d now=%d\n", r, r ? errno : 0, getpriority(PRIO_PROCESS, 0));
    errno = 0; r = chroot("."); printf("chroot(.): rc=%d errno=%d\n", r, r ? errno : 0);
    return 0;
}

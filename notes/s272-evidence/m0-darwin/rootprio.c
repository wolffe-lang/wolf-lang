#include <unistd.h>
#include <stdio.h>
#include <errno.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/statvfs.h>
int main(void) {
    errno = 0; int r = chroot("s272-no-such-dir/inner"); printf("chroot(missing): rc=%d errno=%d %s\n", r, r ? errno : 0, r ? strerror(errno) : "");
    errno = 0; r = chroot("."); printf("chroot(.): rc=%d errno=%d %s\n", r, r ? errno : 0, r ? strerror(errno) : "");
    errno = 0; r = chroot("rootprio.c"); printf("chroot(file): rc=%d errno=%d %s\n", r, r ? errno : 0, r ? strerror(errno) : "");
    errno = 0; int p = getpriority(PRIO_PROCESS, 0); printf("getpriority: %d errno=%d PRIO_MIN=%d PRIO_MAX=%d\n", p, errno, PRIO_MIN, PRIO_MAX);
    errno = 0; r = setpriority(PRIO_PROCESS, 0, p + 1); printf("setpriority(+1): rc=%d errno=%d\n", r, r ? errno : 0);
    errno = 0; printf("now %d\n", getpriority(PRIO_PROCESS, 0));
    errno = 0; r = setpriority(PRIO_PROCESS, 0, p); printf("setpriority(back to %d): rc=%d errno=%d %s\n", p, r, r ? errno : 0, r ? strerror(errno) : "");
    errno = 0; r = setpriority(PRIO_PROCESS, 0, 100); printf("setpriority(100): rc=%d errno=%d %s now %d\n", r, r ? errno : 0, r ? strerror(errno) : "", getpriority(PRIO_PROCESS, 0));
    struct statvfs v; statvfs(".", &v); printf("statvfs . flag=%lx namemax=%lu frsize=%lu bsize=%lu\n", v.f_flag, v.f_namemax, v.f_frsize, v.f_bsize);
    return 0;
}

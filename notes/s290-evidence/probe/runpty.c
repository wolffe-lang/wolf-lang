/* Run argv[1..] as the leader of a new session on a fresh pseudo-terminal,
   copying what it writes to stdout. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <unistd.h>
int main(int argc, char **argv) {
    int m = posix_openpt(O_RDWR | O_NOCTTY);
    if (m < 0 || grantpt(m) < 0 || unlockpt(m) < 0) { perror("pty"); return 1; }
    char *name = ptsname(m);
    pid_t p = fork();
    if (p == 0) {
        setsid();
        int s = open(name, O_RDWR | O_NOCTTY);
        if (s < 0 || ioctl(s, TIOCSCTTY, 0) < 0) { perror("ctty"); _exit(1); }
        dup2(s, 0); dup2(s, 1); dup2(s, 2);
        if (s > 2) close(s);
        close(m);
        execv(argv[1], argv + 1);
        _exit(127);
    }
    char b[4096];
    for (;;) {
        ssize_t n = read(m, b, sizeof b);
        if (n <= 0) break;
        fwrite(b, 1, (size_t)n, stdout);
    }
    int st; waitpid(p, &st, 0);
    return 0;
}

/* s290 probe: a pty master written non-blocking until EAGAIN, its slave raw
 * and unread: what poll(2) then says of POLLOUT, and what a write after that
 * takes, for ten rounds; then the slave reads everything and poll is asked
 * again. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <termios.h>
#include <unistd.h>
int main(void) {
    int m = posix_openpt(O_RDWR | O_NOCTTY); grantpt(m); unlockpt(m);
    int s = open(ptsname(m), O_RDWR | O_NOCTTY);
    struct termios t; tcgetattr(s, &t); cfmakeraw(&t); tcsetattr(s, TCSANOW, &t);
    fcntl(m, F_SETFL, fcntl(m, F_GETFL) | O_NONBLOCK);
    static char buf[4096]; memset(buf, 'w', sizeof buf);
    long total = 0;
    for (int round = 0; round < 10; round++) {
        long took = 0; ssize_t n;
        while ((n = write(m, buf, sizeof buf)) > 0) took += n;
        int e = errno; total += took;
        struct pollfd p = {m, POLLOUT, 0};
        int r = poll(&p, 1, 25);
        printf("round %d: took %ld then %s; poll(25ms) n=%d revents=%s\n", round, took, strerror(e), r,
               (p.revents & POLLOUT) ? "OUT" : "-");
        if (r == 0) break;
    }
    long got = 0; ssize_t n; char rb[65536];
    fcntl(s, F_SETFL, fcntl(s, F_GETFL) | O_NONBLOCK);
    while ((n = read(s, rb, sizeof rb)) > 0) got += n;
    struct pollfd p = {m, POLLOUT, 0};
    int r = poll(&p, 1, 0);
    printf("total written %ld, the slave read %ld; poll after: n=%d revents=%s\n", total, got, r, (p.revents & POLLOUT) ? "OUT" : "-");
    printf("END exit=0\n");
    return 0;
}

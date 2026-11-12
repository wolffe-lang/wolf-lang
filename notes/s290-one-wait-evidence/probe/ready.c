/* s290 probe: what poll(2), epoll and kqueue answer for the descriptors a
 * multiplexer waits on (pty masters, pipes, unix and tcp streams, regular
 * files), and what the peer-credential options give. Black box: the host's
 * answers only. Prints one line per question; ends with "END exit=0". Its
 * scratch (a socket directory, a file) is made in the current directory. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/epoll.h>
#endif
#if defined(__APPLE__) || defined(__FreeBSD__)
#include <sys/event.h>
#include <sys/ucred.h>
#endif

#ifndef POLLRDHUP
#define POLLRDHUP 0
#endif

static void pbits(char *b, short r) {
    b[0] = 0;
    if (r & POLLIN) strcat(b, "IN ");
    if (r & POLLOUT) strcat(b, "OUT ");
    if (r & POLLHUP) strcat(b, "HUP ");
    if (r & POLLERR) strcat(b, "ERR ");
    if (r & POLLNVAL) strcat(b, "NVAL ");
#ifdef __linux__
    if (r & POLLRDHUP) strcat(b, "RDHUP ");
#endif
    if (!b[0]) strcat(b, "- ");
}

static void ask_poll(const char *what, int fd, short ev) {
    struct pollfd p = {fd, ev, 0};
    int n = poll(&p, 1, 0);
    char b[64];
    pbits(b, p.revents);
    printf("poll   %-34s ask=%s n=%d revents=%s\n", what, ev == POLLIN ? "IN " : (ev == POLLOUT ? "OUT" : "I|O"), n, b);
}

static void ask_select(const char *what, int fd) {
    fd_set r, w;
    FD_ZERO(&r); FD_ZERO(&w);
    FD_SET(fd, &r); FD_SET(fd, &w);
    struct timeval tv = {0, 0};
    int n = select(fd + 1, &r, &w, NULL, &tv);
    printf("select %-34s n=%d r=%d w=%d\n", what, n, FD_ISSET(fd, &r) ? 1 : 0, FD_ISSET(fd, &w) ? 1 : 0);
}

#ifdef __linux__
static void ask_kernel(const char *what, int fd, int both) {
    int ep = epoll_create1(EPOLL_CLOEXEC);
    struct epoll_event e = {0};
    e.events = EPOLLIN | EPOLLRDHUP | (both ? EPOLLOUT : 0);
    e.data.u64 = 7;
    if (epoll_ctl(ep, EPOLL_CTL_ADD, fd, &e) < 0) {
        printf("epoll  %-34s ctl=%s\n", what, strerror(errno));
        close(ep);
        return;
    }
    struct epoll_event out[4];
    int n = epoll_wait(ep, out, 4, 0);
    char b[80] = "";
    if (n > 0) {
        unsigned v = out[0].events;
        if (v & EPOLLIN) strcat(b, "IN ");
        if (v & EPOLLOUT) strcat(b, "OUT ");
        if (v & EPOLLHUP) strcat(b, "HUP ");
        if (v & EPOLLERR) strcat(b, "ERR ");
        if (v & EPOLLRDHUP) strcat(b, "RDHUP ");
    } else strcat(b, "- ");
    printf("epoll  %-34s n=%d events=%s\n", what, n, b);
    close(ep);
}
#else
static void ask_kernel(const char *what, int fd, int both) {
    int kq = kqueue();
    struct kevent ch[2];
    int nch = 0;
    EV_SET(&ch[nch++], fd, EVFILT_READ, EV_ADD, 0, 0, (void *)7);
    if (both) EV_SET(&ch[nch++], fd, EVFILT_WRITE, EV_ADD, 0, 0, (void *)7);
    struct kevent out[4];
    struct timespec z = {0, 0};
    /* receipts: register and read each change's result */
    struct kevent rc[2];
    for (int i = 0; i < nch; i++) ch[i].flags |= EV_RECEIPT;
    int r = kevent(kq, ch, nch, rc, nch, &z);
    for (int i = 0; i < r; i++) {
        if ((rc[i].flags & EV_ERROR) && rc[i].data != 0) {
            printf("kqueue %-34s filter=%s ctl=%s\n", what, rc[i].filter == EVFILT_READ ? "READ" : "WRITE",
                   strerror((int)rc[i].data));
        }
    }
    int n = kevent(kq, NULL, 0, out, 4, &z);
    char b[200] = "";
    for (int i = 0; i < n; i++) {
        char one[80];
        snprintf(one, sizeof one, "[%s%s%s data=%ld fflags=%u] ", out[i].filter == EVFILT_READ ? "READ" : "WRITE",
                 (out[i].flags & EV_EOF) ? " EOF" : "", (out[i].flags & EV_ERROR) ? " ERROR" : "",
                 (long)out[i].data, out[i].fflags);
        strcat(b, one);
    }
    if (n <= 0) strcat(b, "-");
    printf("kqueue %-34s n=%d %s\n", what, n, b);
    close(kq);
}
#endif

static void ask_all(const char *what, int fd, short ev) {
    ask_poll(what, fd, ev);
    ask_select(what, fd);
    ask_kernel(what, fd, ev != POLLIN);
}

static void pty_rows(void) {
    int m = posix_openpt(O_RDWR | O_NOCTTY);
    grantpt(m);
    unlockpt(m);
    char *sn = ptsname(m);
    ask_all("pty master, slave never opened", m, POLLIN | POLLOUT);
    int s = open(sn, O_RDWR | O_NOCTTY);
    ask_all("pty master, slave open, idle", m, POLLIN | POLLOUT);
    write(s, "x", 1);
    ask_all("pty master, slave wrote 1", m, POLLIN | POLLOUT);
    char buf[64];
    read(m, buf, sizeof buf);
    close(s);
    ask_all("pty master, slave closed", m, POLLIN | POLLOUT);
    ssize_t r = read(m, buf, sizeof buf);
    printf("read   pty master after slave closed: %zd %s\n", r, r < 0 ? strerror(errno) : "");
    /* a slave that wrote and then closed: data first, then the end */
    s = open(sn, O_RDWR | O_NOCTTY);
    write(s, "yz", 2);
    close(s);
    ask_all("pty master, slave wrote 2 then closed", m, POLLIN | POLLOUT);
    r = read(m, buf, sizeof buf);
    printf("read   pty master (data then close): %zd\n", r);
    r = read(m, buf, sizeof buf);
    printf("read   pty master (then): %zd %s\n", r, r < 0 ? strerror(errno) : "");
    /* a child on the slave that exits */
    pid_t c = fork();
    if (c == 0) {
        setsid();
        int ss = open(sn, O_RDWR);
        write(ss, "bye\n", 4);
        _exit(0);
    }
    int st;
    waitpid(c, &st, 0);
    ask_all("pty master, child on slave exited", m, POLLIN | POLLOUT);
    close(m);
}

static void pipe_rows(void) {
    int p[2];
    pipe(p);
    ask_all("pipe read end, empty", p[0], POLLIN);
    ask_all("pipe write end, room", p[1], POLLOUT);
    close(p[1]);
    ask_all("pipe read end, writer gone", p[0], POLLIN);
    close(p[0]);
    pipe(p);
    close(p[0]);
    signal(SIGPIPE, SIG_IGN);
    ask_all("pipe write end, reader gone", p[1], POLLOUT);
    close(p[1]);
    /* pipe capacity for a nonblocking write */
    pipe(p);
    fcntl(p[1], F_SETFL, O_NONBLOCK);
    static char big[1 << 20];
    ssize_t n = write(p[1], big, sizeof big);
    printf("write  pipe nonblocking 1 MiB: took %zd\n", n);
    ask_all("pipe write end, full", p[1], POLLOUT);
    close(p[0]);
    close(p[1]);
}

static void unix_rows(void) {
    int sv[2];
    socketpair(AF_UNIX, SOCK_STREAM, 0, sv);
    ask_all("socketpair, idle", sv[0], POLLIN | POLLOUT);
    shutdown(sv[1], SHUT_WR);
    ask_all("socketpair, peer SHUT_WR", sv[0], POLLIN | POLLOUT);
    close(sv[1]);
    ask_all("socketpair, peer closed", sv[0], POLLIN | POLLOUT);
    close(sv[0]);
    /* nonblocking send buffer */
    socketpair(AF_UNIX, SOCK_STREAM, 0, sv);
    fcntl(sv[0], F_SETFL, O_NONBLOCK);
    static char big[1 << 20];
    ssize_t n = send(sv[0], big, sizeof big, 0);
    printf("send   unix stream nonblocking 1 MiB: took %zd\n", n);
    n = send(sv[0], big, sizeof big, 0);
    printf("send   unix stream again: %zd %s\n", n, n < 0 ? strerror(errno) : "");
    ask_all("unix stream, send buffer full", sv[0], POLLOUT);
    close(sv[1]);
    n = send(sv[0], big, 10, 0);
    printf("send   unix stream, peer closed: %zd %s\n", n, n < 0 ? strerror(errno) : "");
    close(sv[0]);
}

static void cred_rows(void) {
    char path[] = "s290-probe-XXXXXX";
    /* a listener at a fresh path */
    char dir[64];
    snprintf(dir, sizeof dir, "%s", path);
    if (!mkdtemp(dir)) {
        printf("cred   mkdtemp failed\n");
        return;
    }
    struct sockaddr_un a = {0};
    a.sun_family = AF_UNIX;
    snprintf(a.sun_path, sizeof a.sun_path, "%s/l", dir);
    int l = socket(AF_UNIX, SOCK_STREAM, 0);
    bind(l, (struct sockaddr *)&a, sizeof a);
    listen(l, 4);
    ask_all("unix listener, none pending", l, POLLIN);
    int c = socket(AF_UNIX, SOCK_STREAM, 0);
    connect(c, (struct sockaddr *)&a, sizeof a);
    ask_all("unix listener, one pending", l, POLLIN);
    int s = accept(l, NULL, NULL);
#ifdef __linux__
    struct ucred u;
    socklen_t ul = sizeof u;
    int r = getsockopt(s, SOL_SOCKET, SO_PEERCRED, &u, &ul);
    printf("cred   SO_PEERCRED accepted: r=%d pid_is_self=%d uid=%d gid=%d\n", r, u.pid == getpid(), (int)u.uid,
           (int)u.gid);
    ul = sizeof u;
    r = getsockopt(c, SOL_SOCKET, SO_PEERCRED, &u, &ul);
    printf("cred   SO_PEERCRED dialed: r=%d pid_is_self=%d\n", r, u.pid == getpid());
    ul = sizeof u;
    r = getsockopt(l, SOL_SOCKET, SO_PEERCRED, &u, &ul);
    printf("cred   SO_PEERCRED listener: r=%d %s pid=%d uid=%d\n", r, r < 0 ? strerror(errno) : "", u.pid,
           (int)u.uid);
    int t = socket(AF_INET, SOCK_STREAM, 0);
    ul = sizeof u;
    r = getsockopt(t, SOL_SOCKET, SO_PEERCRED, &u, &ul);
    printf("cred   SO_PEERCRED tcp unconnected: r=%d %s pid=%d uid=%d\n", r, r < 0 ? strerror(errno) : "", u.pid,
           (int)u.uid);
    close(t);
#else
    struct xucred x;
    socklen_t xl = sizeof x;
    int r = getsockopt(s, SOL_LOCAL, LOCAL_PEERCRED, &x, &xl);
    printf("cred   LOCAL_PEERCRED accepted: r=%d ver=%u uid=%d ngroups=%d g0=%d getgid=%d\n", r, x.cr_version,
           (int)x.cr_uid, x.cr_ngroups, (int)x.cr_groups[0], (int)getgid());
#ifdef LOCAL_PEERPID
    pid_t pp = -2;
    socklen_t pl = sizeof pp;
    r = getsockopt(s, SOL_LOCAL, LOCAL_PEERPID, &pp, &pl);
    printf("cred   LOCAL_PEERPID accepted: r=%d pid_is_self=%d\n", r, pp == getpid());
#endif
    xl = sizeof x;
    r = getsockopt(l, SOL_LOCAL, LOCAL_PEERCRED, &x, &xl);
    printf("cred   LOCAL_PEERCRED listener: r=%d %s\n", r, r < 0 ? strerror(errno) : "");
    int t = socket(AF_INET, SOCK_STREAM, 0);
    xl = sizeof x;
    r = getsockopt(t, SOL_LOCAL, LOCAL_PEERCRED, &x, &xl);
    printf("cred   LOCAL_PEERCRED tcp: r=%d %s\n", r, r < 0 ? strerror(errno) : "");
    close(t);
#endif
    close(c);
    close(s);
    close(l);
    unlink(a.sun_path);
    rmdir(dir);
}

static void tcp_rows(void) {
    int l = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in a = {0};
    a.sin_family = AF_INET;
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    bind(l, (struct sockaddr *)&a, sizeof a);
    socklen_t al = sizeof a;
    getsockname(l, (struct sockaddr *)&a, &al);
    listen(l, 4);
    int c = socket(AF_INET, SOCK_STREAM, 0);
    connect(c, (struct sockaddr *)&a, sizeof a);
    int s = accept(l, NULL, NULL);
    usleep(20000);
    ask_all("tcp stream, idle", s, POLLIN | POLLOUT);
    close(c);
    usleep(20000);
    ask_all("tcp stream, peer closed", s, POLLIN | POLLOUT);
    close(s);
    close(l);
}

static void file_rows(void) {
    char p[] = "s290-probe-file-XXXXXX";
    int f = mkstemp(p);
    write(f, "abc", 3);
    lseek(f, 0, SEEK_SET);
    ask_all("regular file, 3 bytes, at 0", f, POLLIN | POLLOUT);
    lseek(f, 0, SEEK_END);
    ask_all("regular file, at end", f, POLLIN | POLLOUT);
    close(f);
    unlink(p);
    int n = open("/dev/null", O_RDWR);
    ask_all("/dev/null", n, POLLIN | POLLOUT);
    close(n);
}

static void kernel_set_rows(void) {
    int p[2];
    pipe(p);
#ifdef __linux__
    int ep = epoll_create1(EPOLL_CLOEXEC);
    struct epoll_event e = {.events = EPOLLIN, .data.u64 = 1};
    epoll_ctl(ep, EPOLL_CTL_ADD, p[0], &e);
    write(p[1], "x", 1);
    struct pollfd q = {ep, POLLIN, 0};
    printf("kset   poll on epoll fd with a ready entry: n=%d revents=%d\n", poll(&q, 1, 0), q.revents);
    int d = dup(p[0]);
    close(p[0]);
    struct epoll_event o[2];
    int n = epoll_wait(ep, o, 2, 0);
    printf("kset   epoll after close of the registered fd, a dup alive: n=%d data=%llu\n", n,
           n > 0 ? (unsigned long long)o[0].data.u64 : 0ULL);
    close(d);
    n = epoll_wait(ep, o, 2, 0);
    printf("kset   epoll after the dup closed too: n=%d\n", n);
    pid_t c = fork();
    if (c == 0) {
        n = epoll_wait(ep, o, 2, 0);
        printf("kset   epoll fd in a fork's child: n=%d %s\n", n, n < 0 ? strerror(errno) : "(usable)");
        fflush(stdout);
        _exit(0);
    }
    waitpid(c, NULL, 0);
    close(ep);
#else
    int kq = kqueue();
    struct kevent ch;
    EV_SET(&ch, p[0], EVFILT_READ, EV_ADD, 0, 0, (void *)1);
    struct timespec z = {0, 0};
    kevent(kq, &ch, 1, NULL, 0, &z);
    write(p[1], "x", 1);
    struct pollfd q = {kq, POLLIN, 0};
    int pn = poll(&q, 1, 0);
    printf("kset   poll on kqueue fd with a ready entry: n=%d revents=%d\n", pn, q.revents);
    fd_set rs;
    FD_ZERO(&rs);
    FD_SET(kq, &rs);
    struct timeval tv = {0, 0};
    printf("kset   select on kqueue fd with a ready entry: n=%d\n", select(kq + 1, &rs, NULL, NULL, &tv));
    int d = dup(p[0]);
    close(p[0]);
    struct kevent o[2];
    int n = kevent(kq, NULL, 0, o, 2, &z);
    printf("kset   kqueue after close of the registered fd, a dup alive: n=%d\n", n);
    close(d);
    pid_t c = fork();
    if (c == 0) {
        n = kevent(kq, NULL, 0, o, 2, &z);
        printf("kset   kqueue fd in a fork's child: n=%d %s\n", n, n < 0 ? strerror(errno) : "(usable)");
        fflush(stdout);
        _exit(0);
    }
    waitpid(c, NULL, 0);
    close(kq);
#endif
    close(p[1]);
}

/* /dev/tty of a session on a fresh pty, in a child */
static void tty_rows(void) {
    int m = posix_openpt(O_RDWR | O_NOCTTY);
    grantpt(m);
    unlockpt(m);
    char *sn = ptsname(m);
    fflush(stdout);
    pid_t c = fork();
    if (c == 0) {
        setsid();
        int s = open(sn, O_RDWR);
#ifdef TIOCSCTTY
        ioctl(s, TIOCSCTTY, 0);
#endif
        int t = open("/dev/tty", O_RDWR);
        printf("tty    /dev/tty open: %d %s\n", t, t < 0 ? strerror(errno) : "");
        if (t >= 0) ask_all("/dev/tty (a pty slave)", t, POLLIN | POLLOUT);
        ask_all("pty slave by path", s, POLLIN | POLLOUT);
        fflush(stdout);
        _exit(0);
    }
    /* drain the child's output to the master so the slave never blocks */
    int st;
    waitpid(c, &st, 0);
    close(m);
}

int main(void) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    pty_rows();
    pipe_rows();
    unix_rows();
    tcp_rows();
    file_rows();
    cred_rows();
    kernel_set_rows();
    tty_rows();
    printf("END exit=0\n");
    return 0;
}

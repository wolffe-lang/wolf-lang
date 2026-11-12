/* s290 probe: LOCAL_PEERPID / SO_PEERCRED of a stream a forked child dialed,
 * asked while the child is connected and again after it has closed. */
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>
int main(void) {
    struct sockaddr_un a; memset(&a, 0, sizeof a); a.sun_family = AF_UNIX;
    snprintf(a.sun_path, sizeof a.sun_path, "peerpid.sock"); unlink(a.sun_path);
    int l = socket(AF_UNIX, SOCK_STREAM, 0); bind(l, (struct sockaddr *)&a, sizeof a); listen(l, 4);
    pid_t c = fork();
    if (c == 0) { int s = socket(AF_UNIX, SOCK_STREAM, 0); connect(s, (struct sockaddr *)&a, sizeof a); write(s, "k", 1); sleep(1); _exit(0); }
    int s = accept(l, NULL, NULL); char b; read(s, &b, 1);
#ifdef __APPLE__
    pid_t p = -2; socklen_t pl = sizeof p;
    int r = getsockopt(s, 0, 2, &p, &pl);
    printf("LOCAL_PEERPID r=%d pid_is_child=%d pid_is_parent=%d len=%u\n", r, p == c, p == getpid(), pl);
#else
    struct ucred u; socklen_t ul = sizeof u; int r = getsockopt(s, SOL_SOCKET, SO_PEERCRED, &u, &ul);
    printf("SO_PEERCRED r=%d pid_is_child=%d pid_is_parent=%d\n", r, u.pid == c, u.pid == getpid());
#endif
    waitpid(c, NULL, 0);
#ifdef __APPLE__
    p = -2; pl = sizeof p; r = getsockopt(s, 0, 2, &p, &pl);
    printf("after the child closed: LOCAL_PEERPID r=%d pid=%d\n", r, (int)p);
#else
    ul = sizeof u; r = getsockopt(s, SOL_SOCKET, SO_PEERCRED, &u, &ul);
    printf("after the child closed: SO_PEERCRED r=%d pid_is_child=%d\n", r, u.pid == c);
#endif
    unlink(a.sun_path); printf("END exit=0\n"); return 0;
}

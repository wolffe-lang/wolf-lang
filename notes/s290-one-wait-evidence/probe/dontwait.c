/* s290 probe: does send(2) with MSG_DONTWAIT park on a BLOCKING Unix-domain
 * stream whose buffer is full? alarm(3) ends a park; prints the answer. */
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>
static void on_alarm(int s) { (void)s; }
int main(void) {
    struct sigaction sa; memset(&sa, 0, sizeof sa); sa.sa_handler = on_alarm; sigaction(SIGALRM, &sa, NULL);
    int sv[2]; socketpair(AF_UNIX, SOCK_STREAM, 0, sv);
    static char buf[4096]; long total = 0;
    alarm(3);
    for (int i = 0; i < 10000; i++) {
        ssize_t n = send(sv[0], buf, sizeof buf, MSG_DONTWAIT);
        if (n < 0) { printf("unix stream, blocking, MSG_DONTWAIT: took %ld then %s\n", total, strerror(errno)); break; }
        total += n;
    }
    alarm(0);
    printf("END exit=0\n");
    return 0;
}

#include <signal.h>
#include <stdio.h>
#include <errno.h>
#include <string.h>
int main(void) {
    int sigs[] = {SIGHUP, SIGINT, SIGKILL, SIGSTOP, SIGTERM, SIGPIPE, SIGSEGV, SIGBUS, 0, 32, 33, 64};
    for (unsigned i = 0; i < sizeof sigs / sizeof sigs[0]; i++) {
        struct sigaction old;
        memset(&old, 0, sizeof old);
        errno = 0;
        int r = sigaction(sigs[i], NULL, &old);
        const char *what = r != 0 ? "-" : old.sa_handler == SIG_DFL ? "default" : old.sa_handler == SIG_IGN ? "ignored" : "handler";
        printf("sig %d: rc=%d errno=%d (%s) %s\n", sigs[i], r, r ? errno : 0, r ? strerror(errno) : "", what);
    }
    return 0;
}

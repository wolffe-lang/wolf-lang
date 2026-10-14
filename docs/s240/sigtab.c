/* s240 probe: the host's signal table as a C program sees it — how many
 * signals, the real-time range, and each signal's description text
 * (strsignal), before and after setlocale(LC_ALL, "").
 *
 * Written from the signal(7), strsignal(3) and setlocale(3) man pages
 * and POSIX.1-2024 <signal.h>; no C library's source was read. Sends no
 * signal.
 */
#define _GNU_SOURCE
#include <locale.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef NSIG
#define NSIG 65
#endif

struct named { const char *name; int num; };

static const struct named posix[] = {
    {"HUP", SIGHUP}, {"INT", SIGINT}, {"QUIT", SIGQUIT}, {"ILL", SIGILL},
    {"TRAP", SIGTRAP}, {"ABRT", SIGABRT}, {"BUS", SIGBUS}, {"FPE", SIGFPE},
    {"KILL", SIGKILL}, {"USR1", SIGUSR1}, {"SEGV", SIGSEGV}, {"USR2", SIGUSR2},
    {"PIPE", SIGPIPE}, {"ALRM", SIGALRM}, {"TERM", SIGTERM}, {"CHLD", SIGCHLD},
    {"CONT", SIGCONT}, {"STOP", SIGSTOP}, {"TSTP", SIGTSTP}, {"TTIN", SIGTTIN},
    {"TTOU", SIGTTOU}, {"URG", SIGURG}, {"XCPU", SIGXCPU}, {"XFSZ", SIGXFSZ},
    {"VTALRM", SIGVTALRM}, {"PROF", SIGPROF}, {"WINCH", SIGWINCH}, {"SYS", SIGSYS},
};

static const struct named hostonly[] = {
#ifdef SIGSTKFLT
    {"STKFLT", SIGSTKFLT},
#endif
#ifdef SIGIO
    {"IO", SIGIO},
#endif
#ifdef SIGPOLL
    {"POLL", SIGPOLL},
#endif
#ifdef SIGPWR
    {"PWR", SIGPWR},
#endif
#ifdef SIGEMT
    {"EMT", SIGEMT},
#endif
#ifdef SIGINFO
    {"INFO", SIGINFO},
#endif
#ifdef SIGLOST
    {"LOST", SIGLOST},
#endif
    {0, 0},
};

int main(int argc, char **argv) {
    int localized = argc > 1 && !strcmp(argv[1], "--setlocale");
    if (localized) {
        const char *l = setlocale(LC_ALL, "");
        printf("setlocale(LC_ALL, \"\"): %s\n", l ? l : "(null)");
    }
    printf("NSIG: %d\n", (int)NSIG);
#ifdef SIGRTMIN
    printf("SIGRTMIN: %d\nSIGRTMAX: %d\n", (int)SIGRTMIN, (int)SIGRTMAX);
#else
    printf("SIGRTMIN: none\nSIGRTMAX: none\n");
#endif
    printf("-- the 28 POSIX names: name number text\n");
    for (size_t i = 0; i < sizeof posix / sizeof posix[0]; i++)
        printf("%s %d %s\n", posix[i].name, posix[i].num, strsignal(posix[i].num));
    printf("-- host-only names: name number text\n");
    for (const struct named *h = hostonly; h->name; h++)
        printf("%s %d %s\n", h->name, h->num, strsignal(h->num));
    printf("-- every number 0..NSIG: number text\n");
    for (int n = 0; n <= (int)NSIG; n++) printf("%d %s\n", n, strsignal(n));
    printf("-- out of range: number text\n");
    printf("%d %s\n", 100, strsignal(100));
    printf("%d %s\n", -1, strsignal(-1));
    return 0;
}

/* s286 container-only taskset shim (a copy of r43s, renamed variables):
 * preserves dispositions and closed standard fds. Tests request 0-3 (four
 * constrained cores) or 0 (one core); those map to the lane's own slot.
 * Every other argument goes to the real taskset, which validates it.
 */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc > 2 && (!strcmp(argv[1], "-c") || !strcmp(argv[1], "--cpu-list"))) {
        const char *replacement = NULL;
        if (!strcmp(argv[2], "0-3")) replacement = getenv("S286_CPU_FOUR");
        if (!strcmp(argv[2], "0")) replacement = getenv("S286_CPU_ONE");
        if (replacement && strcmp(argv[2], replacement)) {
            const char *log = getenv("S286_TASKSET_LOG");
            if (!log) return 2;
            int fd = open(log, O_WRONLY | O_CREAT | O_APPEND, 0644);
            if (fd < 0) return 2;
            dprintf(fd, "taskset: %s -> %s\n", argv[2], replacement);
            close(fd);
            argv[2] = (char *)replacement;
        }
    }
    argv[0] = "/usr/bin/taskset.real";
    execv(argv[0], argv);
    perror("s286 taskset execv");
    return errno == ENOENT ? 127 : 126;
}

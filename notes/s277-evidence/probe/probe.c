/* s277 black-box probe: what the host's *at calls, O_DIRECTORY and fchmod
 * answer, read from their man pages' promises and measured here. No libc or
 * kernel source was read; every line is one call and its errno.
 * usage: probe DIR   (DIR must not exist; it is created and left for a look) */
#define _GNU_SOURCE 1
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

static void row(const char *what, int rc) {
  int e = errno;
  if (rc < 0)
    printf("%-58s rc=-1 errno=%d (%s)\n", what, e, strerror(e));
  else
    printf("%-58s rc=%d\n", what, rc);
}

static const char *kind(mode_t m) {
  if (S_ISREG(m)) return "file";
  if (S_ISDIR(m)) return "dir";
  if (S_ISLNK(m)) return "link";
  if (S_ISFIFO(m)) return "fifo";
  return "other";
}

static void alarm_hit(int s) { (void)s; write(1, "ALARM: the open parked\n", 23); _exit(3); }

int main(int argc, char **argv) {
  if (argc != 2) return 2;
  const char *top = argv[1];
  if (mkdir(top, 0700) != 0) { perror("mkdir top"); return 2; }
  if (chdir(top) != 0) return 2;
  int f = open("file", O_CREAT | O_WRONLY, 0644); write(f, "abc", 3); close(f);
  mkdir("sub", 0755);
  mkdir("full", 0755);
  close(open("full/x", O_CREAT | O_WRONLY, 0644));
  symlink("sub", "ldir");
  symlink("file", "lfile");
  symlink("nowhere", "dangling");
  mkfifo("fifo", 0644);
  signal(SIGALRM, alarm_hit);

  printf("== O_DIRECTORY on open(2)\n");
  errno = 0; row("open(file, RDONLY|DIRECTORY)", f = open("file", O_RDONLY | O_DIRECTORY)); if (f >= 0) close(f);
  errno = 0; row("open(sub, RDONLY|DIRECTORY)", f = open("sub", O_RDONLY | O_DIRECTORY)); if (f >= 0) close(f);
  errno = 0; row("open(ldir, RDONLY|DIRECTORY)", f = open("ldir", O_RDONLY | O_DIRECTORY)); if (f >= 0) close(f);
  errno = 0; row("open(ldir, RDONLY|DIRECTORY|NOFOLLOW)", f = open("ldir", O_RDONLY | O_DIRECTORY | O_NOFOLLOW)); if (f >= 0) close(f);
  errno = 0; row("open(lfile, RDONLY|DIRECTORY|NOFOLLOW)", f = open("lfile", O_RDONLY | O_DIRECTORY | O_NOFOLLOW)); if (f >= 0) close(f);
  errno = 0; row("open(lfile, RDONLY|DIRECTORY)", f = open("lfile", O_RDONLY | O_DIRECTORY)); if (f >= 0) close(f);
  errno = 0; row("open(dangling, RDONLY|DIRECTORY)", f = open("dangling", O_RDONLY | O_DIRECTORY)); if (f >= 0) close(f);
  alarm(3);
  errno = 0; row("open(fifo, RDONLY|DIRECTORY) (no writer)", f = open("fifo", O_RDONLY | O_DIRECTORY)); if (f >= 0) close(f);
  alarm(0);
  errno = 0; row("open(sub, WRONLY|DIRECTORY)", f = open("sub", O_WRONLY | O_DIRECTORY)); if (f >= 0) close(f);
  errno = 0; row("open(sub, RDWR|DIRECTORY)", f = open("sub", O_RDWR | O_DIRECTORY)); if (f >= 0) close(f);
  errno = 0; row("open(newname, RDWR|CREAT|DIRECTORY)", f = open("newname", O_RDWR | O_CREAT | O_DIRECTORY, 0644)); if (f >= 0) close(f);
  struct stat st;
  errno = 0; row("  then lstat(newname)", lstat("newname", &st));
  errno = 0; row("open(sub, RDONLY|NONBLOCK|DIRECTORY)", f = open("sub", O_RDONLY | O_NONBLOCK | O_DIRECTORY)); if (f >= 0) close(f);
  errno = 0; row("open(file/x, RDONLY|DIRECTORY)", f = open("file/x", O_RDONLY | O_DIRECTORY)); if (f >= 0) close(f);

  printf("== openat\n");
  int d = open(".", O_RDONLY | O_DIRECTORY);
  int ff = open("file", O_RDONLY);
  errno = 0; row("openat(d, file, RDONLY)", f = openat(d, "file", O_RDONLY)); if (f >= 0) close(f);
  errno = 0; row("openat(file-fd, x, RDONLY)", f = openat(ff, "x", O_RDONLY)); if (f >= 0) close(f);
  errno = 0; row("openat(d, \"\", RDONLY)", f = openat(d, "", O_RDONLY)); if (f >= 0) close(f);
  errno = 0; row("openat(d, /, RDONLY|DIRECTORY) (absolute: dirfd ignored)", f = openat(d, "/", O_RDONLY | O_DIRECTORY)); if (f >= 0) close(f);
  errno = 0; row("openat(d, lfile, RDONLY|NOFOLLOW)", f = openat(d, "lfile", O_RDONLY | O_NOFOLLOW)); if (f >= 0) close(f);
  errno = 0; row("openat(d, ldir/.., RDONLY|DIRECTORY)", f = openat(d, "ldir/..", O_RDONLY | O_DIRECTORY)); if (f >= 0) close(f);
  errno = 0; row("openat(d, lfile, RDWR|CREAT|EXCL|NOFOLLOW)", f = openat(d, "lfile", O_RDWR | O_CREAT | O_EXCL | O_NOFOLLOW, 0644)); if (f >= 0) close(f);
  errno = 0; row("openat(d, dangling, WRONLY|CREAT|NOFOLLOW)", f = openat(d, "dangling", O_WRONLY | O_CREAT | O_NOFOLLOW, 0644)); if (f >= 0) close(f);
  errno = 0; row("openat(-1, file, RDONLY) (EBADF)", f = openat(-1, "file", O_RDONLY)); if (f >= 0) close(f);

  printf("== fstatat\n");
  errno = 0; row("fstatat(d, lfile, NOFOLLOW)", fstatat(d, "lfile", &st, AT_SYMLINK_NOFOLLOW)); printf("   kind=%s size=%lld\n", kind(st.st_mode), (long long)st.st_size);
  errno = 0; row("fstatat(d, lfile, 0)", fstatat(d, "lfile", &st, 0)); printf("   kind=%s size=%lld\n", kind(st.st_mode), (long long)st.st_size);
  errno = 0; row("fstatat(d, dangling, 0)", fstatat(d, "dangling", &st, 0));
  errno = 0; row("fstatat(d, dangling, NOFOLLOW)", fstatat(d, "dangling", &st, AT_SYMLINK_NOFOLLOW));
  errno = 0; row("fstatat(d, file/x, NOFOLLOW)", fstatat(d, "file/x", &st, AT_SYMLINK_NOFOLLOW));
  errno = 0; row("fstatat(file-fd, x, NOFOLLOW)", fstatat(ff, "x", &st, AT_SYMLINK_NOFOLLOW));
  errno = 0; row("fstatat(d, missing, NOFOLLOW)", fstatat(d, "missing", &st, AT_SYMLINK_NOFOLLOW));
  errno = 0; row("fstatat(d, \"\", NOFOLLOW)", fstatat(d, "", &st, AT_SYMLINK_NOFOLLOW));

  printf("== unlinkat\n");
  errno = 0; row("unlinkat(d, sub, 0) (a directory)", unlinkat(d, "sub", 0));
  errno = 0; row("unlinkat(d, file, REMOVEDIR) (a file)", unlinkat(d, "file", AT_REMOVEDIR));
  errno = 0; row("unlinkat(d, ldir, REMOVEDIR) (a link to a dir)", unlinkat(d, "ldir", AT_REMOVEDIR));
  errno = 0; row("unlinkat(d, full, REMOVEDIR) (not empty)", unlinkat(d, "full", AT_REMOVEDIR));
  errno = 0; row("unlinkat(d, missing, 0)", unlinkat(d, "missing", 0));
  errno = 0; row("unlinkat(file-fd, x, 0)", unlinkat(ff, "x", 0));
  errno = 0; row("unlinkat(d, ., REMOVEDIR)", unlinkat(d, ".", AT_REMOVEDIR));

  printf("== mkdirat\n");
  errno = 0; row("mkdirat(d, sub, 0700) (exists)", mkdirat(d, "sub", 0700));
  errno = 0; row("mkdirat(d, dangling, 0700) (a dangling link)", mkdirat(d, "dangling", 0700));
  errno = 0; row("mkdirat(d, no/such, 0700)", mkdirat(d, "no/such", 0700));
  errno = 0; row("mkdirat(d, made, 0700)", mkdirat(d, "made", 0700));
  errno = 0; row("mkdirat(file-fd, x, 0700)", mkdirat(ff, "x", 0700));

  printf("== symlinkat\n");
  errno = 0; row("symlinkat(\"\", d, empty)", symlinkat("", d, "empty"));
  errno = 0; row("symlinkat(t, d, sub) (exists)", symlinkat("t", d, "sub"));
  errno = 0; row("symlinkat(no/such/place, d, s1)", symlinkat("no/such/place", d, "s1"));
  char buf[256]; ssize_t n = readlinkat(d, "s1", buf, sizeof buf);
  printf("   readlinkat(d, s1) = %.*s\n", (int)(n < 0 ? 0 : n), buf);
  errno = 0; row("symlinkat(t, file-fd, x)", symlinkat("t", ff, "x"));

  printf("== linkat\n");
  errno = 0; row("linkat(d, lfile, d, h0, 0)", linkat(d, "lfile", d, "h0", 0));
  lstat("h0", &st); printf("   h0 kind=%s nlink=%lld\n", kind(st.st_mode), (long long)st.st_nlink);
  errno = 0; row("linkat(d, lfile, d, h1, SYMLINK_FOLLOW)", linkat(d, "lfile", d, "h1", AT_SYMLINK_FOLLOW));
  lstat("h1", &st); printf("   h1 kind=%s nlink=%lld\n", kind(st.st_mode), (long long)st.st_nlink);
  errno = 0; row("linkat(d, dangling, d, h2, SYMLINK_FOLLOW)", linkat(d, "dangling", d, "h2", AT_SYMLINK_FOLLOW));
  errno = 0; row("linkat(d, dangling, d, h3, 0)", linkat(d, "dangling", d, "h3", 0));
  errno = 0; row("linkat(d, sub, d, h4, 0) (a directory)", linkat(d, "sub", d, "h4", 0));
  errno = 0; row("linkat(d, file, d, sub, 0) (exists)", linkat(d, "file", d, "sub", 0));
  errno = 0; row("linkat(d, file, file-fd, x, 0)", linkat(d, "file", ff, "x", 0));
  errno = 0; row("linkat(d, missing, d, h5, 0)", linkat(d, "missing", d, "h5", 0));

  printf("== fchmod\n");
  int p[2]; pipe(p);
  errno = 0; row("fchmod(pipe-read-end, 0600)", fchmod(p[0], 0600));
  errno = 0; row("fchmod(d, 0700) (a directory handle)", fchmod(d, 0700));
  errno = 0; row("fchmod(file-fd O_RDONLY, 04755)", fchmod(ff, 04755));
  fstat(ff, &st); printf("   mode=%o\n", (unsigned)(st.st_mode & 07777));
  errno = 0; row("fchmod(-1, 0600)", fchmod(-1, 0600));
  int sd = open("sub", O_RDONLY | O_DIRECTORY);
  rename("file", "sub2"); /* nothing: placeholder for the reader */
  rename("sub2", "file");
  close(sd);

  printf("== fdopendir on a dup, twice\n");
  for (int pass = 1; pass <= 2; pass++) {
    int dd = dup(d);
    DIR *dp = fdopendir(dd);
    if (!dp) { row("fdopendir", -1); break; }
    rewinddir(dp);
    int count = 0, typed = 0;
    struct dirent *e;
    while ((e = readdir(dp)) != NULL) {
      if (strcmp(e->d_name, ".") == 0 || strcmp(e->d_name, "..") == 0) continue;
      count++;
      if (e->d_type != DT_UNKNOWN) typed++;
    }
    closedir(dp);
    printf("   pass %d: %d entries, %d typed by d_type\n", pass, count, typed);
  }
  errno = 0; row("fdopendir(dup(file-fd))", fdopendir(dup(ff)) ? 0 : -1);

  printf("== depth past PATH_MAX (%d)\n", PATH_MAX);
  mkdir("deep", 0700);
  int cur = openat(d, "deep", O_RDONLY | O_DIRECTORY);
  int depth = 0;
  const char *seg = "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz01"; /* 64 bytes */
  while (depth < 100) {
    if (mkdirat(cur, seg, 0700) != 0) { row("mkdirat deep", -1); break; }
    int nx = openat(cur, seg, O_RDONLY | O_DIRECTORY | O_NOFOLLOW);
    if (nx < 0) { row("openat deep", -1); break; }
    close(cur); cur = nx; depth++;
  }
  printf("   made %d levels of 65 bytes: %d bytes of path\n", depth, depth * 65 + 4);
  close(cur);
  char *path = malloc(100 * 65 + 16);
  strcpy(path, "deep");
  for (int i = 0; i < depth; i++) { strcat(path, "/"); strcat(path, seg); }
  errno = 0; row("stat(the whole path)", stat(path, &st));
  errno = 0; row("rmdir(the whole path)", rmdir(path));
  /* remove by handles: walk down, then up removing */
  int fds[101]; fds[0] = openat(d, "deep", O_RDONLY | O_DIRECTORY);
  for (int i = 1; i <= depth; i++) fds[i] = openat(fds[i - 1], seg, O_RDONLY | O_DIRECTORY | O_NOFOLLOW);
  int okr = 0;
  for (int i = depth - 1; i >= 0; i--) { if (unlinkat(fds[i], seg, AT_REMOVEDIR) == 0) okr++; close(fds[i + 1]); }
  close(fds[0]);
  printf("   removed %d of %d levels by handle\n", okr, depth);
  errno = 0; row("unlinkat(d, deep, REMOVEDIR)", unlinkat(d, "deep", AT_REMOVEDIR));
  return 0;
}

/* s277 linux black box: statx(2) through syscall(2) with a struct laid out
 * from the man page's field list (its padding omitted there: natural
 * alignment, the record 256 bytes), every word compared against fstatat(2)'s
 * for the same entries. Not the C library's header struct: ours. */
#define _GNU_SOURCE 1
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/sysmacros.h>
#include <unistd.h>

struct ts { int64_t sec; uint32_t nsec; int32_t pad; };
struct sx {
  uint32_t mask, blksize; uint64_t attributes;
  uint32_t nlink, uid, gid; uint16_t mode; uint16_t pad0;
  uint64_t ino, size, blocks, attributes_mask;
  struct ts atime, btime, ctime, mtime;
  uint32_t rdev_major, rdev_minor, dev_major, dev_minor;
  uint64_t rest[14];
};

static int one(int d, const char *name, int flags) {
  struct sx x; struct stat st;
  memset(&x, 0, sizeof x);
  long rc = syscall(SYS_statx, d, name, flags, 0x7ff | 0x800, &x);
  int e1 = errno;
  int rc2 = fstatat(d, name, &st, flags);
  int e2 = errno;
  if (rc != 0 || rc2 != 0) { printf("%-10s statx rc=%ld errno=%d  fstatat rc=%d errno=%d\n", name, rc, rc ? e1 : 0, rc2, rc2 ? e2 : 0); return 0; }
  int bad = 0;
#define EQ(a, b) do { if ((uint64_t)(a) != (uint64_t)(b)) { printf("   %s: statx %llu fstatat %llu\n", #a, (unsigned long long)(a), (unsigned long long)(b)); bad++; } } while (0)
  EQ(x.mode, st.st_mode); EQ(x.nlink, st.st_nlink); EQ(x.uid, st.st_uid); EQ(x.gid, st.st_gid);
  EQ(x.ino, st.st_ino); EQ(x.size, st.st_size); EQ(x.blocks, st.st_blocks);
  EQ(x.atime.sec, st.st_atim.tv_sec); EQ(x.atime.nsec, st.st_atim.tv_nsec);
  EQ(x.mtime.sec, st.st_mtim.tv_sec); EQ(x.mtime.nsec, st.st_mtim.tv_nsec);
  EQ(x.ctime.sec, st.st_ctim.tv_sec); EQ(x.ctime.nsec, st.st_ctim.tv_nsec);
  EQ(makedev(x.dev_major, x.dev_minor), st.st_dev); EQ(makedev(x.rdev_major, x.rdev_minor), st.st_rdev);
  printf("%-10s flags=%#x mask=%#x btime=%s%lld.%09u words-equal=%s\n", name, flags, x.mask,
         (x.mask & 0x800) ? "" : "(absent) ", (long long)x.btime.sec, x.btime.nsec, bad ? "NO" : "yes");
  return bad;
}

int main(int argc, char **argv) {
  if (argc != 2) return 2;
  mkdir(argv[1], 0700);
  if (chdir(argv[1])) return 2;
  close(open("file", O_CREAT | O_WRONLY, 0640));
  mkdir("sub", 0750); symlink("file", "lfile"); mkfifo("fifo", 0600);
  int d = open(".", O_RDONLY | O_DIRECTORY);
  printf("sizeof(our struct)=%zu\n", sizeof(struct sx));
  int bad = 0;
  bad += one(d, "file", 0); bad += one(d, "sub", 0); bad += one(d, "lfile", AT_SYMLINK_NOFOLLOW);
  bad += one(d, "lfile", 0); bad += one(d, "fifo", AT_SYMLINK_NOFOLLOW);
  int dev = open("/dev", O_RDONLY | O_DIRECTORY);
  bad += one(dev, "null", 0);
  printf("mismatches=%d\n", bad);
  return bad != 0;
}

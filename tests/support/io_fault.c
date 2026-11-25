#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
/* Test-process-only failures for TermCraft-owned staging files. Never installed globally. */
static int staging(int fd) {
    char name[64], target[4096];
    snprintf(name, sizeof(name), "/proc/self/fd/%d", fd);
    ssize_t n = readlink(name, target, sizeof(target)-1);
    if (n < 0) return 0;
    target[n] = 0;
    return strstr(target, "/.termcraft-") != NULL;
}
static int mode(const char *expected) {
    const char *value = getenv("TERMCRAFT_IO_FAULT_MODE");
    return value && strcmp(value, expected) == 0;
}
ssize_t write(int fd, const void *data, size_t n) {
    ssize_t (*real_write)(int, const void*, size_t) = dlsym(RTLD_NEXT, "write");
    if (mode("write") && staging(fd) && lseek(fd, 0, SEEK_CUR) >= 65536) {errno = ENOSPC; return -1;}
    return real_write(fd, data, n);
}
int fsync(int fd) {
    int (*real_fsync)(int) = dlsym(RTLD_NEXT, "fsync");
    if (mode("sync") && staging(fd)) {errno = ENOSPC; return -1;}
    return real_fsync(fd);
}
int close(int fd) {
    int (*real_close)(int) = dlsym(RTLD_NEXT, "close");
    int fail = mode("close") && staging(fd);
    int result = real_close(fd);
    if (fail) {errno = EIO; return -1;}
    return result;
}

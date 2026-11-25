#define _GNU_SOURCE
#include <dlfcn.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>

/* Loaded only by children of the owned test sshd. Publication succeeds, then its reply is lost. */
static int corrupt_reply = 0;
static int paused = 0;
static void pause_step(int directory, const char *name) {
    if (paused || (strncmp(name, "cancel-step-", 12) != 0 && strncmp(name, "delete-step-", 12) != 0)) return;
    paused = 1;
    int marker = openat(directory, ".termcraft-paused", O_WRONLY | O_CREAT | O_EXCL, 0600);
    if (marker >= 0) close(marker);
    for (int i = 0; i < 500 && faccessat(directory, ".termcraft-release", F_OK, 0) < 0; i++) usleep(10000);
}
static void lose_reply(int result, const char *destination) {
    if (result == 0 && strcmp(destination, "malformed-ack-target") == 0) corrupt_reply = 1;
    if (result == 0 && strcmp(destination, "lost-ack-target") == 0)
        close(STDOUT_FILENO);
}
int renameat(int source_fd, const char *source, int target_fd, const char *target) {
    int (*next)(int, const char *, int, const char *) = dlsym(RTLD_NEXT, "renameat");
    int result = next(source_fd, source, target_fd, target);
    lose_reply(result, target);
    if (result == 0) pause_step(target_fd, target);
    return result;
}
int renameat2(int source_fd, const char *source, int target_fd, const char *target, unsigned int flags) {
    int (*next)(int, const char *, int, const char *, unsigned int) = dlsym(RTLD_NEXT, "renameat2");
    int result = next(source_fd, source, target_fd, target, flags);
    lose_reply(result, target);
    if (result == 0) pause_step(target_fd, target);
    return result;
}

ssize_t write(int fd, const void *buffer, size_t length) {
    ssize_t (*next)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    if (fd == STDOUT_FILENO && corrupt_reply && length >= 16 && memcmp(buffer, "TC_EDITOR_SAVED:", 16) == 0) {
        const char malformed[] = "TC_EDITOR_SAVED:zz\n";
        if (next(fd, malformed, sizeof(malformed) - 1) < 0) return -1;
        return length;
    }
    return next(fd, buffer, length);
}

int unlinkat(int directory, const char *name, int flags) {
    int (*next)(int, const char *, int) = dlsym(RTLD_NEXT, "unlinkat");
    int result = next(directory, name, flags);
    if (result == 0) pause_step(directory, name);
    return result;
}

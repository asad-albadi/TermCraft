#define _GNU_SOURCE
#include <time.h>
#include <dlfcn.h>
static struct timespec delayed(const struct timespec *requested) {
    struct timespec actual = *requested;
    if (actual.tv_sec == 0 && actual.tv_nsec == 10000000L)
        actual.tv_nsec = 80000000L;
    return actual;
}
int nanosleep(const struct timespec *requested, struct timespec *remaining) {
    int (*real_sleep)(const struct timespec *, struct timespec *) = dlsym(RTLD_NEXT, "nanosleep");
    struct timespec actual = delayed(requested);
    return real_sleep(&actual, remaining);
}
int clock_nanosleep(clockid_t clock, int flags, const struct timespec *requested, struct timespec *remaining) {
    int (*real_sleep)(clockid_t, int, const struct timespec *, struct timespec *) = dlsym(RTLD_NEXT, "clock_nanosleep");
    struct timespec actual = flags == 0 ? delayed(requested) : *requested;
    return real_sleep(clock, flags, &actual, remaining);
}

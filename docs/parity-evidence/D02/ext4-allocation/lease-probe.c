#define _GNU_SOURCE
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <unistd.h>
#include <errno.h>
#include <sys/syscall.h>
#include <assert.h>
int main(int argc, char **argv) {
  sigset_t sigs; sigemptyset(&sigs); sigaddset(&sigs, SIGIO); assert(sigprocmask(SIG_BLOCK,&sigs,NULL)==0);
  int fd=open(argv[1],O_CREAT|O_RDWR|O_TRUNC,0600); assert(fd>=0);
  assert(write(fd,"original",8)==8);
  int duplicate=dup(fd); assert(duplicate>=0);
  struct f_owner_ex owner={.type=F_OWNER_TID,.pid=syscall(SYS_gettid)};
  assert(fcntl(fd,F_SETOWN_EX,&owner)==0);
  int r=fcntl(fd,F_SETLEASE,F_WRLCK); printf("lease with dup=%d errno=%d\n",r,errno); assert(r==0);
  int other=open(argv[1],O_WRONLY|O_NONBLOCK); printf("competing open=%d errno=%d, lease target=%d\n",other,errno,fcntl(fd,F_GETLEASE)); assert(other<0 && errno==EWOULDBLOCK);
  assert(fcntl(fd,F_SETLEASE,F_UNLCK)==0);
  struct timespec no_wait={0}; printf("break notification=%d\n",sigtimedwait(&sigs,NULL,&no_wait));
  other=open(argv[1],O_WRONLY|O_APPEND); assert(other>=0);
  errno=0; r=fcntl(fd,F_SETLEASE,F_WRLCK); printf("lease with independent writer=%d errno=%d\n",r,errno); assert(r<0 && errno==EAGAIN);
  close(other); close(duplicate); close(fd); unlink(argv[1]);
}

#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>
#include <errno.h>
#include <sys/wait.h>
#include <assert.h>
int main(int argc,char **argv) {
 (void)argc;
 int fd=open(argv[1],O_CREAT|O_RDWR|O_TRUNC|O_CLOEXEC,0600); assert(fd>=0);
 int other=open(argv[1],O_RDWR|O_CLOEXEC); assert(other>=0);
 int ready[2],done[2]; assert(pipe(ready)==0 && pipe(done)==0);
 pid_t child=fork(); assert(child>=0);
 if(!child){ close(ready[0]);close(done[1]);close(fd);assert(write(ready[1],"r",1)==1);char c;assert(read(done[0],&c,1)==1);close(other);_exit(0); }
 close(ready[1]);close(done[0]);close(other);char c;assert(read(ready[0],&c,1)==1);
 errno=0;int rc=fcntl(fd,F_SETLEASE,F_WRLCK);printf("parent closed independent remote FD; child inherited CLOEXEC FD: lease=%d errno=%d\n",rc,errno);assert(rc<0 && errno==EAGAIN);
 assert(write(done[1],"d",1)==1);int status;assert(waitpid(child,&status,0)==child && status==0);
 errno=0;rc=fcntl(fd,F_SETLEASE,F_WRLCK);printf("child closed inherited FD: lease=%d errno=%d\n",rc,errno);assert(rc==0);assert(fcntl(fd,F_SETLEASE,F_UNLCK)==0);close(fd);unlink(argv[1]);
}

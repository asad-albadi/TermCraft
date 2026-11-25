#include <nfsc/libnfs.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#define CHECK(call) do {int r=(call);if(r<0){fprintf(stderr,"%s failed %d: %s\n",#call,r,nfs_get_error(nfs));return 1;}}while(0)
#define DENY(call) do {int r=(call);if(r>=0){fprintf(stderr,"%s unexpectedly succeeded\n",#call);return 1;}}while(0)
int main(int argc,char **argv){
 if(argc!=3&&argc!=5)return 2;
 struct nfs_context *nfs=nfs_init_context();
 nfs_set_version(nfs,3);nfs_set_nfsport(nfs,atoi(argv[1]));nfs_set_mountport(nfs,atoi(argv[1]));nfs_set_timeout(nfs,3000);
 CHECK(nfs_mount(nfs,"127.0.0.1","/share"));
 struct nfsdir *dir;CHECK(nfs_opendir(nfs,"/",&dir));int found=0;struct nfsdirent *entry;
 while((entry=nfs_readdir(nfs,dir)))if(!strcmp(entry->name,"existing"))found=1;
 nfs_closedir(nfs,dir);if(!found){fprintf(stderr,"listing missing existing fixture\n");return 1;}
 struct nfsfh *fh;CHECK(nfs_open(nfs,"/existing",O_RDONLY,&fh));char buffer[32]={0};
 int read=nfs_pread(nfs,fh,buffer,8,0);if(read!=8||memcmp(buffer,"original",8)){fprintf(stderr,"read mismatch %d\n",read);return 1;}CHECK(nfs_close(nfs,fh));
 if(!strcmp(argv[2],"hardlink")){
  if(argc!=5)return 2;
  CHECK(nfs_open(nfs,"/existing",O_RDWR,&fh));
  if(link(argv[3],argv[4])){perror("fixture hardlink");return 1;}
  int bad=0,r=nfs_pwrite(nfs,fh,"CHANGED!",8,0);if(r>=0){fprintf(stderr,"hardlink WRITE unexpectedly succeeded\n");bad=1;}
  r=nfs_ftruncate(nfs,fh,0);if(r>=0){fprintf(stderr,"hardlink SETATTR truncate unexpectedly succeeded\n");bad=1;}
  CHECK(nfs_close(nfs,fh));
  r=nfs_open(nfs,"/existing",O_RDWR,&fh);if(r>=0){fprintf(stderr,"hardlink mutable open unexpectedly succeeded\n");bad=1;nfs_close(nfs,fh);}
  r=nfs_creat(nfs,"/existing",0600,&fh);if(r>=0){fprintf(stderr,"hardlink UNCHECKED CREATE unexpectedly succeeded\n");bad=1;nfs_close(nfs,fh);}
  nfs_destroy_context(nfs);if(bad)return 1;puts("libnfs hardlink WRITE/SETATTR/UNCHECKED CREATE denied");return 0;
 }else if(!strcmp(argv[2],"ro")){
  DENY(nfs_open(nfs,"/existing",O_RDWR,&fh));
  DENY(nfs_creat(nfs,"/new",0600,&fh));DENY(nfs_unlink(nfs,"/existing"));DENY(nfs_rename(nfs,"/existing","/renamed"));DENY(nfs_mkdir(nfs,"/newdir"));
 }else{
  CHECK(nfs_open(nfs,"/existing",O_RDWR,&fh));CHECK(nfs_pwrite(nfs,fh,"updated!",8,0));CHECK(nfs_close(nfs,fh));
  CHECK(nfs_creat(nfs,"/new",0600,&fh));CHECK(nfs_pwrite(nfs,fh,"hello",5,0));CHECK(nfs_close(nfs,fh));
  CHECK(nfs_rename(nfs,"/new","/renamed"));CHECK(nfs_open(nfs,"/renamed",O_RDONLY,&fh));memset(buffer,0,sizeof(buffer));if(nfs_pread(nfs,fh,buffer,5,0)!=5||memcmp(buffer,"hello",5))return 1;CHECK(nfs_close(nfs,fh));
  CHECK(nfs_rename(nfs,"/renamed","/replace-target"));CHECK(nfs_open(nfs,"/replace-target",O_RDONLY,&fh));memset(buffer,0,sizeof(buffer));if(nfs_pread(nfs,fh,buffer,5,0)!=5||memcmp(buffer,"hello",5))return 1;CHECK(nfs_close(nfs,fh));CHECK(nfs_rename(nfs,"/replace-target","/renamed"));
  CHECK(nfs_open(nfs,"/existing",O_RDWR|O_TRUNC,&fh));CHECK(nfs_pwrite(nfs,fh,"updated!",8,0));CHECK(nfs_close(nfs,fh));CHECK(nfs_open(nfs,"/renamed",O_RDONLY,&fh));CHECK(nfs_unlink(nfs,"/renamed"));DENY(nfs_pread(nfs,fh,buffer,5,0));CHECK(nfs_close(nfs,fh));CHECK(nfs_mkdir(nfs,"/newdir"));CHECK(nfs_rmdir(nfs,"/newdir"));
 }
 nfs_destroy_context(nfs);puts("libnfs independent NFSv3 explicit-port operations passed");return 0;
}

#include <windows.h>
#include <stdio.h>
void menu(HMENU m,int depth){int n=GetMenuItemCount(m);if(depth>5)return;for(int i=0;i<n;i++){char s[512]={0};MENUITEMINFOA info={0};info.cbSize=sizeof(info);info.fMask=MIIM_STRING|MIIM_STATE|MIIM_ID|MIIM_SUBMENU;info.dwTypeData=s;info.cch=sizeof(s)-1;if(GetMenuItemInfoA(m,i,TRUE,&info)){printf("MENU depth=%d index=%d id=%u state=%u text=%s\n",depth,i,info.wID,info.fState,s);if(info.hSubMenu)menu(info.hSubMenu,depth+1);}}}
BOOL CALLBACK visit(HWND h,LPARAM x){char name[256]={0},cls[256]={0};GetWindowTextA(h,name,sizeof(name));GetClassNameA(h,cls,sizeof(cls));printf("WINDOW hwnd=%p visible=%d enabled=%d class=%s title=%s\n",h,IsWindowVisible(h),IsWindowEnabled(h),cls,name);HMENU m=GetMenu(h);if(m)menu(m,0);return TRUE;}
int main(){EnumWindows(visit,0);return 0;}

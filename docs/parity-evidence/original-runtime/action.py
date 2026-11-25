with open(p/'win32-menu-query.log','w') as f:
 subprocess.run(['/work/root/usr/bin/wine','/work/inspect.exe.so'],stdout=f,stderr=f,timeout=15)

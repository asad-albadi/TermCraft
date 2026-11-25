import os,sys,termios,tty,time
p='/evidence/pty-input.bin'; fd=sys.stdin.fileno(); old=termios.tcgetattr(fd)
try:
 tty.setraw(fd); print('B06_OWNED_PTY_CAPTURE_READY',flush=True)
 with open(p,'wb',buffering=0) as f:
  while True:
   b=os.read(fd,1); f.write(b)
   if b==b'!': break
 print('\r\nB06_OWNED_PTY_RESUMED',flush=True)
finally:termios.tcsetattr(fd,termios.TCSADRAIN,old)

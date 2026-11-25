import socket,struct,json,pathlib,sys
report=pathlib.Path(sys.argv[1]);server=socket.socket();server.bind(('127.0.0.1',0));server.listen();(report/'vnc-port').write_text(str(server.getsockname()[1]));c,_=server.accept()
def recv(n):
 b=b''
 while len(b)<n:
  x=c.recv(n-len(b))
  if not x:raise EOFError
  b+=x
 return b
c.sendall(b'RFB 003.008\n');assert recv(12)==b'RFB 003.008\n';c.sendall(b'\x01\x01');assert recv(1)==b'\x01';c.sendall(b'\0'*4);assert recv(1)==b'\x01'
w,h=640,480
c.sendall(struct.pack('>HH',w,h)+bytes([32,24,0,1])+struct.pack('>HHH',255,255,255)+bytes([16,8,0])+b'\0'*3+struct.pack('>I',8)+b'V06 Peer')
with open(report/'vnc-input.jsonl','w',buffering=1) as log:
 sent=False
 while True:
  msg=recv(1)[0]
  if msg==0:recv(19)
  elif msg==2:
   hdr=recv(3);recv(4*int.from_bytes(hdr[1:3],'big'))
  elif msg==3:
   recv(9)
   if not sent:
    pixels=bytearray()
    for y in range(h):
     for x in range(w):pixels.extend([int(x*255/w),int(y*255/h),120 if (x//40+y//40)%2 else 230,0])
    c.sendall(b'\0\0\0\x01'+struct.pack('>HHHHi',0,0,w,h,0)+pixels);sent=True
  elif msg==4:
   data=recv(7);log.write(json.dumps({'type':'key','hex':data.hex()})+'\n')
  elif msg==5:
   data=recv(5);mask,x,y=struct.unpack('>BHH',data);log.write(json.dumps({'type':'pointer','mask':mask,'x':x,'y':y})+'\n')
  elif msg==6:
   data=recv(7);text=recv(int.from_bytes(data[3:7],'big'));log.write(json.dumps({'type':'clipboard','hex':text.hex()})+'\n')
  else:raise AssertionError(msg)

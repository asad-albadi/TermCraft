#!/usr/bin/env python3
"""Fail-closed actual pinned Mosh UDP fixture. No host route/device modification.
Run under: unshare --user --map-root-user --net python run_mosh_roaming.py APP OUTPUT
"""
import ctypes,hashlib,io,json,os,pty,re,select,signal,socket,struct,subprocess,sys,tarfile,tempfile,termios,time,traceback
from pathlib import Path
APP=Path(sys.argv[1]).resolve(); OUT=Path(sys.argv[2]).resolve(); OUT.mkdir(parents=True,exist_ok=True)
assert os.readlink('/proc/self/ns/net') != os.environ['V04_HOST_NETNS'], 'requires private network namespace'
ctypes.CDLL(None).prctl(36,1,0,0,0) # subreaper owns detached mosh-server and descendants

def digest(b): return hashlib.sha256(b).hexdigest()
def cmd(*args): return subprocess.run(args,check=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,timeout=10).stdout
cmd('ip','link','set','lo','up')
manifest=json.loads((APP/'packaging/runtime-manifest.json').read_text()); archive=APP/'target/runtime-inputs/mosh-1.4.0-35-x86_64.pkg.tar.zst'
raw=archive.read_bytes(); expected=next(a['archive_sha256'] for a in manifest['artifacts'] if a['path'].endswith('/mosh-client')); assert digest(raw)==expected
payload=subprocess.run(['zstd','-dc'],input=raw,stdout=subprocess.PIPE,check=True,timeout=10).stdout
private=tempfile.TemporaryDirectory(prefix='v04-',dir=OUT); work=Path(private.name)
with tarfile.open(fileobj=io.BytesIO(payload)) as tar:
    for name in ['mosh-server','mosh-client']:
        m=tar.getmember('usr/bin/'+name); assert m.isfile() and 0<m.size<8*1024*1024
        data=tar.extractfile(m).read(); (work/name).write_bytes(data); (work/name).chmod(0o700)
provenance={'archive':str(archive),'archive_sha256':expected,'manifest_sha256':digest((APP/'packaging/runtime-manifest.json').read_bytes()),'executables':{n:digest((work/n).read_bytes()) for n in ['mosh-server','mosh-client']},'runner_sha256':digest(Path(__file__).read_bytes()),'network_namespace':os.readlink('/proc/self/ns/net'),'uname':os.uname()[:], 'server_version':cmd(str(work/'mosh-server'),'--version')}
provenance['ldd']={n:cmd('ldd',str(work/n)) for n in ['mosh-client','mosh-server']}
libs=set(re.findall(r'(/[^\s()]+)', '\n'.join(provenance['ldd'].values())))
provenance['native_library_sha256']={str(Path(p).resolve()):digest(Path(p).read_bytes()) for p in sorted(libs) if Path(p).is_file()}
source=APP/'target/runtime-inputs/mosh-1.4.0.tar.gz'; source_digest=next(a['source_sha256'] for a in manifest['artifacts'] if a['path'].endswith('/mosh-client')); assert digest(source.read_bytes())==source_digest
with tarfile.open(source) as tar: license_bytes=tar.extractfile('mosh-mosh-1.4.0/COPYING').read()
assert digest(license_bytes)==manifest['mosh_inputs']['source_license_sha256']
provenance['source_sha256']=source_digest; provenance['license_sha256']=digest(license_bytes)
provenance['signature_sha256']=digest(Path(str(archive)+'.sig').read_bytes()); provenance['previously_verified_signer_fingerprint']=manifest['mosh_inputs']['signer_fingerprint']

# Server command journals every received line, preserving a persistent process/state counter.
child=work/'command.py'; child.write_text('''import os,sys,termios,json
fd=sys.stdin.fileno(); t=termios.tcgetattr(fd); t[3]&=~termios.ECHO; termios.tcsetattr(fd,termios.TCSANOW,t)
j=open(sys.argv[1],"a",buffering=1); n=0
print("V04_READY",flush=True)
for line in sys.stdin:
 n+=1; value=line.strip(); j.write(json.dumps({"pid":os.getpid(),"n":n,"value":value})+"\\n"); print("ACK_%s_%d"%(value,n),flush=True)
''')
provenance['command_sha256']=digest(child.read_bytes())
(OUT/'provenance.json').write_text(json.dumps(provenance,indent=2))

def sock(address):
    s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM); s.bind((address,0)); s.setblocking(False); return s

def run_case(name,virtual=False):
    journal=work/(name+'.jsonl'); log=work/(name+'.server'); output=bytearray(); events=[]; counters={'forward':0,'return':0,'drop':0,'reordered':0}; owner=None; client=None; master=None; sockets=[]; held=None
    if virtual:
        cmd('ip','link','add','v04old','type','dummy'); cmd('ip','addr','add','10.203.0.1/32','dev','v04old'); cmd('ip','link','set','v04old','up')
        cmd('ip','link','add','v04new','type','dummy'); cmd('ip','addr','add','10.203.0.2/32','dev','v04new'); cmd('ip','link','set','v04new','up')
        old,new='10.203.0.1','10.203.0.2'
    else: old,new='127.0.0.2','127.0.0.3'
    network_before=cmd('ip','-j','addr','show')
    front=sock('127.0.0.1'); back=sock(old); sockets=[front,back]; client_addr=None; drop_until=0; reorder=False; observed_addresses=[]
    try:
        with log.open('wb') as f:
            launcher=subprocess.Popen([str(work/'mosh-server'),'new','-v','-i','127.0.0.1','-p','0','--',sys.executable,str(child),str(journal)],stdout=f,stderr=f,env={**os.environ,'LANG':'C.UTF-8','MOSH_SERVER_NETWORK_TMOUT':'20'},start_new_session=True)
        launcher.wait(timeout=8); assert launcher.returncode==0
        text=log.read_text(); match=re.search(r'MOSH CONNECT (\d+) (\S+)',text); assert match, text
        server=('127.0.0.1',int(match[1])); key=match[2]; owner=int(re.search(r'detached, pid = (\d+)',text)[1]); assert int(Path('/proc',str(owner),'stat').read_text().split()[3])==os.getpid()
        master,slave=pty.openpty(); termios.tcsetwinsize(slave,(35,120)); env={**os.environ,'MOSH_KEY':key,'MOSH_PREDICTION_DISPLAY':'never','TERM':'xterm-256color','LANG':'C.UTF-8'}
        client=subprocess.Popen([str(work/'mosh-client'),'127.0.0.1',str(front.getsockname()[1])],stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True); os.close(slave); os.set_blocking(master,False)
        def pump(seconds=0.04):
            nonlocal client_addr,held
            end=time.monotonic()+seconds
            while time.monotonic()<end:
                ready=select.select([front,back,master],[],[],min(.02,max(0,end-time.monotonic())))[0]
                for s in ready:
                    if s==master:
                        try: output.extend(os.read(master,65536))
                        except (BlockingIOError,OSError): pass
                        continue
                    data,addr=s.recvfrom(65536)
                    if s==front:
                        client_addr=addr
                        if time.monotonic()<drop_until: counters['drop']+=1; continue
                        if reorder and held is None: held=data; continue
                        back.sendto(data,server); counters['forward']+=1
                        if held is not None: back.sendto(held,server); held=None; counters['reordered']+=1
                    elif client_addr:
                        if time.monotonic()<drop_until: counters['drop']+=1; continue
                        front.sendto(data,client_addr); counters['return']+=1
        def wait_output(marker,timeout=12):
            end=time.monotonic()+timeout
            while marker not in output:
                assert time.monotonic()<end, ('output timeout',marker,bytes(output)[-1500:])
                assert client.poll() is None, ('client exited',client.returncode)
                pump()
        wait_output(b'V04_READY'); observed_addresses.append(back.getsockname()); events.append('ready')
        os.write(master,b'before\n'); wait_output(b'ACK_before_1'); assert journal.exists()
        # Retain the same client, server and command process; change authenticated UDP peer address/port.
        prior=back.getsockname(); back.close()
        if virtual: cmd('ip','link','del','v04old')
        back=sock(new); sockets.append(back); observed_addresses.append(back.getsockname()); assert back.getsockname()!=prior
        events.append('source-ip-and-port-rebound'+('-old-interface-deleted' if virtual else ''))
        network_after=cmd('ip','-j','addr','show')
        drop_until=time.monotonic()+1.2
        os.write(master,b'during_loss\n'); pump(1.3); assert counters['drop']>0
        wait_output(b'ACK_during_loss_2'); events.append('loss-recovered')
        reorder=True; os.write(master,b'reorder_one\nreorder_two\n'); wait_output(b'ACK_reorder_two_4',20); reorder=False
        if held is not None: back.sendto(held,server); held=None
        assert counters['reordered']>0
        os.write(master,b'after\n'); wait_output(b'ACK_after_5'); pump(.2)
        rows=[json.loads(x) for x in journal.read_text().splitlines()]; assert [r['value'] for r in rows]==['before','during_loss','reorder_one','reorder_two','after']; assert [r['n'] for r in rows]==[1,2,3,4,5]; assert len({r['pid'] for r in rows})==1
        # Escape disconnect while server is still live: bounded cancellation then explicit owned cleanup.
        started=time.monotonic(); os.write(master,b'\x1e.');
        while client.poll() is None and time.monotonic()-started<3: pump()
        assert client.poll() is not None, 'client cancel exceeded 3s'
        client.wait(timeout=1); cancel_seconds=time.monotonic()-started
        authenticated_peers=re.findall(r'Server now attached to client at ([0-9.]+):(\d+)',log.read_text())
        assert [(ip,int(port)) for ip,port in authenticated_peers]==observed_addresses, ('server did not authenticate both peer addresses',authenticated_peers,observed_addresses)
        return {'case':name,'status':'passed','scope':'private Linux dummy-interface source-address migration through owned UDP relay' if virtual else 'owned UDP relay source-IP/port rebinding on loopback','network_before':json.loads(network_before),'network_after':json.loads(network_after),'outage_seconds':1.2,'events':events,'counters':counters,'relay_upstream_addresses':observed_addresses,'authenticated_server_peers':authenticated_peers,'journal':rows,'cancel_seconds':cancel_seconds,'server_pid':owner,'client_pid':client.pid}
    finally:
        if client is not None and client.poll() is None: client.kill(); client.wait(timeout=3)
        if owner is not None:
            # server is our adopted child, so no unrelated PID signalling permitted.
            stat=Path('/proc',str(owner),'stat')
            if stat.exists() and int(stat.read_text().split()[3])==os.getpid(): os.kill(owner,signal.SIGTERM)
            end=time.monotonic()+3
            while stat.exists() and time.monotonic()<end:
                try: os.waitpid(owner,os.WNOHANG)
                except ChildProcessError: break
                time.sleep(.02)
            if stat.exists(): os.kill(owner,signal.SIGKILL); os.waitpid(owner,0)
        for s in sockets: s.close()
        if master is not None: os.close(master)
        # mosh terminal command is also owned and may outlive termination briefly.
        end=time.monotonic()+3
        while time.monotonic()<end:
            try:
                pid,status=os.waitpid(-1,os.WNOHANG)
                if pid==0: time.sleep(.02); continue
            except ChildProcessError: break
        else: raise AssertionError('owned descendants not fully reaped')
        (OUT/(name+'.terminal.bin')).write_bytes(output)
        if journal.exists(): (OUT/(name+'.journal.jsonl')).write_bytes(journal.read_bytes())
        if log.exists(): (OUT/(name+'.server.log')).write_text(re.sub(r'MOSH CONNECT (\d+) \S+',r'MOSH CONNECT \1 [REDACTED]',log.read_text()))
        if virtual: cmd('ip','link','del','v04new')

results=[]
try:
    for name,virtual in [('udp-rebind-loss-reordering',False),('private-interface-migration',True)]:
        result=run_case(name,virtual); result['owned_process_cleanup']='passed'; results.append(result)
    assert len(results)==2 and all(x['status']=='passed' for x in results)
    results.extend([{'case':'physical-network-adapter-migration','status':'unavailable','reason':'No designated physical network fixture; private dummy interfaces are emulated.'},{'case':'physical-serial-loopback-unplug','status':'unavailable','reason':'No designated adapter/device/wiring; no host devices opened.'},{'case':'native-Windows-macOS-original','status':'unavailable','reason':'No designated native/original runtime fixture.'}])
    (OUT/'results.json').write_text(json.dumps({'status':'partial','strict_native_original':0,'executed_passed':2,'failed':0,'unavailable':3,'cases':results},indent=2))
    print('PARTIAL: 2 actual pinned-Mosh UDP cases passed; 3 unavailable cells remain open; strict_native_original=0')
except BaseException as e:
    (OUT/'results.json').write_text(json.dumps({'status':'failed','error':str(e),'completed':results},indent=2)); traceback.print_exc(); sys.exit(1)
finally: private.cleanup()

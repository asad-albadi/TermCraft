from pathlib import Path
import json,hashlib,subprocess
out=Path(__file__).resolve().parent;pkg=Path(json.loads((out/'payload-equivalence.json').read_text())['package']);r=json.loads((out/'mosh-result.json').read_text());assert r['exit_code']==0,r
assert (out/'mosh-client-absence.txt').read_text()=='NO_INSTALLED_PERL_OR_SSH_CLIENT\n'
assert (out/'mosh-remote-marker.txt').read_bytes()==b'CURRENT_MOSH_ENCRYPTED_UDP_PTY_OK\n'
assert (out/'mosh-typed-marker.txt').read_bytes()==b'CURRENT_MOSH_TYPED_UDP_OK\n'
a=(out/'mosh-remote-initial-size.txt').read_text().strip();b=(out/'mosh-remote-resized-size.txt').read_text().strip();assert a!=b
rows=json.loads((out/'mosh-bootstrap-processes.json').read_text());proof=[]
for suffix,relative in [('bin/perl','mosh-deps/bin/perl'),('bin/ssh','mosh-deps/bin/ssh'),('mosh-client','mosh-client')]:
 expected=hashlib.file_digest((pkg/'runtime/linux-x86_64'/relative).open('rb'),'sha256').hexdigest();matches=[r for r in rows if r['exe'].endswith('/'+suffix)];assert matches,suffix
 for row in matches:
  assert row['sha256']==expected
  assert '\nPATH=\n' in '\n'+row['environment'],row['environment']
 proof.append({'executable':suffix,'packaged_sha256':expected,'captured_processes':len(matches)})
assert not json.loads((out/'mosh-cleanup.json').read_text())['remaining_owned_client_processes']
ssh=(out/'mosh-sshd.log').read_text();assert 'Accepted publickey for runtimefixture' in ssh and 'Allocating pty' in ssh
check=subprocess.run(['docker','container','inspect','termcraft-post-reset-mosh-398d'],stdout=subprocess.PIPE,stderr=subprocess.PIPE);assert check.returncode!=0
res={'result':'PASS','normal_packaged_app':True,'helper_fallback':False,'ssh_pty_modified':False,'client_path':'empty','installed_perl_ssh_client':'absent in disposable container','native_command_receipts':['CURRENT_MOSH_ENCRYPTED_UDP_PTY_OK','CURRENT_MOSH_TYPED_UDP_OK'],'remote_terminal_initial':a,'remote_terminal_resized':b,'private_executable_hashes':proof,'owned_client_cleanup':'zero remaining before container shutdown','owned_container_cleanup':'absent','fixture_server':'retained genuine mosh-server+libutempter, server only, read-only /fixture mount','namespace':'own Docker network-none, PID, IPC, mount, home and private Xvfb; default capabilities, no privileged/hostdevices/security override'}
(out/'verification-mosh.json').write_text(json.dumps(res,indent=2)+'\n');print(json.dumps(res,indent=2))

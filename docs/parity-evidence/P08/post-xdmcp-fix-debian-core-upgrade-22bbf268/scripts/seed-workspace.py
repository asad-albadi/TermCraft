from pathlib import Path
import json
p=Path('/tmp/qualification-home/workspace/workspace.json');s=json.loads(p.read_text());session=s['sessions'][0]
session.update({'user':'owned-fixture-user','folder':'Owned upgrade group','jump':'127.0.0.1','gateway_credential_ref':'owned-debian-fixture','startup_macro':'Owned saved macro'})
s['tunnels']=[{'name':'Owned saved tunnel','host':'127.0.0.1','user':'owned-fixture-user','ssh_port':22,'key':'','mode':'Local','bind':'127.0.0.1','local_port':18080,'destination':'127.0.0.1','remote_port':18081}]
s['macros']=[{'name':'Owned saved macro','text':'echo owned-offline-fixture','append_enter':False}]
s['preferences']['sidebar_width']=400.0
s['preferences']['clipboard']['copy_on_select']=True
s['preferences']['paste']['line_delay_ms']=12
for binding in s['preferences']['shortcuts']['bindings']:
 if binding['action']=='NewSession':binding['chord']={'key':'F6','command':False,'ctrl':False,'shift':False,'alt':False}
p.write_text(json.dumps(s,indent=2));p.chmod(0o600)
print('Seeded owned offline profile/gateway reference, nonempty tunnel/macro, custom shortcut and preferences')

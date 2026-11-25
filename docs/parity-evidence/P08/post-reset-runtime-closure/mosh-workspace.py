from pathlib import Path
import json
j=json.loads(Path('/evidence/prior-browser-fixture.json').read_text());s=j['sessions'][0];s.update({'id':'abcdef0123456789abcdef0123456789','name':'Current packaged genuine Mosh','protocol':'Mosh','host':'127.0.0.1','user':'runtimefixture','port':60001,'mosh_ssh_port':2222,'mosh_provider':'bundled','key':'/tmp/fixture/client','ssh_options':{'config_file':'/tmp/fixture/ssh_config'}})
Path('/tmp/store/workspace.json').write_text(json.dumps(j))

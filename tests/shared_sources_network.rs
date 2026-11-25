#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/ssh_server.rs"]
mod ssh_server;
use harbor_workspace::{
    model::{Session, Store},
    sessions::shared::{SharedFormat, SharedLocation, SharedSource, SharedSourceConfig},
    shared_network::{NetworkReader, SourceAuth, SourceChallenge},
    ssh::ChallengeAnswer,
};
use std::{
    fs,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
struct Server {
    root: tempfile::TempDir,
    child: Child,
    port: u16,
}
impl Server {
    fn start() -> Self {
        let root = tempfile::tempdir().unwrap();
        let run = |args: Vec<std::ffi::OsString>| {
            assert!(
                Command::new("openssl")
                    .args(args)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .unwrap()
                    .success()
            );
        };
        run(vec![
            "req".into(),
            "-x509".into(),
            "-newkey".into(),
            "rsa:2048".into(),
            "-nodes".into(),
            "-days".into(),
            "1".into(),
            "-subj".into(),
            "/CN=Owned source CA".into(),
            "-addext".into(),
            "basicConstraints=critical,CA:TRUE".into(),
            "-addext".into(),
            "keyUsage=critical,keyCertSign,cRLSign".into(),
            "-keyout".into(),
            root.path().join("ca.key").into_os_string(),
            "-out".into(),
            root.path().join("ca.pem").into_os_string(),
        ]);
        run(vec![
            "req".into(),
            "-new".into(),
            "-newkey".into(),
            "rsa:2048".into(),
            "-nodes".into(),
            "-subj".into(),
            "/CN=localhost".into(),
            "-keyout".into(),
            root.path().join("key.pem").into_os_string(),
            "-out".into(),
            root.path().join("server.csr").into_os_string(),
        ]);
        fs::write(root.path().join("leaf.ext"),"basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost\n").unwrap();
        run(vec![
            "x509".into(),
            "-req".into(),
            "-in".into(),
            root.path().join("server.csr").into_os_string(),
            "-CA".into(),
            root.path().join("ca.pem").into_os_string(),
            "-CAkey".into(),
            root.path().join("ca.key").into_os_string(),
            "-CAcreateserial".into(),
            "-days".into(),
            "1".into(),
            "-extfile".into(),
            root.path().join("leaf.ext").into_os_string(),
            "-out".into(),
            root.path().join("cert.pem").into_os_string(),
        ]);
        let mut payload = Store::default();
        payload.sessions.push(Session {
            name: "Team".into(),
            host: "shared.test".into(),
            ..Default::default()
        });
        payload.save(&root.path().join("source.json")).unwrap();
        let script = r#"import http.server,ssl,sys,pathlib
root=pathlib.Path(sys.argv[1])
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args): pass
 def do_GET(self):
  with open(root/'requests','a') as f: f.write(self.path+'\n')
  if self.path=='/redirect':
   self.send_response(302);self.send_header('Location','http://127.0.0.1:'+str(self.server.server_port)+'/source');self.end_headers();return
  if self.path=='/large':
   self.send_response(200);self.send_header('Content-Length',str(8*1024*1024+1));self.end_headers();return
  if self.path=='/private' and self.headers.get('Authorization')!='Basic dXNlcjpmaXh0dXJlLXBhc3N3b3Jk':
   self.send_response(401);self.send_header('WWW-Authenticate','Basic realm="owned fixture"');self.end_headers();return
  data=(root/'source.json').read_bytes();self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
server=http.server.HTTPServer(('127.0.0.1',0),Handler)
context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.load_cert_chain(str(root/'cert.pem'),str(root/'key.pem'));server.socket=context.wrap_socket(server.socket,server_side=True)
print(server.server_port,flush=True);server.serve_forever()
"#;
        let mut child = Command::new("python3")
            .args(["-u", "-c", script])
            .arg(root.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port = line.trim().parse().unwrap();
        Self { root, child, port }
    }
    fn reader(&self) -> NetworkReader {
        NetworkReader::new(self.root.path().to_owned(), vec![])
            .with_trusted_certificate(&fs::read(self.root.path().join("ca.pem")).unwrap())
            .unwrap()
    }
    fn config(&self, path: &str) -> SharedSourceConfig {
        SharedSourceConfig::from_url(
            "Owned HTTPS",
            &format!("https://localhost:{}{path}", self.port),
            SharedFormat::Workspace,
        )
        .unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct Auth(AtomicUsize);
impl SourceAuth for Auth {
    fn answer(&self, c: SourceChallenge, _: &AtomicBool) -> Result<ChallengeAnswer, String> {
        assert!(
            c.endpoint
                .as_ref()
                .unwrap()
                .starts_with("https://localhost:")
        );
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(ChallengeAnswer {
            id: c.answer_id,
            values: vec![
                "user".to_owned().into(),
                "fixture-password".to_owned().into(),
            ],
            trust: None,
        })
    }
}
#[test]
fn owned_https_source_checks_certificates_and_keeps_basic_credentials_out_of_cache() {
    let server = Server::start();
    let auth = Arc::new(Auth(AtomicUsize::new(0)));
    let reader = server.reader().with_auth(auth.clone());
    let mut source = SharedSource::with_reader(server.config("/private"), Arc::new(reader));
    let snapshot = source.refresh().unwrap();
    assert_eq!(snapshot.entries[0].session.host, "shared.test");
    assert_eq!(auth.0.load(Ordering::Relaxed), 1);
    assert!(
        !serde_json::to_string(source.config())
            .unwrap()
            .contains("fixture-password")
    );
    let mut untrusted = SharedSource::with_reader(
        server.config("/source"),
        Arc::new(NetworkReader::new(server.root.path().to_owned(), vec![])),
    );
    assert!(untrusted.refresh().is_err());
    let wrong = SharedSourceConfig::from_url(
        "Wrong name",
        &format!("https://127.0.0.1:{}/source", server.port),
        SharedFormat::Workspace,
    )
    .unwrap();
    let mut wrong = SharedSource::with_reader(wrong, Arc::new(server.reader()));
    assert!(wrong.refresh().is_err());
}
#[test]
fn https_redirect_and_oversized_source_are_rejected_without_partial_snapshot() {
    let server = Server::start();
    let mut redirect =
        SharedSource::with_reader(server.config("/redirect"), Arc::new(server.reader()));
    assert!(redirect.refresh().unwrap_err().contains("302"));
    assert_eq!(
        fs::read_to_string(server.root.path().join("requests")).unwrap(),
        "/redirect\n"
    );
    let mut large = SharedSource::with_reader(server.config("/large"), Arc::new(server.reader()));
    assert!(large.refresh().unwrap_err().contains("8 MiB"));
    assert!(large.snapshot().is_none());
}
#[test]
fn shared_sftp_source_uses_saved_embedded_identity_and_verified_host_without_a_shell() {
    let server = ssh_server::SshServer::start(true);
    let auth = server.config(9200, true);
    fs::create_dir_all(server.root.join("ssh")).unwrap();
    fs::copy(auth.app_known_hosts, server.root.join("ssh/known_hosts")).unwrap();
    let mut data = Store::default();
    data.sessions.push(Session {
        name: "Shared".into(),
        host: "shared.test".into(),
        ..Default::default()
    });
    let path = server.root.join("shared.json");
    data.save(&path).unwrap();
    let id = auth.session.id.clone();
    let reader =
        NetworkReader::new(server.root.clone(), vec![auth.session]).with_user_known_hosts(vec![]);
    let config = SharedSourceConfig::new(
        "SFTP team",
        SharedLocation::Sftp {
            session_id: id,
            path: path.to_str().unwrap().into(),
        },
        SharedFormat::Workspace,
    )
    .unwrap();
    let mut source = SharedSource::with_reader(config, Arc::new(reader));
    assert_eq!(
        source.refresh().unwrap().entries[0].session.host,
        "shared.test"
    );
    assert_eq!(server.accepted_authentications(), 1);
}

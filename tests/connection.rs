use harbor_workspace::{connection::*, model::*};
#[test]
fn sftp_escapes_quotes_and_rejects_newlines() {
    assert_eq!(
        sftp_quote("file with \"quote\"").unwrap(),
        "\"file with \\\"quote\\\"\""
    );
    assert!(sftp_quote("file\n!danger").is_err());
}
#[test]
fn missing_client_is_actionable() {
    let p = Preferences {
        shell: "/no/such/shell".into(),
        ..Default::default()
    };
    let s = Session {
        protocol: "Local".into(),
        ..Default::default()
    };
    assert!(launch(&s, &p).unwrap_err().contains("not installed"));
}

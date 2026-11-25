#![cfg(windows)]
use harbor_workspace::credentials::{Secret, vault::Vault};
use std::path::PathBuf;

#[test]
#[ignore = "requires an explicit synthetic vault fixture path"]
fn create_or_check_synthetic_vault_for_package_fixture() {
    let path = PathBuf::from(std::env::var_os("TERMCRAFT_TEST_VAULT_PATH").unwrap());
    let password = || Secret::new("TermCraft package fixture only");
    if !path.exists() {
        let mut vault = Vault::create(&path, password()).unwrap();
        vault
            .put("package-fixture".into(), Secret::new("fixture-value"))
            .unwrap();
    }
    let vault = Vault::unlock(&path, password()).unwrap();
    assert_eq!(
        vault.get(&"package-fixture".into()).unwrap().expose(),
        "fixture-value"
    );
}

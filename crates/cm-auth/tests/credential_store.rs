//! The OS credential store for real (platform plan W0, "Credential Manager smoke test"): writes one
//! entry named after the process under a test service, reads it back and deletes it. Windows only
//! (macOS would prompt for the Keychain; its round trip is the `--ignored` test in `secret.rs`).
//! Windows Credential Manager limits one entry to 2560 bytes (the keyring crate stores UTF-16), and
//! `VaultStore` keeps every secret of the app in one entry, so the test also prints how large an
//! entry can get here.

#![cfg(windows)]
#![allow(clippy::unwrap_used)]

use cm_auth::{KeyringStore, SecretStore, VaultStore};
use std::sync::Arc;

const SERVICE: &str = "dev.capturemann.cm-auth-smoke";

fn key(tag: &str) -> String {
    format!("smoke.{tag}.{}", std::process::id())
}

#[test]
fn a_secret_round_trips_through_credential_manager() {
    let s = KeyringStore::new(SERVICE);
    let k = key("one");
    assert_eq!(s.get(&k).unwrap(), None);
    s.set(&k, "päss wörd ✓").unwrap();
    assert_eq!(s.get(&k).unwrap().as_deref(), Some("päss wörd ✓"));
    s.set(&k, "second").unwrap();
    assert_eq!(s.get(&k).unwrap().as_deref(), Some("second"));
    s.delete(&k).unwrap();
    s.delete(&k).unwrap();
    assert_eq!(s.get(&k).unwrap(), None);
}

#[test]
fn how_large_one_entry_can_be() {
    let s = KeyringStore::new(SERVICE);
    let mut largest_ok = 0;
    for chars in [500, 1000, 1200, 1280, 1281, 2000, 4000, 20_000] {
        let k = key("size");
        let value = "a".repeat(chars);
        match s.set(&k, &value) {
            Ok(()) => {
                assert_eq!(s.get(&k).unwrap().as_deref(), Some(value.as_str()));
                largest_ok = chars;
            }
            Err(e) => println!("SIZE {chars} chars: {e}"),
        }
        s.delete(&k).unwrap();
    }
    println!("SIZE largest entry that worked: {largest_ok} chars");
    assert!(largest_ok >= 1000, "an OAuth token pair must fit");
}

#[test]
fn the_vault_keeps_several_accounts_in_the_real_store() {
    let service = format!("{SERVICE}.vault.{}", std::process::id());
    let vault = VaultStore::system(service.clone());
    for i in 0..6 {
        vault
            .set(&format!("oauth.google.acct{i}"), &"t".repeat(300))
            .unwrap();
        vault
            .set(&format!("oauth.google.acct{i}.client"), &"c".repeat(100))
            .unwrap();
    }
    let fresh = VaultStore::over(Arc::new(KeyringStore::new(service)));
    for i in 0..6 {
        assert_eq!(
            fresh
                .get(&format!("oauth.google.acct{i}"))
                .unwrap()
                .as_deref(),
            Some("t".repeat(300).as_str())
        );
        fresh.delete(&format!("oauth.google.acct{i}")).unwrap();
        fresh
            .delete(&format!("oauth.google.acct{i}.client"))
            .unwrap();
    }
}

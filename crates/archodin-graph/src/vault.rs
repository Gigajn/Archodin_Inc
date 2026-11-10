//! The secrets vault: Provider API keys, the GitHub token and other credentials,
//! stored encrypted (ChaCha20-Poly1305) and referenced by name. The caller supplies
//! the 32-byte key; the vault never persists it.
//!
//! A [`Secret`] never prints its value, and [`Vault::redact`] scrubs known values
//! out of text bound for events and logs. Rotating with [`Vault::put`] applies at
//! once, because a Model chain step resolves its key at call time
//! ([`Provider::from_vault`](crate::Provider::from_vault)).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, RwLock};

use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use zeroize::Zeroizing;

const NONCE_LEN: usize = 12;

/// Why a vault operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VaultError {
    /// No secret has this name.
    #[error("unknown secret `{0}`")]
    UnknownSecret(String),
    /// The data could not be decrypted: wrong key, or it was tampered with.
    #[error("could not decrypt the vault")]
    Decrypt,
    /// The sealed bytes are not a vault.
    #[error("malformed sealed vault")]
    Malformed,
}

/// A secret value handed out by the vault. It prints as `[secret]` and is wiped
/// from memory when dropped.
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// The value. Pass it only to the call that needs it, never to an Agent.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([secret])")
    }
}

/// Encrypted secrets, referenced by name. Cloning shares the same vault.
#[derive(Clone)]
pub struct Vault {
    inner: Arc<Inner>,
}

struct Inner {
    cipher: ChaCha20Poly1305,
    /// name -> nonce followed by ciphertext.
    entries: RwLock<BTreeMap<String, Vec<u8>>>,
}

impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vault")
            .field("secrets", &self.names())
            .finish()
    }
}

impl Vault {
    /// An empty vault that encrypts with `key`.
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            inner: Arc::new(Inner {
                cipher: ChaCha20Poly1305::new(&key.into()),
                entries: RwLock::new(BTreeMap::new()),
            }),
        }
    }

    fn encrypt(&self, name: &str, value: &str) -> Vec<u8> {
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = self
            .inner
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: value.as_bytes(),
                    aad: name.as_bytes(),
                },
            )
            .expect("encryption of a short value cannot fail");
        let mut out = nonce.to_vec();
        out.extend(ciphertext);
        out
    }

    fn decrypt(&self, name: &str, blob: &[u8]) -> Result<Secret, VaultError> {
        if blob.len() < NONCE_LEN {
            return Err(VaultError::Malformed);
        }
        let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
        let plain = self
            .inner
            .cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: name.as_bytes(),
                },
            )
            .map_err(|_| VaultError::Decrypt)?;
        let text = String::from_utf8(plain).map_err(|_| VaultError::Malformed)?;
        Ok(Secret(Zeroizing::new(text)))
    }

    /// Stores `value` under `name`, replacing (rotating) any earlier value.
    pub fn put(&self, name: &str, value: &str) {
        let blob = self.encrypt(name, value);
        self.inner
            .entries
            .write()
            .unwrap()
            .insert(name.to_string(), blob);
    }

    /// Removes the secret called `name`; true if there was one.
    pub fn remove(&self, name: &str) -> bool {
        self.inner.entries.write().unwrap().remove(name).is_some()
    }

    /// The names of all secrets, sorted. Values are never listed.
    pub fn names(&self) -> Vec<String> {
        self.inner.entries.read().unwrap().keys().cloned().collect()
    }

    /// Decrypts the secret called `name`.
    pub fn resolve(&self, name: &str) -> Result<Secret, VaultError> {
        let entries = self.inner.entries.read().unwrap();
        let blob = entries
            .get(name)
            .ok_or_else(|| VaultError::UnknownSecret(name.to_string()))?;
        self.decrypt(name, blob)
    }

    /// Replaces every known secret value in `text` with `[redacted]`, for text that
    /// may reach events, logs, Briefings or the Akasha.
    pub fn redact(&self, text: &str) -> String {
        let entries = self.inner.entries.read().unwrap();
        let mut out = text.to_string();
        for (name, blob) in entries.iter() {
            if let Ok(secret) = self.decrypt(name, blob) {
                if !secret.expose().is_empty() {
                    out = out.replace(secret.expose(), "[redacted]");
                }
            }
        }
        out
    }

    /// The vault as bytes safe to write to disk: names in clear, values encrypted.
    pub fn seal(&self) -> Vec<u8> {
        let entries = self.inner.entries.read().unwrap();
        serde_json::to_vec(&*entries).expect("a map of bytes always serializes")
    }

    /// Opens bytes from [`Vault::seal`] with `key`; a wrong key or altered bytes
    /// give [`VaultError::Decrypt`].
    pub fn unseal(key: [u8; 32], sealed: &[u8]) -> Result<Self, VaultError> {
        let entries: BTreeMap<String, Vec<u8>> =
            serde_json::from_slice(sealed).map_err(|_| VaultError::Malformed)?;
        let vault = Self::new(key);
        for (name, blob) in &entries {
            vault.decrypt(name, blob)?;
        }
        *vault.inner.entries.write().unwrap() = entries;
        Ok(vault)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Panics while holding the vault's write lock, so the lock is poisoned.
    fn poison(vault: &Vault) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = vault.inner.entries.write().unwrap();
            panic!("poison the vault lock for the test");
        }));
    }

    #[test]
    fn a_poisoned_vault_lock_recovers_instead_of_panicking() {
        let vault = Vault::new([7; 32]);
        vault.put("groq", "sk-test-123");
        poison(&vault);
        // Every vault operation must keep working after the poisoning.
        vault.put("other", "sk-test-456");
        assert_eq!(
            vault.names(),
            vec!["groq".to_string(), "other".to_string()]
        );
        assert_eq!(vault.resolve("groq").unwrap().expose(), "sk-test-123");
        assert_eq!(
            vault.redact("key sk-test-123 here"),
            "key [redacted] here"
        );
        assert!(vault.remove("other"));
        let sealed = vault.seal();
        assert!(!sealed.is_empty());
        let reopened = Vault::unseal([7; 32], &sealed).unwrap();
        assert_eq!(reopened.names(), vec!["groq".to_string()]);
    }
}

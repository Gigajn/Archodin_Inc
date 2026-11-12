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

use crate::locks::{read, write};

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
    /// Sealing the vault failed (serialisation); nothing was written, so no
    /// caller can mistake an empty byte string for a sealed vault.
    #[error("could not seal the vault")]
    Seal,
    /// The sealed envelope names a format version this build does not know.
    /// Checked before any decryption, so a future format is never read as
    /// if it were the current one.
    #[error("unsupported sealed vault version {0}")]
    UnsupportedVersion(u32),
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
    /// The envelope format version [`Vault::seal_envelope`] writes and
    /// [`Vault::unseal_envelope`] accepts. Bump when the envelope changes;
    /// older versions are then rejected, never misread.
    pub const ENVELOPE_VERSION: u32 = 1;

    /// An empty vault that encrypts with `key`. The key is copied into the
    /// cipher and the `Zeroizing` wrapper wipes its copy when this function
    /// returns, so no plain copy of the key survives the call.
    pub fn new(key: Zeroizing<[u8; 32]>) -> Self {
        let cipher = ChaCha20Poly1305::new_from_slice(&key[..])
            .expect("a 32-byte vault key always has the right length");
        Self {
            inner: Arc::new(Inner {
                cipher,
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
    ///
    /// Names are opaque ids, never key material: model keys live under
    /// `model/<kind>/<account>` (ADR 0021); the Agora login keeps its
    /// pre-existing `agora/*` names. No validation happens here on purpose
    /// — `put` returns nothing, and the existing Agora callers must keep
    /// working — so callers that mint new names must follow that pattern.
    pub fn put(&self, name: &str, value: &str) {
        let blob = self.encrypt(name, value);
        write(&self.inner.entries).insert(name.to_string(), blob);
    }

    /// Removes the secret called `name`; true if there was one.
    pub fn remove(&self, name: &str) -> bool {
        write(&self.inner.entries).remove(name).is_some()
    }

    /// The names of all secrets, sorted. Values are never listed.
    pub fn names(&self) -> Vec<String> {
        read(&self.inner.entries).keys().cloned().collect()
    }

    /// Decrypts the secret called `name`.
    pub fn resolve(&self, name: &str) -> Result<Secret, VaultError> {
        let entries = read(&self.inner.entries);
        let blob = entries
            .get(name)
            .ok_or_else(|| VaultError::UnknownSecret(name.to_string()))?;
        self.decrypt(name, blob)
    }

    /// Replaces every known secret value in `text` with `[redacted]`, for text that
    /// may reach events, logs, Briefings or the Akasha.
    pub fn redact(&self, text: &str) -> String {
        let entries = read(&self.inner.entries);
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
    /// Fails loudly instead of returning empty bytes: a failed seal must
    /// never be mistaken for an empty vault (ADR 0021).
    pub fn seal(&self) -> Result<Vec<u8>, VaultError> {
        let entries = read(&self.inner.entries);
        serde_json::to_vec(&*entries).map_err(|_| VaultError::Seal)
    }

    /// The vault as a versioned envelope safe to persist (ADR 0021):
    /// `{"version": 1, "sealed": <bytes from [`Vault::seal`]>}`. The
    /// `sealed_vault` table stores exactly these bytes.
    pub fn seal_envelope(&self) -> Result<Vec<u8>, VaultError> {
        let sealed = self.seal()?;
        serde_json::to_vec(&SealedEnvelope {
            version: Self::ENVELOPE_VERSION,
            sealed,
        })
        .map_err(|_| VaultError::Seal)
    }

    /// Opens bytes from [`Vault::seal_envelope`] with `key`. The envelope
    /// version is checked BEFORE any decryption, so an unknown version is
    /// rejected with [`VaultError::UnsupportedVersion`] even when the key
    /// is also wrong; a wrong key or altered bytes give
    /// [`VaultError::Decrypt`].
    pub fn unseal_envelope(key: Zeroizing<[u8; 32]>, bytes: &[u8]) -> Result<Self, VaultError> {
        let envelope: SealedEnvelope =
            serde_json::from_slice(bytes).map_err(|_| VaultError::Malformed)?;
        if envelope.version != Self::ENVELOPE_VERSION {
            return Err(VaultError::UnsupportedVersion(envelope.version));
        }
        Self::unseal(key, &envelope.sealed)
    }

    /// Opens bytes from [`Vault::seal`] with `key`; a wrong key or altered bytes
    /// give [`VaultError::Decrypt`]. Kept for the raw sealed form (tests and
    /// in-memory round trips); persisted vaults use the versioned envelope
    /// ([`Vault::seal_envelope`] / [`Vault::unseal_envelope`]) instead.
    pub fn unseal(key: Zeroizing<[u8; 32]>, sealed: &[u8]) -> Result<Self, VaultError> {
        let entries: BTreeMap<String, Vec<u8>> =
            serde_json::from_slice(sealed).map_err(|_| VaultError::Malformed)?;
        let vault = Self::new(key);
        for (name, blob) in &entries {
            vault.decrypt(name, blob)?;
        }
        *write(&vault.inner.entries) = entries;
        Ok(vault)
    }
}

/// The persisted form of a vault: a version tag around the raw sealed bytes
/// ([`Vault::seal`]). `sealed` serialises as a JSON array of byte values.
#[derive(serde::Serialize, serde::Deserialize)]
struct SealedEnvelope {
    version: u32,
    sealed: Vec<u8>,
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
        let vault = Vault::new(Zeroizing::new([7; 32]));
        vault.put("groq", "sk-test-123");
        poison(&vault);
        // Every vault operation must keep working after the poisoning.
        vault.put("other", "sk-test-456");
        assert_eq!(vault.names(), vec!["groq".to_string(), "other".to_string()]);
        assert_eq!(vault.resolve("groq").unwrap().expose(), "sk-test-123");
        assert_eq!(vault.redact("key sk-test-123 here"), "key [redacted] here");
        assert!(vault.remove("other"));
        let sealed = vault.seal().unwrap();
        assert!(!sealed.is_empty());
        let reopened = Vault::unseal(Zeroizing::new([7; 32]), &sealed).unwrap();
        assert_eq!(reopened.names(), vec!["groq".to_string()]);
    }

    #[test]
    fn seal_envelope_roundtrips_and_rejects_unknown_versions() {
        let vault = Vault::new(Zeroizing::new([7; 32]));
        vault.put("model/openai/main", "sk-canary-9f3a2b");
        let envelope = vault.seal_envelope().unwrap();
        let text = String::from_utf8_lossy(&envelope);
        assert!(text.contains("\"version\":1"));
        assert!(!text.contains("sk-canary-9f3a2b"));
        let reopened = Vault::unseal_envelope(Zeroizing::new([7; 32]), &envelope).unwrap();
        assert_eq!(
            reopened.resolve("model/openai/main").unwrap().expose(),
            "sk-canary-9f3a2b"
        );

        // An unknown version is rejected BEFORE any decryption: even with
        // the wrong key the error is about the version, not the decrypt.
        let mut future: serde_json::Value = serde_json::from_slice(&envelope).unwrap();
        future["version"] = serde_json::json!(2);
        let future = serde_json::to_vec(&future).unwrap();
        assert_eq!(
            Vault::unseal_envelope(Zeroizing::new([8; 32]), &future).unwrap_err(),
            VaultError::UnsupportedVersion(2)
        );
        assert_eq!(
            Vault::unseal_envelope(Zeroizing::new([7; 32]), b"not json").unwrap_err(),
            VaultError::Malformed
        );
    }
}

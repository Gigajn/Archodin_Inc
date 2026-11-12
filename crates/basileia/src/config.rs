//! Configuration for the Basileia server, read from the environment.
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `BASILEIA_BIND` | `127.0.0.1:8080` | Address (and port) the Agora listens on. |
//! | `BASILEIA_DB` | `basileia.db` | SQLite file for the SqlStore; `:memory:` selects an in-memory store (tests, throwaway runs). This crate only enables the `sqlite` feature of `archodin-store`, so any other value is treated as a SQLite file name — a `postgres://...` URL would NOT connect to Postgres, it would be created as a (useless) file of that name. |
//! | `BASILEIA_ADMIN_USER` | `admin` | Username for the Agora login. |
//! | `BASILEIA_ADMIN_PASSWORD` | *(required)* | Password for the Agora login. There is no default and no way to start without one: an open Agora is exactly KR-07, and this binary refuses to be that server. The plaintext password is only ever held in memory at startup; it is hashed with `hash_password` and never written to disk. |
//! | `BASILEIA_VAULT_KEY` | *(none)* | The Vault key: base64 of 32 random bytes. When set, the sealed vault (ADR 0021) can be reopened after a restart. Takes precedence over `BASILEIA_VAULT_KEY_FILE` when both are set. Never logged; held as `Zeroizing<[u8; 32]>` and wiped after `Vault::new`. |
//! | `BASILEIA_VAULT_KEY_FILE` | *(none)* | A file holding the base64 Vault key (one line). The file must be mode `0600` — a more permissive file is refused at startup. Only read when `BASILEIA_VAULT_KEY` is absent. |
//! | `BASILEIA_MODEL_<ACCOUNT>_KEY` | *(required per account)* | The API key of one model account (ADR 0020). `<ACCOUNT>` is a lowercase slug (`main`, `groq_cepat`). Required for every kind except `ollama`, which needs no key. |
//! | `BASILEIA_MODEL_<ACCOUNT>_KIND` | *(required per account)* | `openai_compatible`, `gemini`, `claude`, or `ollama`. |
//! | `BASILEIA_MODEL_<ACCOUNT>_BASE_URL` | *(default per kind)* | Optional; when set it must pass `http_guard::validate_endpoint` (checked through `ModelAccount::validate`). Empty means the per-kind default. |
//! | `BASILEIA_MODEL_<ACCOUNT>_MODEL` | *(optional)* | The account's default model. May be empty when the model is set per Role instead (ADR 0023) — such an account's key still lands in the Vault, but it is only registered in the Registry once a model is set.
//!
//! When no Vault key is configured at all, the server still starts: the
//! Vault key is random per process (the old behaviour) and any sealed
//! vault from an earlier run is reported `locked` instead of opened
//! (ADR 0021 §Decision 4). Env model keys still work in that mode — only
//! the accounts added through the Agora are locked.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use archodin_graph::{ModelAccount, ModelKind};
use base64::Engine as _;
use zeroize::Zeroizing;

/// Where the SqlStore keeps the company's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbConfig {
    /// A SQLite file, created when missing.
    File(PathBuf),
    /// A private in-memory SQLite database (tests, throwaway runs).
    Memory,
}

/// One model account as provisioned from the environment (ADR 0020 §Decision 2).
///
/// The plaintext `key` lives ONLY here, only during `assemble`: it is `put`
/// into the Vault and then dies with this `Config` — it is never copied to
/// `App` or any other structure. `Debug` is hand-written for the same
/// reason: the key must never appear in a debug log, a panic message, or
/// an error report.
#[derive(Clone)]
pub struct ModelAccountConfig {
    /// The account slug, for example `main` or `groq_cepat`.
    pub name: String,
    /// Which provider API this account speaks.
    pub kind: ModelKind,
    /// The API root (the per-kind default when the env var was empty).
    pub base_url: String,
    /// The default model; may be empty when the model is set per Role.
    pub default_model: String,
    /// The API key in plaintext, held only until `assemble` puts it in the
    /// Vault. Empty only for `ollama`, which needs no key.
    pub key: String,
}

impl std::fmt::Debug for ModelAccountConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelAccountConfig")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("default_model", &self.default_model)
            .field("key", &"[redacted]")
            .finish()
    }
}

impl ModelAccountConfig {
    /// The Vault secret name holding this account's key:
    /// `model/<kind>/<account>`.
    pub fn vault_secret(&self) -> String {
        ModelAccount::vault_secret_name(self.kind, &self.name)
    }
}

/// The default API root for a kind, used when
/// `BASILEIA_MODEL_<ACCOUNT>_BASE_URL` is empty (ADR 0020 §Decision 2:
/// "bila kosong dipakai default per kind"). Ollama's default is the one
/// ADR 0022 names (`127.0.0.1:11434`); the others are the vendors' public
/// API roots. No model name is defaulted anywhere: model names are
/// configuration, never a hardcoded catalogue (ADR 0022/0023).
pub fn default_base_url(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::OpenAiCompatible => "https://api.openai.com/v1",
        ModelKind::Gemini => "https://generativelanguage.googleapis.com/v1beta",
        ModelKind::Claude => "https://api.anthropic.com",
        ModelKind::Ollama => "http://127.0.0.1:11434",
    }
}

/// Everything `assemble` needs that is not code.
///
/// `Debug` is implemented by hand (not derived): the admin password, the
/// Vault key and every model key must never appear in a debug log, a panic
/// message, or an error report.
#[derive(Clone)]
pub struct Config {
    /// Address the Agora listens on.
    pub bind: SocketAddr,
    /// Where the SqlStore keeps the company's data.
    pub db: DbConfig,
    /// Username for the Agora login.
    pub admin_user: String,
    /// Password for the Agora login. Must not be empty.
    pub admin_password: String,
    /// The Vault key from `BASILEIA_VAULT_KEY` / `BASILEIA_VAULT_KEY_FILE`,
    /// or `None` when neither is set (random per-process key + degraded
    /// mode, ADR 0021 §Decision 4).
    pub vault_key: Option<Zeroizing<[u8; 32]>>,
    /// Model accounts provisioned from `BASILEIA_MODEL_*` env vars.
    pub model_accounts: Vec<ModelAccountConfig>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("bind", &self.bind)
            .field("db", &self.db)
            .field("admin_user", &self.admin_user)
            .field("admin_password", &"[redacted]")
            .field(
                "vault_key",
                &if self.vault_key.is_some() {
                    "[redacted]"
                } else {
                    "<none>"
                },
            )
            .field("model_accounts", &self.model_accounts)
            .finish()
    }
}

/// Why configuration failed.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// `BASILEIA_BIND` does not parse as a socket address.
    #[error("invalid bind address `{0}`: {1}")]
    BadBind(String, String),
    /// No admin password was configured.
    #[error("BASILEIA_ADMIN_PASSWORD is required: refusing to start without auth (see KR-07)")]
    MissingPassword,
    /// The admin username is empty.
    #[error("BASILEIA_ADMIN_USER must not be empty")]
    EmptyUser,
    /// Reading an environment variable failed.
    #[error("environment error: {0}")]
    Env(#[from] std::env::VarError),
    /// `BASILEIA_VAULT_KEY` (or the file it points at) is not base64 of
    /// exactly 32 bytes. The value itself is never carried in the message.
    #[error("BASILEIA_VAULT_KEY is not base64 of 32 bytes: {0}")]
    BadVaultKey(String),
    /// `BASILEIA_VAULT_KEY_FILE` could not be read, or its permissions are
    /// more permissive than 0600. The path is named; the contents never are.
    #[error("cannot use vault key file `{0}`: {1}")]
    VaultKeyFile(String, String),
    /// A `BASILEIA_MODEL_*` variable is missing, empty, or mis-shaped. The
    /// message names the VARIABLE, never its value.
    #[error("model account configuration: {0}")]
    BadModelAccount(String),
}

/// The prefix of every model-account variable (ADR 0020 §Decision 2).
const MODEL_PREFIX: &str = "BASILEIA_MODEL_";

/// Parses the Vault key from the environment map: `BASILEIA_VAULT_KEY`
/// first, else `BASILEIA_VAULT_KEY_FILE`. `None` when neither is set — the
/// caller then uses a random per-process key (degraded mode, ADR 0021).
/// Strict: invalid base64 or a length other than 32 bytes is an error,
/// and the value is never included in it.
pub fn parse_vault_key(
    vars: &HashMap<String, String>,
) -> Result<Option<Zeroizing<[u8; 32]>>, ConfigError> {
    if let Some(encoded) = vars.get("BASILEIA_VAULT_KEY") {
        let trimmed = encoded.trim();
        if trimmed.is_empty() {
            return Err(ConfigError::BadVaultKey(
                "the variable is set but empty".to_string(),
            ));
        }
        return decode_vault_key(trimmed).map(Some);
    }
    let Some(path) = vars.get("BASILEIA_VAULT_KEY_FILE") else {
        return Ok(None);
    };
    if path.trim().is_empty() {
        return Err(ConfigError::VaultKeyFile(
            path.clone(),
            "the variable is set but empty".to_string(),
        ));
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::VaultKeyFile(path.clone(), e.to_string()))?;
    check_key_file_permissions(path)?;
    decode_vault_key(contents.trim())
        .map(Some)
        .map_err(|e| match e {
            ConfigError::BadVaultKey(reason) => ConfigError::VaultKeyFile(path.clone(), reason),
            other => other,
        })
}

/// Decodes one base64 Vault key: exactly 32 bytes after decoding.
fn decode_vault_key(trimmed: &str) -> Result<Zeroizing<[u8; 32]>, ConfigError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .map_err(|_| ConfigError::BadVaultKey("not valid base64".to_string()))?;
    let array: [u8; 32] = bytes.try_into().map_err(|_| {
        ConfigError::BadVaultKey("decoded to a length other than 32 bytes".to_string())
    })?;
    Ok(Zeroizing::new(array))
}

/// Refuses a key file whose permissions are more permissive than `0600`:
/// any group/other bit set means the key is readable beyond its owner.
/// On non-Unix platforms there is no portable permission bit to check, so
/// the check is skipped there (and documented as such).
fn check_key_file_permissions(path: &str) -> Result<(), ConfigError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)
            .map_err(|e| ConfigError::VaultKeyFile(path.to_string(), e.to_string()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(ConfigError::VaultKeyFile(
                path.to_string(),
                format!(
                    "permissions are {mode:04o}, more permissive than 0600 — run `chmod 0600` on the file"
                ),
            ));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        // No portable permission check exists on this platform; the file is
        // used as-is. Operators on Windows should use an ACL instead.
    }
    Ok(())
}

/// One account's worth of `BASILEIA_MODEL_*` variables, as they arrive
/// before the required fields are checked.
#[derive(Default)]
struct PartialAccount {
    key: Option<String>,
    kind: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
}

/// The field suffixes of one account, longest first so `BASE_URL` is
/// matched before any shorter suffix could claim it.
const MODEL_FIELDS: [&str; 4] = ["_BASE_URL", "_KEY", "_KIND", "_MODEL"];

/// Groups `BASILEIA_MODEL_*` variables per account slug and validates
/// their shape only (ADR 0020 §Decision 5 — no network calls): a defined
/// account needs a non-empty `_KEY` (except `ollama`, which needs no key)
/// and a known `_KIND`; a non-empty `_BASE_URL` must pass
/// `ModelAccount::validate` (which runs `http_guard::validate_endpoint`
/// inside `archodin-graph`); `_MODEL` may be empty. Errors name the
/// VARIABLE, never its value. Accounts come back sorted by name.
pub fn parse_model_accounts(
    vars: &HashMap<String, String>,
) -> Result<Vec<ModelAccountConfig>, ConfigError> {
    let mut grouped: HashMap<String, PartialAccount> = HashMap::new();
    for (name, value) in vars {
        let Some(rest) = name.strip_prefix(MODEL_PREFIX) else {
            continue;
        };
        let Some((account, field)) = MODEL_FIELDS
            .iter()
            .find_map(|suffix| rest.strip_suffix(suffix).map(|a| (a, *suffix)))
        else {
            return Err(ConfigError::BadModelAccount(format!(
                "unknown variable `{name}`: expected BASILEIA_MODEL_<ACCOUNT>_(KEY|KIND|BASE_URL|MODEL)"
            )));
        };
        // Env var names are uppercase by convention (`BASILEIA_MODEL_MAIN_KEY`),
        // so the account part arrives uppercase and is lowercased into the
        // slug (`main`); mixed case (`Main`) is refused, not guessed.
        if account.is_empty()
            || !account
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(ConfigError::BadModelAccount(format!(
                "account in `{name}` is not an uppercase slug [A-Z0-9_]+ (lowercased to [a-z0-9_]+)"
            )));
        }
        let account = account.to_ascii_lowercase();
        let entry = grouped.entry(account).or_default();
        match field {
            "_KEY" => entry.key = Some(value.clone()),
            "_KIND" => entry.kind = Some(value.clone()),
            "_BASE_URL" => entry.base_url = Some(value.clone()),
            "_MODEL" => entry.model = Some(value.clone()),
            _ => unreachable!("the suffix came from MODEL_FIELDS"),
        }
    }

    let mut accounts = Vec::new();
    for (name, partial) in grouped {
        // Error messages name the VARIABLE as the operator wrote it
        // (uppercase); the account slug itself is lowercase.
        let var_account = name.to_ascii_uppercase();
        let kind_text = partial.kind.as_deref().unwrap_or("");
        let kind = ModelKind::from_str(kind_text).ok_or_else(|| {
            ConfigError::BadModelAccount(format!(
                "BASILEIA_MODEL_{var_account}_KIND is required and must be one of \
                 openai_compatible|gemini|claude|ollama"
            ))
        })?;
        let key = partial.key.clone().unwrap_or_default();
        if kind != ModelKind::Ollama && key.trim().is_empty() {
            return Err(ConfigError::BadModelAccount(format!(
                "BASILEIA_MODEL_{var_account}_KEY is required for a {kind} account and must not be empty"
            )));
        }
        let base_url = match partial.base_url.as_deref().unwrap_or("") {
            "" => default_base_url(kind).to_string(),
            url => url.to_string(),
        };
        // Shape-check the base URL through the Registry's own validation
        // (it runs `http_guard::validate_endpoint`): the probe account
        // carries a placeholder model so only the URL is under test here.
        // `http_guard` is a private module of `archodin-graph`, so this
        // crate cannot call `validate_endpoint` directly.
        let probe = ModelAccount::new(&name, kind, &base_url, "placeholder");
        if let Err(e) = probe.validate() {
            return Err(ConfigError::BadModelAccount(format!(
                "BASILEIA_MODEL_{var_account}_BASE_URL is not a usable endpoint: {e}"
            )));
        }
        accounts.push(ModelAccountConfig {
            name,
            kind,
            base_url,
            default_model: partial.model.clone().unwrap_or_default(),
            key,
        });
    }
    accounts.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(accounts)
}

impl Config {
    /// Reads the configuration from the environment (see the module docs).
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind = std::env::var("BASILEIA_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
        let bind: SocketAddr = bind
            .parse()
            .map_err(|e: std::net::AddrParseError| ConfigError::BadBind(bind, e.to_string()))?;

        let db = match std::env::var("BASILEIA_DB").as_deref() {
            Ok(":memory:") => DbConfig::Memory,
            Ok(path) => DbConfig::File(PathBuf::from(path)),
            Err(_) => DbConfig::File(PathBuf::from("basileia.db")),
        };

        let admin_user = std::env::var("BASILEIA_ADMIN_USER").unwrap_or_else(|_| "admin".into());
        if admin_user.trim().is_empty() {
            return Err(ConfigError::EmptyUser);
        }

        // A non-Unicode password is a different failure from a missing one:
        // report it as the environment error it is, not as "missing".
        let admin_password = match std::env::var("BASILEIA_ADMIN_PASSWORD") {
            Ok(password) => password,
            Err(std::env::VarError::NotPresent) => {
                return Err(ConfigError::MissingPassword);
            }
            Err(error) => return Err(ConfigError::Env(error)),
        };
        if admin_password.is_empty() {
            return Err(ConfigError::MissingPassword);
        }

        // The model/vault scan reads the whole environment at once (via
        // `vars_os`, so one non-Unicode value elsewhere cannot panic the
        // scan); entries that are not valid UTF-8 are skipped — keys are
        // base64/ASCII by construction, so nothing real is lost.
        let vars: HashMap<String, String> = std::env::vars_os()
            .filter_map(|(k, v)| {
                let k = k.into_string().ok()?;
                let v = v.into_string().ok()?;
                Some((k, v))
            })
            .collect();
        let vault_key = parse_vault_key(&vars)?;
        let model_accounts = parse_model_accounts(&vars)?;

        Ok(Self {
            bind,
            db,
            admin_user,
            admin_password,
            vault_key,
            model_accounts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sk-canary-*` placeholders only — never a real key, in tests or
    /// anywhere else (ADR 0021).
    const CANARY: &str = "sk-canary-9f3a2b";

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn key_b64() -> String {
        base64::engine::general_purpose::STANDARD.encode([7u8; 32])
    }

    #[test]
    fn vault_key_parses_from_env_and_rejects_bad_shapes() {
        let parsed = parse_vault_key(&vars(&[("BASILEIA_VAULT_KEY", &key_b64())]))
            .unwrap()
            .unwrap();
        assert_eq!(&*parsed, &[7u8; 32]);

        assert!(parse_vault_key(&vars(&[])).unwrap().is_none());

        let err = parse_vault_key(&vars(&[("BASILEIA_VAULT_KEY", "not-base64!!")])).unwrap_err();
        assert!(matches!(err, ConfigError::BadVaultKey(_)));
        assert!(!err.to_string().contains("not-base64!!"));

        let short = base64::engine::general_purpose::STANDARD.encode([7u8; 16]);
        let err = parse_vault_key(&vars(&[("BASILEIA_VAULT_KEY", &short)])).unwrap_err();
        assert!(matches!(err, ConfigError::BadVaultKey(_)));

        let err = parse_vault_key(&vars(&[("BASILEIA_VAULT_KEY", "")])).unwrap_err();
        assert!(matches!(err, ConfigError::BadVaultKey(_)));
    }

    #[test]
    fn vault_key_file_is_read_and_its_permissions_checked() {
        let dir =
            std::env::temp_dir().join(format!("basileia-vault-key-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("key.txt");
        std::fs::write(&path, format!("{}\n", key_b64())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            let parsed = parse_vault_key(&vars(&[(
                "BASILEIA_VAULT_KEY_FILE",
                path.to_str().unwrap(),
            )]))
            .unwrap()
            .unwrap();
            assert_eq!(&*parsed, &[7u8; 32]);

            // A world-readable key file is refused, and the error names the
            // file — never the key.
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            let err = parse_vault_key(&vars(&[(
                "BASILEIA_VAULT_KEY_FILE",
                path.to_str().unwrap(),
            )]))
            .unwrap_err();
            assert!(matches!(err, ConfigError::VaultKeyFile(..)));
            assert!(!err.to_string().contains(&key_b64()));
            assert!(err.to_string().contains("0600"));
        }
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();

        let err = parse_vault_key(&vars(&[(
            "BASILEIA_VAULT_KEY_FILE",
            "/nonexistent/basileia-key.txt",
        )]))
        .unwrap_err();
        assert!(matches!(err, ConfigError::VaultKeyFile(..)));
    }

    #[test]
    fn model_vars_group_per_account_with_defaults() {
        let accounts = parse_model_accounts(&vars(&[
            ("BASILEIA_MODEL_MAIN_KEY", CANARY),
            ("BASILEIA_MODEL_MAIN_KIND", "openai_compatible"),
            (
                "BASILEIA_MODEL_MAIN_BASE_URL",
                "https://groq.example.com/v1",
            ),
            ("BASILEIA_MODEL_MAIN_MODEL", "llama-3.3-70b"),
            ("BASILEIA_MODEL_GROQ_CEPAT_KEY", "sk-canary-2"),
            ("BASILEIA_MODEL_GROQ_CEPAT_KIND", "openai_compatible"),
        ]))
        .unwrap();
        assert_eq!(accounts.len(), 2);
        // Sorted by name.
        assert_eq!(accounts[0].name, "groq_cepat");
        assert_eq!(accounts[0].base_url, "https://api.openai.com/v1");
        assert_eq!(accounts[0].default_model, "");
        assert_eq!(accounts[0].key, "sk-canary-2");
        assert_eq!(accounts[1].name, "main");
        assert_eq!(accounts[1].base_url, "https://groq.example.com/v1");
        assert_eq!(accounts[1].default_model, "llama-3.3-70b");
        assert_eq!(accounts[1].key, CANARY);
    }

    #[test]
    fn a_missing_key_errors_and_names_the_variable() {
        let err =
            parse_model_accounts(&vars(&[("BASILEIA_MODEL_MAIN_KIND", "gemini")])).unwrap_err();
        assert!(matches!(err, ConfigError::BadModelAccount(_)));
        assert!(err.to_string().contains("BASILEIA_MODEL_MAIN_KEY"), "{err}");
        assert!(!err.to_string().contains(CANARY));
    }

    #[test]
    fn an_unknown_kind_errors_and_names_the_variable() {
        let err = parse_model_accounts(&vars(&[
            ("BASILEIA_MODEL_MAIN_KEY", CANARY),
            ("BASILEIA_MODEL_MAIN_KIND", "bedrock"),
        ]))
        .unwrap_err();
        assert!(matches!(err, ConfigError::BadModelAccount(_)));
        assert!(
            err.to_string().contains("BASILEIA_MODEL_MAIN_KIND"),
            "{err}"
        );
    }

    #[test]
    fn a_missing_kind_errors() {
        let err = parse_model_accounts(&vars(&[("BASILEIA_MODEL_MAIN_KEY", CANARY)])).unwrap_err();
        assert!(err.to_string().contains("BASILEIA_MODEL_MAIN_KIND"));
    }

    #[test]
    fn a_bad_base_url_errors_and_names_the_variable() {
        let err = parse_model_accounts(&vars(&[
            ("BASILEIA_MODEL_MAIN_KEY", CANARY),
            ("BASILEIA_MODEL_MAIN_KIND", "openai_compatible"),
            ("BASILEIA_MODEL_MAIN_BASE_URL", "ftp://example.com/v1"),
        ]))
        .unwrap_err();
        assert!(matches!(err, ConfigError::BadModelAccount(_)));
        assert!(
            err.to_string().contains("BASILEIA_MODEL_MAIN_BASE_URL"),
            "{err}"
        );
    }

    #[test]
    fn an_unknown_suffix_and_a_bad_slug_error() {
        let err = parse_model_accounts(&vars(&[("BASILEIA_MODEL_MAIN_FOO", "x")])).unwrap_err();
        assert!(err.to_string().contains("BASILEIA_MODEL_MAIN_FOO"));
        let err = parse_model_accounts(&vars(&[("BASILEIA_MODEL_Main_KEY", CANARY)])).unwrap_err();
        assert!(err.to_string().contains("BASILEIA_MODEL_Main_KEY"));
    }

    #[test]
    fn ollama_needs_no_key_and_gets_its_default_url() {
        let accounts = parse_model_accounts(&vars(&[
            ("BASILEIA_MODEL_LOCAL_KIND", "ollama"),
            ("BASILEIA_MODEL_LOCAL_MODEL", "llama3.2"),
        ]))
        .unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].base_url, "http://127.0.0.1:11434");
        assert_eq!(accounts[0].key, "");
    }

    #[test]
    fn no_key_ever_appears_in_config_debug() {
        let config = Config {
            bind: "127.0.0.1:8080".parse().unwrap(),
            db: DbConfig::Memory,
            admin_user: "admin".to_string(),
            admin_password: "s3cret".to_string(),
            vault_key: Some(Zeroizing::new([7u8; 32])),
            model_accounts: vec![ModelAccountConfig {
                name: "main".to_string(),
                kind: ModelKind::OpenAiCompatible,
                base_url: "https://api.openai.com/v1".to_string(),
                default_model: "m1".to_string(),
                key: CANARY.to_string(),
            }],
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains(CANARY), "{debug}");
        assert!(!debug.contains("s3cret"), "{debug}");
        assert!(debug.contains("[redacted]"), "{debug}");
        // The base64 of the vault key must not leak either.
        assert!(!debug.contains(&key_b64()), "{debug}");
    }
}

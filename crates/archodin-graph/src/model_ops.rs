//! Operasional kunci model (ADR 0024): provisioning, rotasi, revoke,
//! health check on-demand, audit lifecycle, dan Budget per akun.
//!
//! Modul ini murni logika — tanpa HTTP, tanpa store — sehingga semua
//! aturannya bisa diuji tanpa server: handler Agora di
//! `archodin-console/src/model_api.rs` hanya memanggil fungsi-fungsi di
//! sini lalu menjalankan efek persistensinya (tabel `model_accounts`,
//! `model_audit`, sealed blob) lewat trait store.
//!
//! Aturan bocor (ADR 0025 §1) berlaku di seluruh modul ini: tidak satu pun
//! struct yang memegang kunci men-derive `Debug`; setiap teks yang bisa
//! mengandung kunci dilewatkan [`Vault::redact`] sebelum menjadi hasil
//! audit, detail health check, atau pesan error.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::http_guard::{shared_client, validate_endpoint};
use crate::locks::mutex;
use crate::{
    Budget, BudgetError, BudgetScope, ModelAccount, ModelError, ModelKind, Usage, UsageMeter, Vault,
};

/// Alasan persist gagal di mode degraded: tidak ada kunci Vault yang
/// dikonfigurasi, jadi blob yang disegel kunci acak per proses tidak akan
/// bisa dibuka lagi setelah restart — blob itu TIDAK PERNAH disimpan
/// (ADR 0021 §Decision 4). Mutasi tetap berlaku in-memory; respons
/// membawa `persisted: false` + alasan ini, bukan error keras.
pub const NO_VAULT_KEY_REASON: &str =
    "no vault key configured — set BASILEIA_VAULT_KEY and restart to persist";

/// Aktor audit di v1: satu admin Agora (ADR 0024). Sesi login belum
/// membawa nama pengguna ke handler, jadi semua peristiwa dicatat atas
/// nama `"admin"` sampai identitas sesi tersedia.
pub const AUDIT_ACTOR: &str = "admin";

/// Batas peristiwa audit yang dikembalikan `GET /api/models/audit`.
pub const AUDIT_LIST_LIMIT: usize = 100;

/// Nama akun model harus slug `[a-z0-9_]+` (ADR 0020 §Decision 2): huruf
/// kecil, angka, garis bawah. Huruf besar ditolak, bukan dilowercase —
/// API bersifat eksplisit, tidak menebak.
pub fn validate_account_name(name: &str) -> Result<(), ModelError> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(ModelError::invalid(
            ModelKind::OpenAiCompatible,
            name,
            "",
            "an account name must be a slug [a-z0-9_]+",
        ));
    }
    Ok(())
}

/// API root default per kind, dipakai bila `base_url` kosong saat
/// provisioning (ADR 0020 §Decision 2). Sama dengan
/// `basileia::config::default_base_url`; tidak ada nama model yang
/// di-default di mana pun (ADR 0022/0023).
pub fn default_base_url(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::OpenAiCompatible => "https://api.openai.com/v1",
        ModelKind::Gemini => "https://generativelanguage.googleapis.com/v1beta",
        ModelKind::Claude => "https://api.anthropic.com",
        ModelKind::Ollama => "http://127.0.0.1:11434",
        ModelKind::OpenRouter => "https://openrouter.ai/api/v1",
    }
}

/// Satu permintaan tambah akun, seperti yang diterima `POST
/// /api/models/accounts`. `Debug` ditulis tangan: `key` tidak pernah
/// muncul di `Debug`, log, atau error — hanya `[redacted]`.
pub struct ProvisionInput {
    /// Slug akun, `[a-z0-9_]+`.
    pub name: String,
    /// Spelling kind: `openai_compatible`, `gemini`, `claude`, `ollama`,
    /// `openrouter`.
    pub kind: String,
    /// API root; kosong berarti default per kind.
    pub base_url: Option<String>,
    /// Model default akun; boleh kosong (model ditetapkan per Role).
    pub default_model: Option<String>,
    /// Kunci baru; wajib kecuali `ollama`, yang tidak boleh membawa kunci.
    pub key: Option<String>,
}

impl std::fmt::Debug for ProvisionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionInput")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("default_model", &self.default_model)
            .field("key", &self.key.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

/// Hasil validasi [`ProvisionInput`]: akun yang siap didaftarkan, plus
/// kunci yang siap di-`put` ke Vault (`None` hanya untuk `ollama`).
/// Kunci dikembalikan sebagai `String` di sini — pemanggil harus
/// langsung `put` ke Vault dan tidak menyimpannya di tempat lain.
pub struct ValidatedProvision {
    /// Akun (metadata non-rahasia) untuk Registry + tabel.
    pub account: ModelAccount,
    /// Kunci plaintext; `None` untuk `ollama`.
    pub key: Option<String>,
}

impl std::fmt::Debug for ValidatedProvision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidatedProvision")
            .field("account", &self.account)
            .field("key", &self.key.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

/// Validasi bentuk saja, tanpa jaringan (ADR 0020 §Decision 5): slug nama,
/// kind dikenal, kunci tidak kosong (kecuali `ollama`), `base_url` lolos
/// [`validate_endpoint`]. Pesan error menyebut nama akun/kind, TIDAK
/// PERNAH nilai kunci.
pub fn validate_provision(input: ProvisionInput) -> Result<ValidatedProvision, ModelError> {
    validate_account_name(&input.name)?;
    let kind = ModelKind::from_str(&input.kind).ok_or_else(|| {
        ModelError::invalid(
            ModelKind::OpenAiCompatible,
            &input.name,
            "",
            "kind is required and must be one of openai_compatible|gemini|claude|ollama|openrouter (OpenRouter accounts use kind openrouter; Muse has no kind of its own)",
        )
    })?;
    let key = input.key.unwrap_or_default();
    let key = match kind {
        ModelKind::Ollama => {
            if !key.is_empty() {
                return Err(ModelError::invalid(
                    kind,
                    &input.name,
                    "",
                    "an ollama account needs no key",
                ));
            }
            None
        }
        _ => {
            if key.trim().is_empty() {
                return Err(ModelError::invalid(
                    kind,
                    &input.name,
                    "",
                    "a key is required and must not be empty",
                ));
            }
            if key.chars().any(char::is_whitespace) {
                return Err(ModelError::invalid(
                    kind,
                    &input.name,
                    "",
                    "a key must not contain whitespace",
                ));
            }
            Some(key)
        }
    };
    let base_url = match input.base_url.as_deref().unwrap_or("") {
        "" => default_base_url(kind).to_string(),
        url => url.to_string(),
    };
    // Shape-check the URL the same way env provisioning does (a probe
    // account carries a placeholder model so only the URL is under test).
    if let Err(e) = validate_endpoint(&base_url) {
        return Err(ModelError::invalid(
            kind,
            &input.name,
            "",
            format!("base URL refused: {e}"),
        ));
    }
    let account = ModelAccount::new(
        &input.name,
        kind,
        &base_url,
        input.default_model.unwrap_or_default(),
    );
    account.validate()?;
    Ok(ValidatedProvision { account, key })
}

/// Validasi kunci rotasi: tidak kosong, tanpa whitespace — tanpa pernah
/// membawa nilai kunci ke dalam pesan error.
pub fn validate_rotation_key(kind: ModelKind, account: &str, key: &str) -> Result<(), ModelError> {
    if kind == ModelKind::Ollama {
        return Err(ModelError::invalid(
            kind,
            account,
            "",
            "an ollama account has no key to rotate",
        ));
    }
    if key.trim().is_empty() {
        return Err(ModelError::invalid(
            kind,
            account,
            "",
            "a key is required and must not be empty",
        ));
    }
    if key.chars().any(char::is_whitespace) {
        return Err(ModelError::invalid(
            kind,
            account,
            "",
            "a key must not contain whitespace",
        ));
    }
    Ok(())
}

/// Validasi Budget per akun: token tidak negatif, biaya finite dan tidak
/// negatif. `None` berarti "tidak dibatasi" di sisi itu.
pub fn validate_budget(tokens: Option<i64>, cost: Option<f64>) -> Result<(), String> {
    if let Some(t) = tokens {
        if t < 0 {
            return Err("budget_tokens must not be negative".to_string());
        }
    }
    if let Some(c) = cost {
        if !c.is_finite() || c < 0.0 {
            return Err("budget_cost must be finite and non-negative".to_string());
        }
    }
    Ok(())
}

/// Detik sejak Unix epoch, untuk kolom waktu audit. Jam sebelum epoch
/// men-stamp 0, bukan gagal.
pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64
}

/// Aksi peristiwa audit (ADR 0024): hanya lifecycle kunci + kegagalan
/// auth. Audit per-call DITOLAK di v1 — terlalu berisik dan tidak
/// menjawab "kunci mana yang bocor / kapan terakhir dirotasi".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditAction {
    /// Akun ditambahkan (env tidak diaudit — hanya Agora).
    Provision,
    /// Kunci diganti dengan nilai baru.
    Rotate,
    /// Kunci dicabut (hapus akun = revoke).
    Revoke,
    /// Health check on-demand dijalankan.
    HealthCheck,
    /// Provider menolak kunci (401/403) saat health check.
    AuthFailed,
    /// Pemanggilan model gagal karena auth (dicatat jalur pemanggilan,
    /// bukan di sini — disediakan agar audit punya satu kosakata).
    CallFailed,
}

impl AuditAction {
    /// Spelling yang disimpan di tabel `model_audit`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Provision => "provision",
            Self::Rotate => "rotate",
            Self::Revoke => "revoke",
            Self::HealthCheck => "health_check",
            Self::AuthFailed => "auth_failed",
            Self::CallFailed => "call_failed",
        }
    }
}

/// Satu peristiwa audit, siap disimpan — tanpa pernah membawa nilai kunci.
/// `result` dilewatkan [`Vault::redact`] di konstruktor, jadi pemanggil
/// tidak bisa lupa: teks apa pun yang mengandung kunci yang dikenal Vault
/// sampai ke sini sudah bersih.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// Waktu peristiwa (epoch detik).
    pub at: i64,
    /// Siapa yang melakukannya (`"admin"` di v1).
    pub actor: String,
    /// Aksi lifecycle.
    pub action: AuditAction,
    /// Kind provider (`openai_compatible`, ...).
    pub kind: String,
    /// Nama akun.
    pub account: String,
    /// Nama model default akun (boleh kosong).
    pub model: String,
    /// Hasil — sudah di-redact; contoh `ok`, `auth_failed`,
    /// `not_persisted: <alasan>`, `persist_failed: <alasan>`.
    pub result: String,
}

impl AuditEvent {
    /// Membangun peristiwa audit; `result` di-redact terhadap Vault
    /// sebelum disimpan, dan tidak ada field lain yang pernah membawa
    /// nilai kunci (hanya nama akun/model/kind).
    pub fn new(
        vault: &Vault,
        action: AuditAction,
        kind: ModelKind,
        account: &str,
        model: &str,
        result: &str,
    ) -> Self {
        Self {
            at: now_epoch(),
            actor: AUDIT_ACTOR.to_string(),
            action,
            kind: kind.as_str().to_string(),
            account: account.to_string(),
            model: model.to_string(),
            result: vault.redact(result),
        }
    }
}

/// Hasil health check: status, bukan janji (ADR 0024). Body respons
/// TIDAK PERNAH dibaca, apalagi disimpan — status + latensi + waktu
/// sudah cukup, dan body adalah tempat kunci paling sering bocor
/// (echo error provider).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Provider menjawab 2xx pada endpoint daftar model.
    Ok,
    /// Provider menjawab 401/403: kunci salah atau dicabut di vendor.
    AuthFailed,
    /// Provider menjawab 402: akun kehabisan kredit (khas OpenRouter).
    /// Bukan `unreachable` — jaringan baik-baik saja, yang habis adalah
    /// saldo; operator harus top up, bukan memeriksa jaringan.
    PaymentRequired,
    /// Tidak terjangkau: transport gagal, timeout, atau status lain.
    Unreachable,
    /// Kunci akun tidak ada di Vault (revoked / vault terkunci).
    KeyMissing,
}

impl HealthStatus {
    /// Spelling yang dikembalikan API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::AuthFailed => "auth_failed",
            Self::PaymentRequired => "payment_required",
            Self::Unreachable => "unreachable",
            Self::KeyMissing => "key_missing",
        }
    }
}

/// Hasil satu health check on-demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthOutcome {
    /// Status (`ok` / `auth_failed` / `payment_required` / `unreachable` / `key_missing`).
    pub status: HealthStatus,
    /// Latensi request, milidetik.
    pub latency_ms: u64,
    /// Waktu pengecekan (epoch detik).
    pub checked_at: i64,
    /// Detail aman: hanya status HTTP atau pesan transport yang sudah
    /// di-redact — TIDAK PERNAH body respons mentah.
    pub detail: String,
}

/// URL health check untuk satu akun (ADR 0024): endpoint daftar model
/// provider, tanpa inference. Bila `base_url` sudah berakhiran segmen
/// versi (`/v1`, `/v1beta`), segmen itu dilepas dulu agar tidak ganda:
/// `https://api.openai.com/v1` + `/v1/models` (bukan `/v1/v1/models`),
/// dan base mock `http://127.0.0.1:PORT` tetap mendapat `/v1/models`.
pub fn health_url(account: &ModelAccount) -> String {
    let base = account.base_url.trim_end_matches('/');
    let (base, suffix) = match account.kind {
        ModelKind::OpenAiCompatible => (base.strip_suffix("/v1").unwrap_or(base), "/v1/models"),
        // OpenRouter reuses the `openai_compatible` shape: its root ends
        // in `/v1`, so the same strip-then-append applies (ADR 0027).
        ModelKind::OpenRouter => (base.strip_suffix("/v1").unwrap_or(base), "/v1/models"),
        ModelKind::Gemini => (
            base.strip_suffix("/v1beta").unwrap_or(base),
            "/v1beta/models",
        ),
        ModelKind::Claude => (base.strip_suffix("/v1").unwrap_or(base), "/v1/models"),
        ModelKind::Ollama => (base, "/api/tags"),
    };
    format!("{base}{suffix}")
}

/// Health check on-demand untuk satu akun (ADR 0024): satu panggilan live
/// lewat [`shared_client`] (timeout 30 dtk, guard SSRF), tanpa cache,
/// tanpa memblokir startup, tanpa berjalan otomatis berkala.
///
/// Kunci diambil dari Vault di sini (bukan diteruskan sebagai `String`):
/// `ollama` tidak memakai kunci; akun lain yang kuncinya hilang memberi
/// [`HealthStatus::KeyMissing`] tanpa satu pun request keluar. Status HTTP
/// dipetakan 2xx → ok, 401/403 → auth_failed, 402 → payment_required,
/// sisanya → unreachable.
/// Body respons tidak pernah dibaca.
pub async fn health_check(
    vault: &Vault,
    account: &ModelAccount,
) -> Result<HealthOutcome, crate::http_guard::HttpGuardError> {
    let checked_at = now_epoch();
    let started = Instant::now();
    let key = match account.kind {
        ModelKind::Ollama => None,
        _ => match vault.resolve(&account.vault_secret) {
            Ok(secret) => Some(secret.expose().to_string()),
            Err(_) => {
                return Ok(HealthOutcome {
                    status: HealthStatus::KeyMissing,
                    latency_ms: started.elapsed().as_millis() as u64,
                    checked_at,
                    detail: "the key for this account is missing".to_string(),
                });
            }
        },
    };
    let client = shared_client()?;
    let mut request = client.get(health_url(account));
    match account.kind {
        // OpenRouter authenticates like any OpenAI-compatible API
        // (Bearer key) — and a health check sends NO attribution
        // headers: attribution belongs to chat completions only
        // (ADR 0027 §D4).
        ModelKind::OpenAiCompatible | ModelKind::OpenRouter => {
            if let Some(key) = &key {
                request = request.bearer_auth(key);
            }
        }
        ModelKind::Gemini => {
            if let Some(key) = &key {
                request = request.header("x-goog-api-key", key);
            }
        }
        ModelKind::Claude => {
            if let Some(key) = &key {
                request = request
                    .header("x-api-key", key)
                    .header("anthropic-version", "2023-06-01");
            }
        }
        ModelKind::Ollama => {}
    }
    // The key is dropped here, before any error text is built: no error
    // below can carry it, and the outcome's `detail` is redacted anyway.
    drop(key);
    let latency_ms = |started: Instant| started.elapsed().as_millis() as u64;
    match request.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let outcome_status = match status {
                200..=299 => HealthStatus::Ok,
                401 | 403 => HealthStatus::AuthFailed,
                402 => HealthStatus::PaymentRequired,
                _ => HealthStatus::Unreachable,
            };
            let detail = if outcome_status == HealthStatus::Ok {
                String::new()
            } else {
                vault.redact(&format!("provider answered HTTP {status}"))
            };
            Ok(HealthOutcome {
                status: outcome_status,
                latency_ms: latency_ms(started),
                checked_at,
                detail,
            })
        }
        Err(e) => Ok(HealthOutcome {
            status: HealthStatus::Unreachable,
            latency_ms: latency_ms(started),
            checked_at,
            detail: vault.redact(&format!("request failed: {e}")),
        }),
    }
}

/// Budget per akun model (ADR 0024 §Budget): SEAM tipis di atas
/// [`UsageMeter`] yang sudah ada — bukan sistem metering baru.
///
/// Satu akun = satu scope Agen meter bernama `model/<account>`, jadi
/// [`UsageMeter::agent_usage`] / `set_agent_budget` / `exceeded` yang
/// sudah ada langsung berlaku, termasuk alur persetujuan overrun yang
/// sudah ada. Wiring ke jalur pemanggilan model (merekam pemakaian nyata
/// tiap call) adalah follow-up di luar modul ini: pemanggil kelak cukup
/// memanggil [`ModelBudgets::record`] dengan [`Usage`] yang dilaporkan
/// provider.
///
/// Bila satu provider tidak melaporkan pemakaian, akun itu dinyatakan
/// `"unmeasured"` secara eksplisit ([`ModelBudgets::measurement`]) —
/// bukan diam-diam dianggap nol: nol yang tidak terukur dan nol yang
/// benar-benar nol tidak boleh terlihat sama.
pub struct ModelBudgets {
    meter: Arc<UsageMeter>,
    /// Akun yang sudah pernah melaporkan pemakaian (satu kali `record`).
    measured: Mutex<HashSet<String>>,
}

impl ModelBudgets {
    /// Membungkus `meter` yang dipakai bersama (biasanya meter Agora).
    pub fn new(meter: Arc<UsageMeter>) -> Self {
        Self {
            meter,
            measured: Mutex::new(HashSet::new()),
        }
    }

    /// Scope meter untuk satu akun: `model/<account>`.
    pub fn scope(account: &str) -> String {
        format!("model/{account}")
    }

    /// Menetapkan Budget akun: token dan/atau biaya. `None` di kedua sisi
    /// berarti tanpa batas (Budget default meter). Mengganti Budget
    /// membatalkan waiver overrun yang masih berlaku untuk scope itu
    /// (perilaku `UsageMeter::set_agent_budget`).
    pub fn set_budget(
        &self,
        account: &str,
        tokens: Option<u64>,
        cost: Option<f64>,
    ) -> Result<(), BudgetError> {
        let mut budget = Budget::default();
        if let Some(tokens) = tokens {
            budget = budget.with_tokens(tokens);
        }
        if let Some(cost) = cost {
            budget = budget.with_cost(cost)?;
        }
        self.meter.set_agent_budget(&Self::scope(account), budget);
        Ok(())
    }

    /// Pemakaian tercatat akun sejauh ini (token + biaya).
    pub fn usage(&self, account: &str) -> Usage {
        self.meter.agent_usage(&Self::scope(account))
    }

    /// Merekam pemakaian nyata satu call (`job` yang mengerjakannya):
    /// masuk ke meter di bawah scope akun, dan menandai akun itu
    /// `"measured"`. Pemakaian tidak valid (biaya NaN/negatif) diabaikan
    /// meter — lihat [`UsageMeter::record`].
    pub fn record(&self, account: &str, job: &str, usage: Usage) {
        self.meter.record(job, &Self::scope(account), usage);
        mutex(&self.measured).insert(account.to_string());
    }

    /// Budget pertama yang terlampaui untuk `job` di akun ini, bila ada
    /// (cek baca-saja: tidak mengonsumsi waiver).
    pub fn check(&self, account: &str, job: &str) -> Option<BudgetScope> {
        self.meter.exceeded(job, &Self::scope(account))
    }

    /// Apakah akun ini sudah pernah melaporkan pemakaian.
    pub fn measured(&self, account: &str) -> bool {
        mutex(&self.measured).contains(account)
    }

    /// `"measured"` bila sudah ada pemakaian tercatat, `"unmeasured"`
    /// bila belum — untuk ditampilkan apa adanya di Agora.
    pub fn measurement(&self, account: &str) -> &'static str {
        if self.measured(account) {
            "measured"
        } else {
            "unmeasured"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Registry;

    /// Runs an async block on a throwaway current-thread runtime
    /// (`tokio::test` is unavailable: the `macros` feature of tokio is not
    /// enabled for this crate).
    fn run_blocking<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(f)
    }

    /// `sk-canary-*` placeholders only — never a real key (ADR 0021).
    fn canary() -> String {
        format!(
            "sk-canary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock runs forward")
                .as_nanos()
        )
    }

    fn provision_input(name: &str, kind: &str, key: Option<&str>) -> ProvisionInput {
        ProvisionInput {
            name: name.to_string(),
            kind: kind.to_string(),
            base_url: None,
            default_model: Some("m1".to_string()),
            key: key.map(str::to_string),
        }
    }

    #[test]
    fn default_base_urls_match_the_spec_for_all_five_kinds() {
        // Test kesepakatan (ADR 0026 / C1, diamandemen ADR 0027):
        // `default_base_url` di sini diduplikasi di
        // `basileia::config::default_base_url` — dua test literal ini
        // bersama-sama mendeteksi drift antara keduanya. Nilai-nilai di
        // bawah adalah sumber kebenaran independen (dok vendor /
        // ADR 0020 §Decision 2), bukan salinan dari kode.
        assert_eq!(
            default_base_url(ModelKind::OpenAiCompatible),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            default_base_url(ModelKind::Gemini),
            "https://generativelanguage.googleapis.com/v1beta"
        );
        assert_eq!(
            default_base_url(ModelKind::Claude),
            "https://api.anthropic.com"
        );
        assert_eq!(
            default_base_url(ModelKind::Ollama),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            default_base_url(ModelKind::OpenRouter),
            "https://openrouter.ai/api/v1"
        );
    }

    #[test]
    fn account_names_must_be_slugs() {
        assert!(validate_account_name("main").is_ok());
        assert!(validate_account_name("groq_cepat").is_ok());
        assert!(validate_account_name("a1").is_ok());
        for bad in ["", "Main", "my-account", "a b", "a.b", "a/b", "UPPER"] {
            let err = validate_account_name(bad).unwrap_err();
            assert!(err.to_string().contains("slug"), "{bad}: {err}");
        }
    }

    #[test]
    fn openrouter_is_accepted_as_its_own_kind() {
        // ADR 0027: `openrouter` is a kind now (it changes behaviour —
        // attribution headers), so provisioning it succeeds and the
        // base URL defaults to the OpenRouter root, no BASE_URL needed.
        let validated =
            validate_provision(provision_input("main", "openrouter", Some("sk-x"))).unwrap();
        assert_eq!(validated.account.kind, ModelKind::OpenRouter);
        assert_eq!(validated.account.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(validated.account.vault_secret, "model/openrouter/main");
        assert_eq!(validated.key.as_deref(), Some("sk-x"));
    }

    #[test]
    fn muse_still_has_no_kind_and_unknown_kinds_name_all_five() {
        // Kind is an API shape, never a model name (ADR 0022/0026):
        // `muse` stays rejected — the error must now point at kind
        // `openrouter` (no longer at `openai_compatible` + BASE_URL)
        // and list all five kinds.
        for bad in ["muse", "bedrock"] {
            let err = validate_provision(provision_input("main", bad, Some("sk-x"))).unwrap_err();
            assert!(err.to_string().contains("openrouter"), "{bad}: {err}");
            assert!(
                err.to_string()
                    .contains("openai_compatible|gemini|claude|ollama|openrouter"),
                "{bad}: {err}"
            );
            assert!(
                !err.to_string()
                    .contains("(OpenRouter/Muse use openai_compatible with BASE_URL)"),
                "{bad}: {err}"
            );
        }
    }

    #[test]
    fn provision_validation_rejects_bad_shapes_without_network() {
        // Unknown kind.
        let err = validate_provision(provision_input("main", "bedrock", Some("sk-x"))).unwrap_err();
        assert!(err.to_string().contains("openai_compatible"), "{err}");
        // Empty key for a keyed kind.
        let err = validate_provision(provision_input("main", "claude", Some("  "))).unwrap_err();
        assert!(err.to_string().contains("must not be empty"), "{err}");
        let err = validate_provision(provision_input("main", "claude", None)).unwrap_err();
        assert!(err.to_string().contains("must not be empty"), "{err}");
        // A key with whitespace is refused (paste error), without echoing it.
        let err =
            validate_provision(provision_input("main", "claude", Some("sk-foo bar"))).unwrap_err();
        assert!(!err.to_string().contains("sk-foo"), "{err}");
        // Ollama must not carry a key; an empty one is fine.
        let err = validate_provision(provision_input("local", "ollama", Some("sk-x"))).unwrap_err();
        assert!(err.to_string().contains("no key"), "{err}");
        assert!(validate_provision(provision_input("local", "ollama", None)).is_ok());
        // A bad base URL is refused.
        let mut input = provision_input("main", "gemini", Some("sk-x"));
        input.base_url = Some("not a url".to_string());
        assert!(validate_provision(input).is_err());
    }

    #[test]
    fn provision_validation_applies_the_per_kind_default_url() {
        let validated =
            validate_provision(provision_input("main", "openai_compatible", Some("sk-x"))).unwrap();
        assert_eq!(validated.account.base_url, "https://api.openai.com/v1");
        assert_eq!(
            validated.account.vault_secret,
            "model/openai_compatible/main"
        );
        assert_eq!(validated.key.as_deref(), Some("sk-x"));
        let validated = validate_provision(provision_input("local", "ollama", None)).unwrap();
        assert_eq!(validated.account.base_url, "http://127.0.0.1:11434");
        assert!(validated.key.is_none());
    }

    #[test]
    fn provision_input_debug_never_shows_the_key() {
        let canary = canary();
        let input = provision_input("main", "claude", Some(&canary));
        let debug = format!("{input:?}");
        assert!(debug.contains("[redacted]"), "{debug}");
        assert!(!debug.contains(&canary), "{debug}");
    }

    #[test]
    fn rotation_key_validation_never_echoes_the_key() {
        assert!(validate_rotation_key(ModelKind::Claude, "work", "sk-new-123").is_ok());
        let err = validate_rotation_key(ModelKind::Claude, "work", "").unwrap_err();
        assert!(!err.to_string().contains("sk-"), "{err}");
        assert!(validate_rotation_key(ModelKind::Ollama, "local", "sk-x").is_err());
    }

    #[test]
    fn budget_validation_rejects_negative_and_non_finite() {
        assert!(validate_budget(Some(100), Some(1.5)).is_ok());
        assert!(validate_budget(None, None).is_ok());
        assert!(validate_budget(Some(-1), None).is_err());
        assert!(validate_budget(None, Some(f64::NAN)).is_err());
        assert!(validate_budget(None, Some(f64::INFINITY)).is_err());
        assert!(validate_budget(None, Some(-0.5)).is_err());
    }

    #[test]
    fn health_urls_never_double_the_version_segment() {
        let openai = ModelAccount::new(
            "main",
            ModelKind::OpenAiCompatible,
            "https://api.openai.com/v1",
            "m1",
        );
        assert_eq!(health_url(&openai), "https://api.openai.com/v1/models");
        let groq = ModelAccount::new(
            "groq",
            ModelKind::OpenAiCompatible,
            "https://api.groq.com/openai/v1/",
            "m1",
        );
        assert_eq!(health_url(&groq), "https://api.groq.com/openai/v1/models");
        let gemini = ModelAccount::new(
            "g",
            ModelKind::Gemini,
            "https://generativelanguage.googleapis.com/v1beta",
            "m1",
        );
        assert_eq!(
            health_url(&gemini),
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
        let claude = ModelAccount::new("c", ModelKind::Claude, "https://api.anthropic.com", "m1");
        assert_eq!(health_url(&claude), "https://api.anthropic.com/v1/models");
        let ollama = ModelAccount::new(
            "local",
            ModelKind::Ollama,
            "http://127.0.0.1:11434",
            "llama3.2",
        );
        assert_eq!(health_url(&ollama), "http://127.0.0.1:11434/api/tags");
        let openrouter = ModelAccount::new(
            "openrouter",
            ModelKind::OpenRouter,
            "https://openrouter.ai/api/v1",
            "meta/muse-spark-open",
        );
        assert_eq!(
            health_url(&openrouter),
            "https://openrouter.ai/api/v1/models"
        );
    }

    #[test]
    fn audit_events_redact_a_canary_in_the_result() {
        let canary = canary();
        let vault = Vault::new([7; 32].into());
        vault.put("model/claude/work", &canary);
        let event = AuditEvent::new(
            &vault,
            AuditAction::Provision,
            ModelKind::Claude,
            "work",
            "m1",
            &format!("persist_failed: upstream said {canary}"),
        );
        assert_eq!(event.action.as_str(), "provision");
        assert_eq!(event.actor, "admin");
        assert!(event.result.contains("[redacted]"), "{}", event.result);
        assert!(!event.result.contains(&canary), "{}", event.result);
        assert!(!format!("{event:?}").contains(&canary));
    }

    #[test]
    fn health_check_maps_200_401_and_a_dead_server() {
        let mut server = mockito::Server::new();
        let base = server.url();
        let vault = Vault::new([7; 32].into());
        let canary = canary();
        vault.put("model/openai_compatible/main", &canary);
        let account = ModelAccount::new("main", ModelKind::OpenAiCompatible, &base, "m1");

        // 200 → ok.
        let ok = server.mock("GET", "/v1/models").with_status(200).create();
        let outcome = run_blocking(health_check(&vault, &account));
        ok.assert();
        let outcome = outcome.expect("a 200 must map, not error");
        assert_eq!(outcome.status, HealthStatus::Ok);
        assert!(outcome.detail.is_empty());
        assert!(!format!("{outcome:?}").contains(&canary));

        // 401 → auth_failed, and the audit action for it exists.
        let denied = server
            .mock("GET", "/v1/models")
            .with_status(401)
            .with_body(format!("{{\"error\": \"bad key {canary}\"}}"))
            .create();
        let outcome = run_blocking(health_check(&vault, &account)).unwrap();
        denied.assert();
        assert_eq!(outcome.status, HealthStatus::AuthFailed);
        // The raw body is never read: the detail names only the status.
        assert!(!outcome.detail.contains(&canary), "{}", outcome.detail);
        assert!(!outcome.detail.contains("bad key"), "{}", outcome.detail);
        assert_eq!(outcome.detail, "provider answered HTTP 401");

        // A dead server → unreachable.
        let dead = ModelAccount::new(
            "dead",
            ModelKind::OpenAiCompatible,
            "http://127.0.0.1:9",
            "m1",
        );
        vault.put("model/openai_compatible/dead", &canary);
        let outcome = run_blocking(health_check(&vault, &dead)).unwrap();
        assert_eq!(outcome.status, HealthStatus::Unreachable);
        assert!(!outcome.detail.contains(&canary), "{}", outcome.detail);
    }

    #[test]
    fn health_check_maps_402_to_payment_required_not_to_unreachable() {
        // 402 means "no credits" (typical of OpenRouter): reporting it as
        // `unreachable` would send the operator chasing the network
        // instead of topping up the account.
        let mut server = mockito::Server::new();
        let base = server.url();
        let vault = Vault::new([7; 32].into());
        let canary = canary();
        vault.put("model/openai_compatible/main", &canary);
        let account = ModelAccount::new("main", ModelKind::OpenAiCompatible, &base, "m1");
        let mock = server.mock("GET", "/v1/models").with_status(402).create();
        let outcome = run_blocking(health_check(&vault, &account)).unwrap();
        mock.assert();
        assert_eq!(outcome.status, HealthStatus::PaymentRequired);
        assert_eq!(outcome.status.as_str(), "payment_required");
        assert_eq!(outcome.detail, "provider answered HTTP 402");
        assert!(!outcome.detail.contains(&canary), "{}", outcome.detail);
    }

    #[test]
    fn health_check_for_an_openrouter_account_uses_one_v1_segment_and_bearer() {
        // ADR 0027 (7e): an OpenRouter account is now kind `openrouter`
        // (model name in the TEST only — no model name is ever a code
        // default). The health URL must be
        // `https://openrouter.ai/api/v1/models`, never `/api/v1/v1/models`.
        let openrouter = ModelAccount::new(
            "openrouter",
            ModelKind::OpenRouter,
            "https://openrouter.ai/api/v1",
            "meta/muse-spark-open",
        );
        assert_eq!(
            health_url(&openrouter),
            "https://openrouter.ai/api/v1/models"
        );

        // The same base shape against a local mock: `/api/v1` + `/v1/models`
        // collapses to `/api/v1/models`, the Bearer key is sent, 200 → ok.
        // Attribution headers are NOT sent on a health check (ADR 0027 §D4):
        // the `Matcher::Missing` assertions below pin that down.
        let mut server = mockito::Server::new();
        let base = format!("{}/api/v1", server.url());
        let vault = Vault::new([7; 32].into());
        vault.put("model/openrouter/openrouter", "sk-canary-openrouter");
        let account = ModelAccount::new(
            "openrouter",
            ModelKind::OpenRouter,
            &base,
            "meta/muse-spark-open",
        );
        assert_eq!(
            health_url(&account),
            format!("{}/api/v1/models", server.url())
        );
        let mock = server
            .mock("GET", "/api/v1/models")
            .match_header("authorization", "Bearer sk-canary-openrouter")
            .match_header("http-referer", mockito::Matcher::Missing)
            .match_header("x-openrouter-title", mockito::Matcher::Missing)
            .with_status(200)
            .create();
        let outcome = run_blocking(health_check(&vault, &account)).unwrap();
        mock.assert();
        assert_eq!(outcome.status, HealthStatus::Ok);
    }

    #[test]
    fn health_check_without_a_key_reports_key_missing_without_a_request() {
        let vault = Vault::new([7; 32].into());
        let account =
            ModelAccount::new("work", ModelKind::Claude, "https://api.anthropic.com", "m1");
        let outcome = run_blocking(health_check(&vault, &account)).unwrap();
        assert_eq!(outcome.status, HealthStatus::KeyMissing);
    }

    #[test]
    fn health_check_sends_the_right_headers_per_kind() {
        let mut server = mockito::Server::new();
        let base = server.url();
        let vault = Vault::new([7; 32].into());
        vault.put("model/gemini/g", "sk-canary-gemini");
        let gemini = ModelAccount::new("g", ModelKind::Gemini, &base, "m1");
        let mock = server
            .mock("GET", "/v1beta/models")
            .match_header("x-goog-api-key", "sk-canary-gemini")
            .with_status(200)
            .create();
        let outcome = run_blocking(health_check(&vault, &gemini)).unwrap();
        mock.assert();
        assert_eq!(outcome.status, HealthStatus::Ok);

        vault.put("model/claude/c", "sk-canary-claude");
        let claude = ModelAccount::new("c", ModelKind::Claude, &base, "m1");
        let mock = server
            .mock("GET", "/v1/models")
            .match_header("x-api-key", "sk-canary-claude")
            .match_header("anthropic-version", "2023-06-01")
            .with_status(200)
            .create();
        let outcome = run_blocking(health_check(&vault, &claude)).unwrap();
        mock.assert();
        assert_eq!(outcome.status, HealthStatus::Ok);

        // Ollama sends no auth header at all.
        let ollama = ModelAccount::new("local", ModelKind::Ollama, &base, "llama3.2");
        let mock = server.mock("GET", "/api/tags").with_status(200).create();
        let outcome = run_blocking(health_check(&vault, &ollama)).unwrap();
        mock.assert();
        assert_eq!(outcome.status, HealthStatus::Ok);
    }

    #[test]
    fn model_budgets_wrap_the_existing_meter_and_report_unmeasured() {
        let meter = Arc::new(UsageMeter::new());
        let budgets = ModelBudgets::new(meter);
        assert_eq!(budgets.measurement("work"), "unmeasured");
        assert_eq!(budgets.check("work", "job-1"), None);

        budgets.set_budget("work", Some(100), None).unwrap();
        // 60 tokens recorded: within budget, and now measured.
        budgets.record("work", "job-1", Usage::new(40, 20, 0.0));
        assert_eq!(budgets.measurement("work"), "measured");
        assert_eq!(budgets.usage("work").total_tokens(), 60);
        assert_eq!(budgets.check("work", "job-1"), None);

        // 60 more tokens: over the 100-token budget.
        budgets.record("work", "job-1", Usage::new(60, 0, 0.0));
        assert_eq!(
            budgets.check("work", "job-1"),
            Some(BudgetScope::Agent("model/work".to_string()))
        );

        // A cost budget rejects a non-finite cap instead of storing it.
        assert!(budgets.set_budget("work", None, Some(f64::NAN)).is_err());
        assert!(budgets.set_budget("work", None, Some(2.5)).is_ok());
        // Clearing both limits removes the cap (the usage stays recorded).
        budgets.set_budget("work", None, None).unwrap();
        assert_eq!(budgets.check("work", "job-1"), None);
        assert_eq!(budgets.usage("work").total_tokens(), 120);
    }

    #[test]
    fn a_registry_account_can_be_removed_and_then_resolves_no_more() {
        let mut registry = Registry::new();
        registry
            .add_account(ModelAccount::new(
                "work",
                ModelKind::OpenAiCompatible,
                "https://api.example.com/v1",
                "m1",
            ))
            .unwrap();
        registry
            .set_role_chain("writer", vec![("work".into(), "m1".into())])
            .unwrap();
        let removed = registry.remove_account("work");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name, "work");
        assert!(registry.account("work").is_none());
        assert!(registry.remove_account("work").is_none());
        // A chain naming the removed account now fails closed (ADR 0023:
        // no silent fallback to another provider).
        assert!(registry.resolve("nova", "writer").is_err());
        assert_eq!(registry.accounts().len(), 0);
    }
}

//! The Agora model API (ADR 0020 §Decision 3, ADR 0024): add, rotate,
//! revoke, health-check and audit model accounts, all behind the Agora
//! admin login that already protects every other route — these handlers
//! are registered inside [`Agora::router`](crate::Agora::router), so
//! [`Auth::protect`](crate::Auth) covers them with no extra work, and
//! this module never touches the auth mechanism.
//!
//! Every handler takes its state from [`ModelApi`], which the application
//! attaches with [`AgoraBuilder::model_api`](crate::AgoraBuilder::model_api)
//! (Vault + Registry + store + meter + whether a Vault key is configured).
//! Without it the routes answer 503: the Agora keeps working, the model
//! API simply is not wired.
//!
//! Aturan bocor (ADR 0025 §1): tidak satu pun respons, pesan error, atau
//! baris audit di modul ini membawa nilai kunci. Request yang membawa
//! kunci (`CreateAccountRequest`, `RotateRequest`) punya `Debug` manual
//! yang menulis `[redacted]`; semua teks bebas yang masuk audit dilewatkan
//! [`Vault::redact`] oleh `AuditEvent` di `archodin-graph/src/model_ops.rs`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use archodin_graph::locks::mutex;
use archodin_graph::{
    health_check, validate_budget, validate_provision, validate_rotation_key, AuditAction,
    AuditEvent, HealthStatus, ModelAccount, ModelBudgets, ModelKind, ProvisionInput, Registry,
    UsageMeter, Vault, AUDIT_LIST_LIMIT, NO_VAULT_KEY_REASON,
};
use archodin_store::{ModelAccountRecord, ModelAuditRecord, ModelStore, StoreError};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::Agora;

type ApiError = (StatusCode, String);

/// The shared state of the model API: the Vault that holds the keys, the
/// Registry that names the accounts, the store that persists metadata +
/// sealed blob + audit, the meter the per-account Budgets wrap, and the
/// two startup facts the status endpoint reports.
///
/// `Debug` is hand-written: the Vault prints only secret names, the
/// Registry holds no keys, and the store is opaque — nothing here can
/// print a key value.
#[derive(Clone)]
pub struct ModelApi {
    vault: Vault,
    registry: Arc<Mutex<Registry>>,
    store: Arc<dyn ModelStore + Send + Sync>,
    budgets: Arc<ModelBudgets>,
    /// Whether `BASILEIA_VAULT_KEY` was configured at startup. When it
    /// was not, the Vault key is random per process and a sealed blob
    /// could never be reopened after a restart — so mutations are
    /// in-memory only and are reported `persisted: false` (degraded
    /// mode, ADR 0021 §Decision 4).
    vault_key_configured: bool,
    /// Startup fact (ADR 0021 §Decision 4): a sealed vault exists but no
    /// Vault key was configured, so its accounts could not be opened.
    vault_locked: bool,
}

impl std::fmt::Debug for ModelApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelApi")
            .field("vault", &self.vault)
            .field("registry", &self.registry.lock().map(|r| r.accounts()))
            .field("vault_key_configured", &self.vault_key_configured)
            .field("vault_locked", &self.vault_locked)
            .finish()
    }
}

impl ModelApi {
    /// Builds the model API state. The `registry` is shared (`Arc<Mutex>`)
    /// with the application, so accounts added here are visible to the
    /// application's own Registry reads and vice versa. `store` is any
    /// backend implementing all three model traits (`SqlStore` in
    /// production, `MemoryStore` in tests).
    pub fn new(
        vault: Vault,
        registry: Arc<Mutex<Registry>>,
        store: Arc<dyn ModelStore + Send + Sync>,
        meter: Arc<UsageMeter>,
        vault_key_configured: bool,
        vault_locked: bool,
    ) -> Self {
        Self {
            vault,
            registry,
            store,
            budgets: Arc::new(ModelBudgets::new(meter)),
            vault_key_configured,
            vault_locked,
        }
    }

    /// The Vault (keys are resolved per call, never copied out).
    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    /// The shared Registry.
    pub fn registry(&self) -> &Arc<Mutex<Registry>> {
        &self.registry
    }

    /// Whether a Vault key is configured (persist is possible).
    pub fn vault_key_configured(&self) -> bool {
        self.vault_key_configured
    }

    /// Whether the sealed vault is locked (startup fact).
    pub fn vault_locked(&self) -> bool {
        self.vault_locked
    }

    /// Accounts whose key is not in the Vault right now: sealed accounts
    /// in a locked vault, and table accounts whose key was never
    /// provisioned. Ollama accounts never count (they need no key).
    pub fn locked_accounts(&self) -> Vec<String> {
        let registry = mutex(&self.registry);
        let mut locked: Vec<String> = registry
            .accounts()
            .iter()
            .filter(|a| {
                a.kind != ModelKind::Ollama && !self.vault.names().contains(&a.vault_secret)
            })
            .map(|a| a.name.clone())
            .collect();
        locked.sort();
        locked.dedup();
        locked
    }

    /// Records one audit event; a failing audit write is logged to stderr
    /// but never fails the request it belongs to — the mutation already
    /// happened, and hiding it behind an audit error would be worse than
    /// a loud log line.
    async fn audit(&self, event: AuditEvent) {
        let record = ModelAuditRecord {
            seq: 0,
            at: event.at,
            actor: event.actor,
            action: event.action.as_str().to_string(),
            kind: event.kind,
            account: event.account,
            model: event.model,
            result: event.result,
        };
        if let Err(e) = self.store.record_audit(&record).await {
            eprintln!("[agora] model audit write failed: {e}");
        }
    }

    /// Re-seals the Vault and persists the envelope — the step that makes
    /// a mutation survive a restart (ADR 0021/0024).
    ///
    /// # What sealing seals — read this before changing it
    ///
    /// `seal_envelope` seals the WHOLE Vault, not just model keys: the
    /// `agora/*` login secrets and every env-provisioned key are in the
    /// same envelope. That is ACCEPTED in v1, for two reasons that must
    /// both stay true:
    /// - `assemble` always overwrites `agora/*` with a freshly hashed
    ///   password at startup, so a sealed login secret is never trusted
    ///   across a restart.
    /// - Env keys are re-provisioned on every restart, and an env key
    ///   that was removed from the environment becomes inert: no Registry
    ///   entry refers to it, so nothing ever resolves it again (the
    ///   ciphertext lingers in the blob until the next mutation re-seals,
    ///   which is dead weight, not a live key).
    ///
    /// Returns `(persisted, reason)`: `(true, None)` when the blob was
    /// written; `(false, Some(reason))` in degraded mode — where the blob
    /// is deliberately NOT written, because a blob sealed with a random
    /// per-process key could never be reopened after a restart and
    /// writing it would pretend otherwise. A seal/persist failure is an
    /// `Err`: the in-memory Vault already changed, and the caller must
    /// report that loudly (and audit `persist_failed`), never silently.
    async fn reseal_and_persist(&self) -> Result<(bool, Option<String>), String> {
        if !self.vault_key_configured {
            return Ok((false, Some(NO_VAULT_KEY_REASON.to_string())));
        }
        let envelope = self
            .vault
            .seal_envelope()
            .map_err(|e| format!("could not seal the vault: {e}"))?;
        self.store
            .save_sealed_vault(&envelope)
            .await
            .map_err(|e| format!("could not persist the sealed vault: {e}"))?;
        Ok((true, None))
    }
}

/// One account as the API shows it: metadata only, never a key.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct AccountView {
    name: String,
    kind: String,
    base_url: String,
    default_model: String,
    /// `ok` (key present, or ollama) or `locked` (key missing).
    /// There is no `revoked` row: deleting an account removes it, and
    /// the revoke itself is in the audit log.
    status: &'static str,
    budget_tokens: Option<i64>,
    budget_cost: Option<f64>,
}

fn account_view(
    account: &ModelAccount,
    budgets: &HashMap<String, ModelAccountRecord>,
    vault: &Vault,
) -> AccountView {
    let status =
        if account.kind == ModelKind::Ollama || vault.names().contains(&account.vault_secret) {
            "ok"
        } else {
            "locked"
        };
    let record = budgets.get(&account.name);
    AccountView {
        name: account.name.clone(),
        kind: account.kind.as_str().to_string(),
        base_url: account.base_url.clone(),
        default_model: account.default_model.clone(),
        status,
        budget_tokens: record.and_then(|r| r.budget_tokens),
        budget_cost: record.and_then(|r| r.budget_cost),
    }
}

async fn stored_records(api: &ModelApi) -> Result<HashMap<String, ModelAccountRecord>, StoreError> {
    Ok(api
        .store
        .load_model_accounts()
        .await?
        .into_iter()
        .map(|r| (r.name.clone(), r))
        .collect())
}

fn models(agora: &Agora) -> Result<ModelApi, ApiError> {
    agora.model_api().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "the model API is not configured".to_string(),
    ))
}

fn not_found(what: &str) -> ApiError {
    (StatusCode::NOT_FOUND, format!("no such {what}"))
}

fn bad_request(message: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, message.into())
}

/// `GET /api/models/accounts` — every account (metadata + `ok`/`locked`
/// status), plus `vault_locked` and `locked_accounts`. Never a key value.
pub(crate) async fn list_accounts(State(agora): State<Agora>) -> Result<Json<Value>, ApiError> {
    let api = models(&agora)?;
    let records = stored_records(&api).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not load model accounts: {e}"),
        )
    })?;
    let registry = mutex(&api.registry);
    let accounts: Vec<AccountView> = registry
        .accounts()
        .iter()
        .map(|a| account_view(a, &records, &api.vault))
        .collect();
    drop(registry);
    Ok(Json(json!({
        "vault_locked": api.vault_locked,
        "locked_accounts": api.locked_accounts(),
        "accounts": accounts,
    })))
}

/// `POST /api/models/accounts` — add an account. `{name, kind, base_url?,
/// model?, key?}`; the key may be empty only for `ollama`. Flow:
/// `Vault::put` → metadata to `model_accounts` → Registry → re-seal +
/// persist → audit `provision`. The key NEVER appears in the response.
#[derive(Deserialize)]
pub(crate) struct CreateAccountRequest {
    name: String,
    kind: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    key: Option<String>,
}

impl std::fmt::Debug for CreateAccountRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateAccountRequest")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("key", &self.key.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

fn record_of(account: &ModelAccount) -> ModelAccountRecord {
    ModelAccountRecord {
        name: account.name.clone(),
        kind: account.kind.as_str().to_string(),
        base_url: account.base_url.clone(),
        default_model: account.default_model.clone(),
        budget_tokens: None,
        budget_cost: None,
    }
}

pub(crate) async fn create_account(
    State(agora): State<Agora>,
    Json(request): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let api = models(&agora)?;
    let validated = validate_provision(ProvisionInput {
        name: request.name,
        kind: request.kind,
        base_url: request.base_url,
        default_model: request.model,
        key: request.key,
    })
    .map_err(|e| bad_request(e.to_string()))?;
    let account = validated.account;
    let secret = account.vault_secret.clone();

    // Vault first (in-memory); every later failure rolls this back.
    if let Some(key) = &validated.key {
        api.vault.put(&secret, key);
    }
    let record = record_of(&account);
    if let Err(e) = api.store.save_model_account(&record).await {
        if validated.key.is_some() {
            api.vault.remove(&secret);
        }
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not save model account `{}`: {e}", account.name),
        ));
    }
    // The guard is dropped at the end of this statement, BEFORE any
    // await below: holding a std MutexGuard across an await would make
    // this handler's future `!Send` (and block the executor thread).
    let registered = mutex(&api.registry).add_account(account.clone());
    if let Err(e) = registered {
        if validated.key.is_some() {
            api.vault.remove(&secret);
        }
        let _ = api.store.remove_model_account(&account.name).await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not register model account `{}`: {e}", account.name),
        ));
    }
    match api.reseal_and_persist().await {
        Ok((persisted, persist_reason)) => {
            let result = if persisted {
                "ok".to_string()
            } else {
                format!("not_persisted: {}", persist_reason.as_deref().unwrap_or(""))
            };
            api.audit(AuditEvent::new(
                &api.vault,
                AuditAction::Provision,
                account.kind,
                &account.name,
                &account.default_model,
                &result,
            ))
            .await;
            let records = stored_records(&api).await.unwrap_or_default();
            let mut body = json!({
                "account": account_view(&account, &records, &api.vault),
                "persisted": persisted,
            });
            if let Some(reason) = persist_reason {
                body["persist_reason"] = json!(reason);
            }
            Ok((StatusCode::CREATED, Json(body)))
        }
        Err(reason) => {
            // The in-memory Vault, the table and the Registry already
            // changed — report the failure loudly and audit it, never
            // silently pretend the account is persisted.
            api.audit(AuditEvent::new(
                &api.vault,
                AuditAction::Provision,
                account.kind,
                &account.name,
                &account.default_model,
                &format!("persist_failed: {reason}"),
            ))
            .await;
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "model account `{}` was provisioned in memory but could not be persisted: {reason}",
                    account.name
                ),
            ))
        }
    }
}

/// `DELETE /api/models/accounts/{name}` — delete an account = revoke its
/// key (ADR 0024): `Vault::remove` + metadata + `Registry::remove_account`
/// + re-seal → audit `revoke`. A later call to the account fails with
/// `ModelKeyMissing`. Revoking at the vendor's dashboard is still the
/// operator's job — the server only stops using the key.
pub(crate) async fn delete_account(
    State(agora): State<Agora>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let api = models(&agora)?;
    let account = mutex(&api.registry)
        .account(&name)
        .cloned()
        .ok_or_else(|| not_found("model account"))?;
    api.vault.remove(&account.vault_secret);
    if let Err(e) = api.store.remove_model_account(&name).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not remove model account `{name}`: {e}"),
        ));
    }
    mutex(&api.registry).remove_account(&name);
    match api.reseal_and_persist().await {
        Ok((persisted, persist_reason)) => {
            let result = if persisted {
                "ok".to_string()
            } else {
                format!("not_persisted: {}", persist_reason.as_deref().unwrap_or(""))
            };
            api.audit(AuditEvent::new(
                &api.vault,
                AuditAction::Revoke,
                account.kind,
                &name,
                &account.default_model,
                &result,
            ))
            .await;
            let mut body = json!({
                "ok": true,
                "revoked": name,
                "persisted": persisted,
            });
            if let Some(reason) = persist_reason {
                body["persist_reason"] = json!(reason);
            }
            Ok(Json(body))
        }
        Err(reason) => {
            api.audit(AuditEvent::new(
                &api.vault,
                AuditAction::Revoke,
                account.kind,
                &name,
                &account.default_model,
                &format!("persist_failed: {reason}"),
            ))
            .await;
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "model account `{name}` was revoked in memory but could not be persisted: {reason}"
                ),
            ))
        }
    }
}

/// `POST /api/models/accounts/{name}/rotate` — `{key}`: an atomic swap via
/// `Vault::put` under the same secret name (no dual-key, ADR 0024). A call
/// already running finishes on the old key; a new call uses the new one.
/// Then re-seal → audit `rotate`.
#[derive(Deserialize)]
pub(crate) struct RotateRequest {
    key: String,
}

impl std::fmt::Debug for RotateRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RotateRequest")
            .field("key", &"[redacted]")
            .finish()
    }
}

pub(crate) async fn rotate_account(
    State(agora): State<Agora>,
    Path(name): Path<String>,
    Json(request): Json<RotateRequest>,
) -> Result<Json<Value>, ApiError> {
    let api = models(&agora)?;
    let account = mutex(&api.registry)
        .account(&name)
        .cloned()
        .ok_or_else(|| not_found("model account"))?;
    validate_rotation_key(account.kind, &name, &request.key)
        .map_err(|e| bad_request(e.to_string()))?;
    api.vault.put(&account.vault_secret, &request.key);
    match api.reseal_and_persist().await {
        Ok((persisted, persist_reason)) => {
            let result = if persisted {
                "ok".to_string()
            } else {
                format!("not_persisted: {}", persist_reason.as_deref().unwrap_or(""))
            };
            api.audit(AuditEvent::new(
                &api.vault,
                AuditAction::Rotate,
                account.kind,
                &name,
                &account.default_model,
                &result,
            ))
            .await;
            let records = stored_records(&api).await.unwrap_or_default();
            let mut body = json!({
                "account": account_view(&account, &records, &api.vault),
                "persisted": persisted,
            });
            if let Some(reason) = persist_reason {
                body["persist_reason"] = json!(reason);
            }
            Ok(Json(body))
        }
        Err(reason) => {
            api.audit(AuditEvent::new(
                &api.vault,
                AuditAction::Rotate,
                account.kind,
                &name,
                &account.default_model,
                &format!("persist_failed: {reason}"),
            ))
            .await;
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "the key for model account `{name}` was rotated in memory but could not be persisted: {reason}"
                ),
            ))
        }
    }
}

/// `POST /api/models/accounts/{name}/health` — one live health check
/// (ADR 0024 table): `GET /v1/models` (or the per-kind equivalent) through
/// the guarded client. No body is ever read or returned; the outcome is a
/// status, a latency and a time. A missing key answers 409 with the typed
/// `ModelKeyMissing` message.
pub(crate) async fn health_account(
    State(agora): State<Agora>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let api = models(&agora)?;
    let account = mutex(&api.registry)
        .account(&name)
        .cloned()
        .ok_or_else(|| not_found("model account"))?;
    let outcome = health_check(&api.vault, &account).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("health check failed: {e}"),
        )
    })?;
    let (action, result) = match outcome.status {
        HealthStatus::Ok => (AuditAction::HealthCheck, "ok".to_string()),
        HealthStatus::AuthFailed => (AuditAction::AuthFailed, "auth_failed".to_string()),
        HealthStatus::Unreachable => (
            AuditAction::HealthCheck,
            format!("unreachable: {}", outcome.detail),
        ),
        HealthStatus::KeyMissing => (AuditAction::HealthCheck, "key_missing".to_string()),
    };
    api.audit(AuditEvent::new(
        &api.vault,
        action,
        account.kind,
        &name,
        &account.default_model,
        &result,
    ))
    .await;
    if outcome.status == HealthStatus::KeyMissing {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "the key for {} account `{}` (model `{}`) is missing",
                account.kind, name, account.default_model
            ),
        ));
    }
    Ok(Json(json!({
        "account": name,
        "kind": account.kind.as_str(),
        "model": account.default_model,
        "status": outcome.status.as_str(),
        "latency_ms": outcome.latency_ms,
        "checked_at": outcome.checked_at,
        "detail": outcome.detail,
    })))
}

/// `GET /api/models/audit` — the newest audit events first (at most
/// [`AUDIT_LIST_LIMIT`]). No key value ever appears in a row.
pub(crate) async fn list_audit(State(agora): State<Agora>) -> Result<Json<Value>, ApiError> {
    let api = models(&agora)?;
    let events = api.store.list_audit(AUDIT_LIST_LIMIT).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not load the audit log: {e}"),
        )
    })?;
    Ok(Json(json!({
        "events": events.iter().map(|e| json!({
            "seq": e.seq,
            "at": e.at,
            "actor": e.actor,
            "action": e.action,
            "kind": e.kind,
            "account": e.account,
            "model": e.model,
            "result": e.result,
        })).collect::<Vec<_>>(),
    })))
}

/// `GET /api/models/status` — `vault_locked` + `locked_accounts` (+ whether
/// a Vault key is configured), for the Agora UI.
pub(crate) async fn model_status(State(agora): State<Agora>) -> Result<Json<Value>, ApiError> {
    let api = models(&agora)?;
    Ok(Json(json!({
        "vault_locked": api.vault_locked,
        "vault_key_configured": api.vault_key_configured,
        "locked_accounts": api.locked_accounts(),
    })))
}

/// `GET /api/models/accounts/{name}/budget` — the account's caps, its
/// recorded usage, and whether that usage was ever measured (`"measured"`
/// vs `"unmeasured"`, ADR 0024: an unmeasured provider is never silently
/// reported as zero).
pub(crate) async fn get_budget(
    State(agora): State<Agora>,
    Path(name): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let api = models(&agora)?;
    let account = mutex(&api.registry)
        .account(&name)
        .cloned()
        .ok_or_else(|| not_found("model account"))?;
    let records = stored_records(&api).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not load model accounts: {e}"),
        )
    })?;
    let record = records.get(&name);
    let usage = api.budgets.usage(&name);
    Ok(Json(json!({
        "account": name,
        "kind": account.kind.as_str(),
        "budget_tokens": record.and_then(|r| r.budget_tokens),
        "budget_cost": record.and_then(|r| r.budget_cost),
        "used_tokens": usage.total_tokens(),
        "used_cost": usage.cost,
        "measured": api.budgets.measurement(&name),
        "exceeded": api.budgets.check(&name, "").is_some(),
    })))
}

/// `PUT /api/models/accounts/{name}/budget` — set the account's caps
/// (`budget_tokens` and/or `budget_cost`; `null` clears that side).
/// Persisted to `model_accounts` and applied to the shared meter via the
/// `ModelBudgets` seam, so a future model-call path can enforce it with
/// `ModelBudgets::check` / `record`.
#[derive(Debug, Deserialize)]
pub(crate) struct SetBudgetRequest {
    #[serde(default)]
    budget_tokens: Option<i64>,
    #[serde(default)]
    budget_cost: Option<f64>,
}

pub(crate) async fn set_budget(
    State(agora): State<Agora>,
    Path(name): Path<String>,
    Json(request): Json<SetBudgetRequest>,
) -> Result<Json<Value>, ApiError> {
    let api = models(&agora)?;
    let account = mutex(&api.registry)
        .account(&name)
        .cloned()
        .ok_or_else(|| not_found("model account"))?;
    validate_budget(request.budget_tokens, request.budget_cost).map_err(|e| bad_request(e))?;
    let mut record = stored_records(&api)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not load model accounts: {e}"),
            )
        })?
        .remove(&name)
        .unwrap_or_else(|| record_of(&account));
    record.budget_tokens = request.budget_tokens;
    record.budget_cost = request.budget_cost;
    api.store.save_model_account(&record).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not save the budget for `{name}`: {e}"),
        )
    })?;
    api.budgets
        .set_budget(
            &name,
            request.budget_tokens.map(|t| t as u64),
            request.budget_cost,
        )
        .map_err(|e| bad_request(e.to_string()))?;
    let usage = api.budgets.usage(&name);
    Ok(Json(json!({
        "account": name,
        "kind": account.kind.as_str(),
        "budget_tokens": request.budget_tokens,
        "budget_cost": request.budget_cost,
        "used_tokens": usage.total_tokens(),
        "used_cost": usage.cost,
        "measured": api.budgets.measurement(&name),
        "exceeded": api.budgets.check(&name, "").is_some(),
    })))
}

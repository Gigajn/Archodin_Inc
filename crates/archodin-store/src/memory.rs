//! The in-memory backend, behind the same traits as the databases.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use archodin_graph::locks::mutex;
use archodin_graph::{fact_key, Checkpoint, CheckpointStore, SkillSnapshot};
use async_trait::async_trait;
use futures::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{tidy_fact, AgentRecord, AkashaStore, RoleRecord, RosterStore, SkillStore, StoreError};

#[derive(Default)]
struct Inner {
    /// Checkpoints per Job, as JSON so one store holds any State type.
    checkpoints: HashMap<String, Vec<(usize, String, Option<String>)>>,
    roles: BTreeMap<String, RoleRecord>,
    agents: Vec<AgentRecord>,
    skills: SkillSnapshot,
    facts: BTreeMap<String, Vec<String>>,
}

/// Everything a company keeps, in memory. Cloning gives another handle to the
/// same storage.
#[derive(Clone, Default)]
pub struct MemoryStore {
    inner: Arc<Mutex<Inner>>,
}

fn invalid(e: impl std::fmt::Display) -> StoreError {
    StoreError::Invalid(e.to_string())
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S> CheckpointStore<S> for MemoryStore
where
    S: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn save(&self, checkpoint: Checkpoint<S>) -> BoxFuture<'_, Result<(), StoreError>> {
        Box::pin(async move {
            let state = serde_json::to_string(&checkpoint.state).map_err(invalid)?;
            mutex(&self.inner)
                .checkpoints
                .entry(checkpoint.job)
                .or_default()
                .push((checkpoint.step, state, checkpoint.next));
            Ok(())
        })
    }

    fn checkpoints(&self, job: &str) -> BoxFuture<'_, Result<Vec<Checkpoint<S>>, StoreError>> {
        let job = job.to_string();
        let rows = mutex(&self.inner)
            .checkpoints
            .get(&job)
            .cloned()
            .unwrap_or_default();
        Box::pin(async move {
            rows.into_iter()
                .map(|(step, state, next)| {
                    let state: S = serde_json::from_str(&state).map_err(invalid)?;
                    Ok(Checkpoint {
                        job: job.clone(),
                        step,
                        state,
                        next,
                    })
                })
                .collect::<Result<Vec<Checkpoint<S>>, StoreError>>()
        })
    }
}

#[async_trait]
impl RosterStore for MemoryStore {
    async fn save_role(&self, role: &RoleRecord) -> Result<(), StoreError> {
        mutex(&self.inner)
            .roles
            .insert(role.name.clone(), role.clone());
        Ok(())
    }

    async fn roles(&self) -> Result<Vec<RoleRecord>, StoreError> {
        Ok(mutex(&self.inner).roles.values().cloned().collect())
    }

    async fn save_agent(&self, agent: &AgentRecord) -> Result<(), StoreError> {
        let mut inner = mutex(&self.inner);
        match inner.agents.iter_mut().find(|a| a.name == agent.name) {
            Some(existing) => *existing = agent.clone(),
            None => inner.agents.push(agent.clone()),
        }
        Ok(())
    }

    async fn remove_agent(&self, name: &str) -> Result<bool, StoreError> {
        let mut inner = mutex(&self.inner);
        let before = inner.agents.len();
        inner.agents.retain(|a| a.name != name);
        Ok(inner.agents.len() != before)
    }

    async fn agents(&self) -> Result<Vec<AgentRecord>, StoreError> {
        Ok(mutex(&self.inner).agents.clone())
    }
}

#[async_trait]
impl SkillStore for MemoryStore {
    async fn save_skills(&self, snapshot: &SkillSnapshot) -> Result<(), StoreError> {
        mutex(&self.inner).skills = snapshot.clone();
        Ok(())
    }

    async fn load_skills(&self) -> Result<SkillSnapshot, StoreError> {
        Ok(mutex(&self.inner).skills.clone())
    }
}

#[async_trait]
impl AkashaStore for MemoryStore {
    async fn remember(&self, role: &str, fact: &str) -> Result<bool, StoreError> {
        let fact = tidy_fact(fact);
        let key = fact_key(&fact);
        if key.is_empty() {
            return Ok(false);
        }
        let mut inner = mutex(&self.inner);
        let list = inner.facts.entry(role.to_string()).or_default();
        if list.iter().any(|f| fact_key(f) == key) {
            return Ok(false);
        }
        list.push(fact);
        Ok(true)
    }

    async fn facts(&self, role: &str) -> Result<Vec<String>, StoreError> {
        Ok(mutex(&self.inner)
            .facts
            .get(role)
            .cloned()
            .unwrap_or_default())
    }

    async fn fact_roles(&self) -> Result<Vec<String>, StoreError> {
        Ok(mutex(&self.inner).facts.keys().cloned().collect())
    }
}


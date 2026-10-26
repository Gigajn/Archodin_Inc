//! Ronin finds Skills before work is assigned (ADR 0004).
//!
//! Before a Strategos assigns a Job, Ronin works out which Skills the task needs,
//! reports which ones the Role already has and which are missing, and searches for
//! the missing ones for free: a self-hosted SearXNG when configured, langchain-rust's
//! DuckDuckGo tool otherwise, and the GitHub API (an optional token raises the rate
//! limits and enables code search). Every Skill found goes to Vigil, which holds the
//! ones that pass in Codex's queue for a person to approve.

use futures::future::BoxFuture;
use langchain_rust::tools::DuckDuckGoSearchResults;

use crate::{
    AgentError, AgentNode, Assignment, Skill, SkillLibrary, SkillSource, Strategos, StrategosError,
    Verdict, Vigil,
};

/// How many hits of one search engine Ronin fetches and inspects per missing Skill.
const HITS_PER_ENGINE: usize = 5;

/// Why a search, a download or a Skill check failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoninError {
    /// A search engine could not be reached or answered badly.
    #[error("search failed: {0}")]
    Search(String),
    /// The text behind a hit could not be downloaded.
    #[error("could not fetch {0}")]
    Fetch(String),
    /// The Agent working out which Skills a task needs failed.
    #[error("could not tell which Skills the task needs: {0}")]
    Analyst(AgentError),
    /// Vigil's review could not be completed.
    #[error("Vigil could not inspect a Skill: {0}")]
    Vigil(AgentError),
    /// The Strategos could not handle the Job.
    #[error(transparent)]
    Strategos(#[from] StrategosError),
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// The result's title.
    pub title: String,
    /// Where the candidate `SKILL.md` text can be fetched (or the page to look at).
    pub url: String,
    /// A short excerpt.
    pub snippet: String,
}

/// Somewhere Ronin can search for Skills. Tests use scripted fakes.
pub trait SearchEngine: Send + Sync {
    /// A short name for the engine, such as `searxng`.
    fn name(&self) -> &str;

    /// Hits for `query`.
    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<Vec<SearchHit>, RoninError>>;

    /// The text behind `hit`; by default a plain GET of its URL.
    fn fetch<'a>(&'a self, hit: &'a SearchHit) -> BoxFuture<'a, Result<String, RoninError>> {
        Box::pin(async move { get_text(&hit.url, None, None).await })
    }
}

impl SearchEngine for Box<dyn SearchEngine> {
    fn name(&self) -> &str {
        (**self).name()
    }
    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<Vec<SearchHit>, RoninError>> {
        (**self).search(query)
    }
    fn fetch<'a>(&'a self, hit: &'a SearchHit) -> BoxFuture<'a, Result<String, RoninError>> {
        (**self).fetch(hit)
    }
}

/// The web search Ronin uses: the SearXNG at `searxng_url` when one is configured,
/// DuckDuckGo otherwise.
pub fn web_search(searxng_url: Option<&str>) -> Box<dyn SearchEngine> {
    match searxng_url {
        Some(url) => Box::new(SearxngSearch::new(url)),
        None => Box::new(DuckDuckGoSearch::new()),
    }
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

async fn get_text(
    url: &str,
    accept: Option<&str>,
    token: Option<&str>,
) -> Result<String, RoninError> {
    let mut request = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "archodin-ronin");
    if let Some(accept) = accept {
        request = request.header("Accept", accept);
    }
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| RoninError::Search(e.to_string()))?;
    if !response.status().is_success() {
        return Err(RoninError::Fetch(format!("{url} ({})", response.status())));
    }
    response
        .text()
        .await
        .map_err(|e| RoninError::Fetch(format!("{url} ({e})")))
}

fn str_field(value: &serde_json::Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

/// A self-hosted SearXNG instance, asked for JSON results.
pub struct SearxngSearch {
    base_url: String,
}

impl SearxngSearch {
    /// Searches the SearXNG at `base_url`.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }
}

impl SearchEngine for SearxngSearch {
    fn name(&self) -> &str {
        "searxng"
    }

    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<Vec<SearchHit>, RoninError>> {
        Box::pin(async move {
            let url = format!("{}/search?q={}&format=json", self.base_url, encode(query));
            let body = get_text(&url, Some("application/json"), None).await?;
            let json: serde_json::Value =
                serde_json::from_str(&body).map_err(|e| RoninError::Search(e.to_string()))?;
            Ok(json["results"]
                .as_array()
                .map(|results| {
                    results
                        .iter()
                        .map(|r| SearchHit {
                            title: str_field(r, "title"),
                            url: str_field(r, "url"),
                            snippet: str_field(r, "content"),
                        })
                        .collect()
                })
                .unwrap_or_default())
        })
    }
}

/// langchain-rust's DuckDuckGo tool, used when no SearXNG is configured.
pub struct DuckDuckGoSearch {
    max_results: usize,
}

impl DuckDuckGoSearch {
    /// A DuckDuckGo search returning a handful of results.
    pub fn new() -> Self {
        Self {
            max_results: HITS_PER_ENGINE,
        }
    }
}

impl Default for DuckDuckGoSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchEngine for DuckDuckGoSearch {
    fn name(&self) -> &str {
        "duckduckgo"
    }

    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<Vec<SearchHit>, RoninError>> {
        Box::pin(async move {
            let max = self.max_results;
            let query = query.to_string();
            // The upstream tool's future is not `Send`, so it runs on a blocking thread.
            let handle = tokio::runtime::Handle::current();
            let answer = tokio::task::spawn_blocking(move || {
                handle.block_on(async {
                    DuckDuckGoSearchResults::new()
                        .with_max_results(max)
                        .search(&query)
                        .await
                        .map_err(|e| e.to_string())
                })
            })
            .await
            .map_err(|e| RoninError::Search(e.to_string()))?
            .map_err(RoninError::Search)?;
            let json: serde_json::Value =
                serde_json::from_str(&answer).map_err(|e| RoninError::Search(e.to_string()))?;
            Ok(json
                .as_array()
                .map(|results| {
                    results
                        .iter()
                        .map(|r| SearchHit {
                            title: str_field(r, "title"),
                            url: str_field(r, "link"),
                            snippet: str_field(r, "snippet"),
                        })
                        .collect()
                })
                .unwrap_or_default())
        })
    }
}

/// The GitHub API: repository search works without a login; a token raises the
/// rate limits and also enables code search for `SKILL.md` files.
pub struct GitHubSearch {
    api_url: String,
    token: Option<String>,
}

impl GitHubSearch {
    /// Searches the GitHub API at `api_url` (normally `https://api.github.com`).
    pub fn new(api_url: impl Into<String>) -> Self {
        Self {
            api_url: api_url.into().trim_end_matches('/').to_string(),
            token: None,
        }
    }

    /// Uses `token` for every request.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    async fn items(&self, path: &str, query: &str) -> Result<Vec<serde_json::Value>, RoninError> {
        let url = format!(
            "{}{path}?q={}&per_page={HITS_PER_ENGINE}",
            self.api_url,
            encode(query)
        );
        let body = get_text(
            &url,
            Some("application/vnd.github+json"),
            self.token.as_deref(),
        )
        .await?;
        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| RoninError::Search(e.to_string()))?;
        Ok(json["items"].as_array().cloned().unwrap_or_default())
    }
}

impl SearchEngine for GitHubSearch {
    fn name(&self) -> &str {
        "github"
    }

    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<Vec<SearchHit>, RoninError>> {
        Box::pin(async move {
            let mut hits = Vec::new();
            let mut failure = None;
            let mut succeeded = false;
            match self.items("/search/repositories", query).await {
                Ok(items) => {
                    succeeded = true;
                    hits.extend(items.iter().map(|r| {
                        let name = str_field(r, "full_name");
                        SearchHit {
                            url: format!("{}/repos/{name}/contents/SKILL.md", self.api_url),
                            title: name,
                            snippet: str_field(r, "description"),
                        }
                    }));
                }
                Err(e) => failure = Some(e),
            }
            if self.token.is_some() {
                match self
                    .items("/search/code", &format!("{query} filename:SKILL.md"))
                    .await
                {
                    Ok(items) => {
                        succeeded = true;
                        hits.extend(items.iter().map(|c| SearchHit {
                            title: str_field(c, "html_url"),
                            url: str_field(c, "url"),
                            snippet: str_field(c, "name"),
                        }));
                    }
                    Err(e) => failure = Some(e),
                }
            }
            match (succeeded, failure) {
                (false, Some(e)) => Err(e),
                _ => Ok(hits),
            }
        })
    }

    fn fetch<'a>(&'a self, hit: &'a SearchHit) -> BoxFuture<'a, Result<String, RoninError>> {
        Box::pin(async move {
            get_text(
                &hit.url,
                Some("application/vnd.github.raw+json"),
                self.token.as_deref(),
            )
            .await
        })
    }
}

/// A Skill Ronin found for a need, and what Vigil said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundSkill {
    /// The need it was searched for.
    pub need: String,
    /// The name of the Skill found.
    pub skill: String,
    /// Vigil's verdict; a passing Skill now waits in Codex's queue.
    pub verdict: Verdict,
}

/// What Ronin learned about a task's Skills.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scouting {
    /// Needs the Role already has an approved Skill for.
    pub present: Vec<String>,
    /// Needs a search found a Skill for, which Vigil passed to Codex.
    pub found: Vec<FoundSkill>,
    /// Needs with no Skill, even after searching.
    pub missing: Vec<String>,
}

/// The Agent that scouts for Skills.
pub struct Ronin {
    analyst: AgentNode,
    vigil: Vigil,
    library: SkillLibrary,
    engines: Vec<Box<dyn SearchEngine>>,
}

impl Ronin {
    /// A Ronin whose `analyst` works out the Skills a task needs, handing what it
    /// finds to `vigil` and, when that passes, to the `library` (Codex's queue).
    pub fn new(analyst: AgentNode, vigil: Vigil, library: SkillLibrary) -> Self {
        Self {
            analyst,
            vigil,
            library,
            engines: Vec::new(),
        }
    }

    /// Adds a place to search; engines are tried in the order they were added.
    pub fn search(mut self, engine: impl SearchEngine + 'static) -> Self {
        self.engines.push(Box::new(engine));
        self
    }

    /// Checks which Skills `task` needs for `role`, reports those present and
    /// missing, and searches for the missing ones.
    pub async fn scout(&self, role: &str, task: &str) -> Result<Scouting, RoninError> {
        let needs = self.needs(task).await?;
        let have = self.library.skills_for_role(role);
        let mut report = Scouting::default();
        for need in needs {
            if have.iter().any(|s| s.name().eq_ignore_ascii_case(&need)) {
                report.present.push(need);
            } else if let Some(found) = self.find(role, &need).await? {
                report.found.push(found);
            } else {
                report.missing.push(need);
            }
        }
        Ok(report)
    }

    /// Scouts for `job`, and only then lets `strategos` split and assign it.
    pub async fn assign(
        &self,
        strategos: &Strategos,
        role: &str,
        job: &str,
    ) -> Result<(Scouting, Vec<Assignment>), RoninError> {
        let report = self.scout(role, job).await?;
        let assignments = strategos.handle(job).await?;
        Ok((report, assignments))
    }

    async fn needs(&self, task: &str) -> Result<Vec<String>, RoninError> {
        let answer = self
            .analyst
            .answer(&format!(
                "Task: {task}\nList the Skills this task needs, one short Skill name per line, \
                 nothing else. Reply `none` if it needs no Skill."
            ))
            .await
            .map_err(RoninError::Analyst)?;
        let mut needs: Vec<String> = Vec::new();
        for line in answer.lines() {
            let name = line.trim().trim_start_matches(['-', '*', ' ']).trim();
            if !name.is_empty()
                && !name.eq_ignore_ascii_case("none")
                && !needs.iter().any(|n| n.eq_ignore_ascii_case(name))
            {
                needs.push(name.to_string());
            }
        }
        Ok(needs)
    }

    /// Searches every engine for `need` and submits candidates to Vigil until one passes.
    async fn find(&self, role: &str, need: &str) -> Result<Option<FoundSkill>, RoninError> {
        let query = format!("{need} agent skill SKILL.md");
        for engine in &self.engines {
            let Ok(hits) = engine.search(&query).await else {
                continue;
            };
            for hit in hits.iter().take(HITS_PER_ENGINE) {
                let Ok(text) = engine.fetch(hit).await else {
                    continue;
                };
                let Ok(skill) = Skill::from_markdown(&text) else {
                    continue;
                };
                let name = skill.name().to_string();
                let verdict = self
                    .vigil
                    .submit(&self.library, role, skill, SkillSource::Unverified)
                    .await
                    .map_err(RoninError::Vigil)?;
                if matches!(verdict, Verdict::Passed { .. }) {
                    return Ok(Some(FoundSkill {
                        need: need.to_string(),
                        skill: name,
                        verdict,
                    }));
                }
            }
        }
        Ok(None)
    }
}

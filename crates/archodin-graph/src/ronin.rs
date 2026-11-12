//! Ronin finds Skills before work is assigned (ADR 0004).
//!
//! Before a Strategos assigns a Job, Ronin works out which Skills the task needs,
//! reports which ones the Role already has and which are missing, and searches for
//! the missing ones for free: a self-hosted SearXNG when configured, langchain-rust's
//! DuckDuckGo tool otherwise, and the GitHub API (an optional token raises the rate
//! limits and enables code search). Every Skill found goes to Vigil, which holds the
//! ones that pass in Codex's queue for a person to approve.

use base64::Engine as _;
use futures::future::BoxFuture;
use langchain_rust::tools::DuckDuckGoSearchResults;

use crate::http_guard::{
    read_capped, same_origin, shared_client, text_capped, validate_endpoint, validate_url,
    MAX_RESPONSE_BYTES,
};
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

/// Fetches `url` with the hardened client (KR-01): the URL must pass the
/// strict check (public `https` only), the request times out after 30 s, at
/// most 5 redirects are followed (only to allowed targets), and at most
/// 2 MiB of the response is read. A refused URL fails here, before any
/// request is sent.
async fn get_text(
    url: &str,
    accept: Option<&str>,
    token: Option<&str>,
) -> Result<String, RoninError> {
    get_text_checked(url, accept, token, false).await
}

/// Like [`get_text`], but `operator_base` marks a URL built from an endpoint
/// the operator configured (a self-hosted SearXNG, the GitHub API): those may
/// be plain `http` on a private host, because the operator chose them. A hit
/// URL from search results uses the operator check only when it shares the
/// search endpoint's origin (it is served by the operator's own service);
/// any other origin always goes through the strict check in [`get_text`].
async fn get_text_checked(
    url: &str,
    accept: Option<&str>,
    token: Option<&str>,
    operator_base: bool,
) -> Result<String, RoninError> {
    let checked = if operator_base {
        validate_endpoint(url)
    } else {
        validate_url(url)
    };
    checked.map_err(|e| RoninError::Fetch(format!("refused {url}: {e}")))?;
    let mut request = shared_client()
        .map_err(|e| RoninError::Fetch(format!("could not build HTTP client: {e}")))?
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
    let body = read_capped(response)
        .await
        .map_err(|e| RoninError::Fetch(format!("{url}: {e}")))?;
    Ok(text_capped(body))
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
            // The SearXNG base URL is operator configuration, so this request
            // goes through the operator check; a hit on the same origin as
            // this base URL is fetched the same way (see `fetch` below), any
            // other hit origin stays strict.
            let body = get_text_checked(&url, Some("application/json"), None, true).await?;
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

    fn fetch<'a>(&'a self, hit: &'a SearchHit) -> BoxFuture<'a, Result<String, RoninError>> {
        Box::pin(async move {
            // A hit on the same origin as this SearXNG is served by that
            // service itself, so it uses the operator check; any other
            // origin is attacker-influenced and stays strict.
            let operator = same_origin(&self.base_url, &hit.url);
            get_text_checked(&hit.url, None, None, operator).await
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
///
/// Search and file fetch go through the typed octocrab handlers
/// (`search().repositories()`, `search().code()`, `repos().get_content()`),
/// and the octocrab type never leaves this struct — the `SearchEngine` seam
/// (`search` + `fetch`) is unchanged (ADR 0008).
///
/// Three compensations keep the KR-01 protections the hand-rolled client had:
/// the base URI is validated once with `validate_endpoint` at construction;
/// every request runs on `http_guard::shared_client()` (30 s timeout, the
/// audited redirect policy), injected via `ReqwestClientConfig::Custom`; and
/// a fetched file is capped — `Content.size` is checked before its base64
/// body is decoded, and the encoded payload itself is bounded too, so a
/// lying `size` field cannot smuggle a huge allocation in.
pub struct GitHubSearch {
    api_url: String,
    token: Option<String>,
}

impl GitHubSearch {
    /// Searches the GitHub API at `api_url` (normally `https://api.github.com`).
    /// The URL is validated once here, with the operator check, so a blocked
    /// shape fails at construction instead of at the first request.
    pub fn new(api_url: impl Into<String>) -> Result<Self, RoninError> {
        let api_url = api_url.into().trim_end_matches('/').to_string();
        validate_endpoint(&api_url)
            .map_err(|e| RoninError::Search(format!("refused GitHub API URL {api_url}: {e}")))?;
        Ok(Self {
            api_url,
            token: None,
        })
    }

    /// Uses `token` for every request.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// The octocrab client for this search: the operator-validated base URI,
    /// the optional token as a `Bearer` header, and the hardened shared
    /// client underneath. Built per call — cloning the shared client is cheap
    /// (reference-counted) and the builder only assembles layers — so
    /// `token()` stays a plain builder method with no cache to go stale.
    fn crab(&self) -> Result<octocrab::Octocrab, RoninError> {
        let mut builder = octocrab::Octocrab::builder();
        builder = builder
            .base_uri(self.api_url.as_str())
            .map_err(|e| RoninError::Search(e.to_string()))?;
        if let Some(token) = &self.token {
            builder = builder.personal_token(token.clone());
        }
        let client = shared_client()
            .map_err(|e| RoninError::Search(format!("could not build HTTP client: {e}")))?;
        builder
            .build_with_reqwest(octocrab::ReqwestClientConfig::Custom(client))
            .map_err(|e| RoninError::Search(e.to_string()))
    }

    /// Repositories matching `query`, each pointing at its `SKILL.md` on the
    /// contents API. A hit without a `full_name` is dropped: there is no
    /// contents URL to build for it.
    async fn search_repositories(&self, query: &str) -> Result<Vec<SearchHit>, RoninError> {
        let page = self
            .crab()?
            .search()
            .repositories(query)
            .per_page(HITS_PER_ENGINE as u8)
            .send()
            .await
            .map_err(|e| RoninError::Search(e.to_string()))?;
        Ok(page
            .items
            .into_iter()
            .filter_map(|r| {
                let full_name = r.full_name?;
                Some(SearchHit {
                    url: format!("{}/repos/{full_name}/contents/SKILL.md", self.api_url),
                    title: full_name,
                    snippet: r.description.unwrap_or_default(),
                })
            })
            .collect())
    }

    /// `SKILL.md` files matching `query` (code search, token only).
    async fn search_code(&self, query: &str) -> Result<Vec<SearchHit>, RoninError> {
        let page = self
            .crab()?
            .search()
            .code(&format!("{query} filename:SKILL.md"))
            .per_page(HITS_PER_ENGINE as u8)
            .send()
            .await
            .map_err(|e| RoninError::Search(e.to_string()))?;
        Ok(page
            .items
            .into_iter()
            .map(|c| SearchHit {
                title: c.html_url.to_string(),
                url: c.url.to_string(),
                snippet: c.name,
            })
            .collect())
    }

    /// The `SKILL.md` behind a contents-API path, decoded from the typed
    /// `get_content` answer. The declared `size` is the decoded size, so it
    /// is checked before the base64 body is decoded; the encoded payload is
    /// bounded as well, because the field could lie. `Content::decoded_content`
    /// is deliberately not used: it unwraps a bad base64 body, and a panic on
    /// a job path is not an error (KR-06).
    async fn fetch_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
    ) -> Result<String, RoninError> {
        let at = format!("{}/repos/{owner}/{repo}/contents/{path}", self.api_url);
        let refused = |why: &str| RoninError::Fetch(format!("{at}: {why}"));
        let mut items = self
            .crab()?
            .repos(owner, repo)
            .get_content()
            .path(path)
            .send()
            .await
            .map_err(|e| RoninError::Fetch(e.to_string()))?;
        let mut items = items.take_items();
        if items.len() != 1 {
            return Err(refused("not a single file"));
        }
        // Exactly one item was just checked.
        let item = items.pop().expect("one item");
        if item.r#type != "file" {
            return Err(refused("not a file"));
        }
        if item.size < 0 || item.size as u64 > MAX_RESPONSE_BYTES as u64 {
            return Err(refused(&format!(
                "exceeds the {MAX_RESPONSE_BYTES} byte limit"
            )));
        }
        if let Some(encoding) = &item.encoding {
            if encoding != "base64" {
                return Err(refused(&format!("unsupported encoding {encoding}")));
            }
        }
        let encoded = item.content.unwrap_or_default();
        // Base64 inflates by 4/3 plus padding: bound the payload itself so a
        // small `size` field cannot smuggle a huge decode in.
        if encoded.len() > MAX_RESPONSE_BYTES * 4 / 3 + 4 {
            return Err(refused(&format!(
                "exceeds the {MAX_RESPONSE_BYTES} byte limit"
            )));
        }
        let cleaned: Vec<u8> = encoded
            .bytes()
            .filter(|b| !b.is_ascii_whitespace())
            .collect();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&cleaned)
            .map_err(|e| refused(&format!("not valid base64: {e}")))?;
        // The encoded-length bound above cannot tell 2 MiB from 2 MiB + 1
        // (base64 padding), so the decoded length is checked too: this is
        // what catches a lying `size` field.
        if decoded.len() > MAX_RESPONSE_BYTES {
            return Err(refused(&format!(
                "exceeds the {MAX_RESPONSE_BYTES} byte limit"
            )));
        }
        Ok(String::from_utf8_lossy(&decoded).into_owned())
    }
}

/// Splits a contents-API URL on this API's own origin into
/// `(owner, repo, path)`: `/repos/{owner}/{repo}/contents/{path...}`.
/// Anything else (another origin, another route) is not a contents URL.
fn parse_contents_url(base: &str, url: &str) -> Option<(String, String, String)> {
    if !same_origin(base, url) {
        return None;
    }
    let parsed = reqwest::Url::parse(url).ok()?;
    let mut segments = parsed.path_segments()?;
    if segments.next()? != "repos" {
        return None;
    }
    let owner = segments.next()?.to_string();
    let repo = segments.next()?.to_string();
    if segments.next()? != "contents" {
        return None;
    }
    let path: Vec<&str> = segments.collect();
    if path.is_empty() {
        return None;
    }
    Some((owner, repo, path.join("/")))
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
            match self.search_repositories(query).await {
                Ok(items) => {
                    succeeded = true;
                    hits.extend(items);
                }
                Err(e) => failure = Some(e),
            }
            if self.token.is_some() {
                match self.search_code(query).await {
                    Ok(items) => {
                        succeeded = true;
                        hits.extend(items);
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
            // A hit on the same origin as this GitHub API (the contents API
            // lives there) goes through the typed octocrab handler; any other
            // origin is attacker-influenced and stays on the strict path, and
            // a same-origin URL that is not a contents URL keeps the old raw
            // fetch instead of being forced into a shape it has not got.
            if let Some((owner, repo, path)) = parse_contents_url(&self.api_url, &hit.url) {
                self.fetch_contents(&owner, &repo, &path).await
            } else {
                let operator = same_origin(&self.api_url, &hit.url);
                get_text_checked(
                    &hit.url,
                    Some("application/vnd.github.raw+json"),
                    self.token.as_deref(),
                    operator,
                )
                .await
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_guard::MAX_RESPONSE_BYTES;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            // A current-thread runtime with no special settings always builds.
            .expect("a test runtime always builds")
            .block_on(future)
    }

    /// Every URL in this table must fail in validation, before any request
    /// is sent — so these tests need no network and never touch the target.
    #[test]
    fn fetch_refuses_ssrf_targets_before_any_request() {
        for url in [
            "http://169.254.169.254/latest/meta-data/",
            "https://169.254.169.254/",
            "http://localhost/",
            "https://localhost/",
            "http://10.0.0.1/",
            "https://192.168.1.1/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "http://example.com/skill.md",
        ] {
            let err = block_on(get_text(url, None, None));
            assert!(
                matches!(err, Err(RoninError::Fetch(_))),
                "{url} must be refused with a Fetch error, got {err:?}"
            );
        }
    }

    #[test]
    fn fetch_refuses_credentials_in_the_url() {
        let err = block_on(get_text(
            "https://user:pass@example.com/skill.md",
            None,
            None,
        ));
        assert!(matches!(err, Err(RoninError::Fetch(_))));
    }

    /// A response over the 2 MiB cap fails end-to-end, not just in the pure
    /// `push_limited` helper: the body is refused with `TooLarge` instead of
    /// being read unbounded through `get_text_checked` (KR-01, K-03).
    #[test]
    fn fetch_refuses_a_response_over_the_size_limit() {
        let mut server = mockito::Server::new();
        let big = "x".repeat(MAX_RESPONSE_BYTES + 1);
        let mock = server
            .mock("GET", "/big")
            .with_status(200)
            .with_body(big)
            .create();
        let url = format!("{}/big", server.url());
        let result = block_on(get_text_checked(&url, None, None, true));
        mock.assert();
        match result {
            Err(RoninError::Fetch(message)) => assert!(
                message.contains("exceeded"),
                "an over-limit body must fail as TooLarge, got {message:?}"
            ),
            other => panic!("an over-limit body must fail as TooLarge, got {other:?}"),
        }
    }

    /// A redirect to the cloud metadata address is refused end-to-end: the
    /// redirect policy never follows a hop off the starting origin to a
    /// blocked host (KR-01, K-03).
    #[test]
    fn fetch_refuses_a_redirect_to_the_metadata_address() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/hop")
            .with_status(302)
            .with_header("location", "https://169.254.169.254/")
            .create();
        let url = format!("{}/hop", server.url());
        let err = block_on(get_text_checked(&url, None, None, true));
        mock.assert();
        assert!(
            matches!(err, Err(RoninError::Search(_))),
            "a redirect to metadata must fail while sending, got {err:?}"
        );
    }

    /// The operator-configured GitHub base URI is validated once, at
    /// construction: a blocked shape never reaches the HTTP client, and a
    /// plain-`http` loopback endpoint (the mock servers in these tests) is
    /// still accepted because the operator chose it (ADR 0008).
    #[test]
    fn github_search_rejects_a_blocked_base_uri_at_construction() {
        for url in [
            "ftp://example.com",
            "file:///etc/passwd",
            "https://user:pass@api.github.com",
            "https://",
            "not a url",
        ] {
            assert!(
                GitHubSearch::new(url).is_err(),
                "{url} must be rejected at construction"
            );
        }
        for url in [
            "https://api.github.com",
            "http://127.0.0.1:1234",
            "http://localhost:8080",
        ] {
            assert!(
                GitHubSearch::new(url).is_ok(),
                "{url} must be accepted as an operator endpoint"
            );
        }
    }

    /// The configured token is sent as an `Authorization: Bearer` header on
    /// the octocrab path (a mock server proves the header arrives).
    #[test]
    fn github_search_sends_the_configured_token() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/search/repositories")
            .match_query(mockito::Matcher::Any)
            .match_header("authorization", "Bearer tok-1")
            .with_header("content-type", "application/json")
            .with_body(r#"{"items": []}"#)
            .create();
        let engine = GitHubSearch::new(server.url())
            .expect("a mock URL is a valid operator endpoint")
            .token("tok-1");
        let hits = block_on(engine.search("sql agent skill SKILL.md"))
            .expect("the search itself succeeds");
        assert!(hits.is_empty());
        mock.assert();
    }

    /// A contents entry whose declared size is over the 2 MiB cap is refused
    /// before its base64 body is decoded (KR-01, ADR 0008).
    #[test]
    fn github_fetch_refuses_a_file_over_the_size_limit() {
        use base64::Engine as _;
        let mut server = mockito::Server::new();
        let mock = server
            // `match_query` is required: octocrab's `get_content` serializes
            // an empty parameter set, so the request arrives as
            // `.../SKILL.md?` (trailing `?`), which an exact path never
            // matches — mockito compares against path-and-query.
            .mock("GET", "/repos/a/b/contents/SKILL.md")
            .match_query(mockito::Matcher::Any)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "type": "file",
                    "encoding": "base64",
                    "size": MAX_RESPONSE_BYTES + 1,
                    "name": "SKILL.md",
                    "path": "SKILL.md",
                    "content": base64::engine::general_purpose::STANDARD.encode("tiny"),
                    "sha": "abc",
                    "url": format!("{}/repos/a/b/contents/SKILL.md", server.url()),
                    "git_url": format!("{}/repos/a/b/git/blobs/abc", server.url()),
                    "html_url": "https://github.com/a/b/blob/main/SKILL.md",
                    "download_url": format!("{}/a/b/raw/SKILL.md", server.url()),
                    "_links": {"self": format!("{}/repos/a/b/contents/SKILL.md", server.url())},
                })
                .to_string(),
            )
            .create();
        let engine =
            GitHubSearch::new(server.url()).expect("a mock URL is a valid operator endpoint");
        let hit = SearchHit {
            title: "a/b".to_string(),
            url: format!("{}/repos/a/b/contents/SKILL.md", server.url()),
            snippet: String::new(),
        };
        let err = block_on(engine.fetch(&hit));
        mock.assert();
        match err {
            Err(RoninError::Fetch(message)) => assert!(
                message.contains("exceed"),
                "an over-limit file must fail as too large, got {message:?}"
            ),
            other => panic!("an over-limit file must fail as too large, got {other:?}"),
        }
    }

    /// The declared size is not trusted on its own: a payload whose base64
    /// body is over the cap is refused even when the `size` field claims
    /// the file is small.
    #[test]
    fn github_fetch_refuses_an_oversized_payload_with_a_lying_size_field() {
        use base64::Engine as _;
        let mut server = mockito::Server::new();
        let huge =
            base64::engine::general_purpose::STANDARD.encode(vec![b'x'; MAX_RESPONSE_BYTES + 1]);
        let mock = server
            .mock("GET", "/repos/a/b/contents/SKILL.md")
            .match_query(mockito::Matcher::Any)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "type": "file",
                    "encoding": "base64",
                    "size": 3,
                    "name": "SKILL.md",
                    "path": "SKILL.md",
                    "content": huge,
                    "sha": "abc",
                    "url": format!("{}/repos/a/b/contents/SKILL.md", server.url()),
                    "git_url": format!("{}/repos/a/b/git/blobs/abc", server.url()),
                    "html_url": "https://github.com/a/b/blob/main/SKILL.md",
                    "download_url": format!("{}/a/b/raw/SKILL.md", server.url()),
                    "_links": {"self": format!("{}/repos/a/b/contents/SKILL.md", server.url())},
                })
                .to_string(),
            )
            .create();
        let engine =
            GitHubSearch::new(server.url()).expect("a mock URL is a valid operator endpoint");
        let hit = SearchHit {
            title: "a/b".to_string(),
            url: format!("{}/repos/a/b/contents/SKILL.md", server.url()),
            snippet: String::new(),
        };
        let err = block_on(engine.fetch(&hit));
        mock.assert();
        assert!(
            matches!(err, Err(RoninError::Fetch(_))),
            "an oversized payload must be refused, got {err:?}"
        );
    }

    /// No retry: a failed search is attempted exactly once. Octocrab is built
    /// without the `retry` feature, so there is no `RetryLayer` in the stack —
    /// a rate limit (or a 5xx) surfaces as an error instead of being retried
    /// behind Ronin's back. ADR 0008 leaves the choice to the integration
    /// (HandleRateLimits or no retry); this integration chooses no retry,
    /// and this test pins that choice.
    #[test]
    fn github_search_does_not_retry_a_failed_request() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/search/repositories")
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_body(r#"{"message": "Internal Server Error"}"#)
            .expect(1)
            .create();
        let engine =
            GitHubSearch::new(server.url()).expect("a mock URL is a valid operator endpoint");
        let err = block_on(engine.search("sql agent skill SKILL.md"));
        mock.assert();
        assert!(
            matches!(err, Err(RoninError::Search(_))),
            "a failed search must surface as an error, got {err:?}"
        );
    }

    /// A redirect to the cloud metadata address is refused on the octocrab
    /// path too: with octocrab built without `follow-redirect`, no Tower
    /// layer runs ahead of the injected shared client's redirect policy
    /// (KR-01, ADR 0008).
    #[test]
    fn github_search_refuses_a_redirect_to_the_metadata_address() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/search/repositories")
            .match_query(mockito::Matcher::Any)
            .with_status(302)
            .with_header("location", "https://169.254.169.254/")
            .create();
        let engine =
            GitHubSearch::new(server.url()).expect("a mock URL is a valid operator endpoint");
        let err = block_on(engine.search("sql agent skill SKILL.md"));
        mock.assert();
        assert!(
            matches!(err, Err(RoninError::Search(_))),
            "a redirect to metadata must fail on the octocrab path, got {err:?}"
        );
    }

    /// A small file behind a contents URL decodes through the octocrab path.
    #[test]
    fn github_fetch_decodes_a_small_file() {
        use base64::Engine as _;
        let text = "---\nname: sql\ndescription: Query databases\n---\nWrite careful SQL.\n";
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/repos/a/b/contents/SKILL.md")
            .match_query(mockito::Matcher::Any)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "type": "file",
                    "encoding": "base64",
                    "size": text.len(),
                    "name": "SKILL.md",
                    "path": "SKILL.md",
                    "content": base64::engine::general_purpose::STANDARD.encode(text),
                    "sha": "abc",
                    "url": format!("{}/repos/a/b/contents/SKILL.md", server.url()),
                    "git_url": format!("{}/repos/a/b/git/blobs/abc", server.url()),
                    "html_url": "https://github.com/a/b/blob/main/SKILL.md",
                    "download_url": format!("{}/a/b/raw/SKILL.md", server.url()),
                    "_links": {"self": format!("{}/repos/a/b/contents/SKILL.md", server.url())},
                })
                .to_string(),
            )
            .create();
        let engine =
            GitHubSearch::new(server.url()).expect("a mock URL is a valid operator endpoint");
        let hit = SearchHit {
            title: "a/b".to_string(),
            url: format!("{}/repos/a/b/contents/SKILL.md", server.url()),
            snippet: String::new(),
        };
        let fetched = block_on(engine.fetch(&hit)).expect("a small file decodes");
        mock.assert();
        assert_eq!(fetched, text);
    }
}

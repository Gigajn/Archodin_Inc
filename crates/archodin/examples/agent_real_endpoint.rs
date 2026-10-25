//! Manual example, not run in CI: an Agent Node against a real OpenAI-compatible endpoint.
//!
//! ```text
//! ARCHODIN_BASE_URL=http://localhost:1234/v1 \
//! ARCHODIN_API_KEY=lm-studio \
//! ARCHODIN_MODEL=your-model \
//! cargo run -p archodin --example agent_real_endpoint -- "What is 2 + 40?"
//! ```
//!
//! The base URL can be 9router, OpenRouter, Groq, LM Studio or vLLM.

use archodin::graph::{AgentNode, AgentTool, Graph, Provider, TypedTool};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
struct AddInput {
    a: i64,
    b: i64,
}

#[derive(Debug, thiserror::Error)]
#[error("cannot add")]
struct AddError;

struct Add;

impl TypedTool for Add {
    type Input = AddInput;
    type Error = AddError;
    fn name(&self) -> String {
        "add".into()
    }
    fn description(&self) -> String {
        "Adds two integers".into()
    }
    async fn call(&self, input: AddInput) -> Result<String, AddError> {
        Ok((input.a + input.b).to_string())
    }
}

#[derive(Clone)]
struct Job {
    request: String,
    answer: String,
}

#[tokio::main]
async fn main() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("set {name}"));
    let provider = Provider::openai_compatible(
        var("ARCHODIN_BASE_URL"),
        var("ARCHODIN_API_KEY"),
        var("ARCHODIN_MODEL"),
    );
    let agent = AgentNode::new(provider, "Answer briefly. Use tools for arithmetic.")
        .tool(AgentTool::typed(Add));
    let graph = Graph::builder()
        .agent_node(
            "agent",
            agent,
            |s: &Job| s.request.clone(),
            |mut s: Job, answer| {
                s.answer = answer.unwrap_or_else(|e| format!("failed: {e}"));
                s
            },
        )
        .entry("agent")
        .build()
        .unwrap();
    let request = std::env::args().nth(1).unwrap_or("What is 2 + 40?".into());
    let done = graph
        .run(Job {
            request,
            answer: String::new(),
        })
        .await
        .unwrap();
    println!("{}", done.answer);
}

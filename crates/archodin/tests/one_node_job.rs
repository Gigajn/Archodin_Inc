use archodin::graph::{Graph, JobEvent};
use futures::StreamExt;

#[derive(Debug, Clone, PartialEq)]
struct Tally {
    total: u32,
}

fn add_ten_graph() -> Graph<Tally> {
    Graph::builder()
        .node("add_ten", |s: Tally| async move {
            Tally {
                total: s.total + 10,
            }
        })
        .entry("add_ten")
        .build()
        .expect("valid graph")
}

#[tokio::test]
async fn running_a_one_node_job_returns_the_updated_state() {
    let final_state = add_ten_graph().run(Tally { total: 1 }).await.unwrap();
    assert_eq!(final_state, Tally { total: 11 });
}

#[tokio::test]
async fn the_job_streams_start_node_events_and_finish_in_order() {
    let graph = add_ten_graph();
    let events: Vec<_> = graph.stream(Tally { total: 1 }).collect().await;
    assert_eq!(
        events,
        vec![
            JobEvent::JobStarted,
            JobEvent::NodeStarted {
                node: "add_ten".into()
            },
            JobEvent::NodeFinished {
                node: "add_ten".into()
            },
            JobEvent::JobFinished {
                state: Tally { total: 11 }
            },
        ]
    );
}

#[test]
fn a_graph_without_a_known_entry_node_is_rejected() {
    let missing = Graph::<Tally>::builder().build();
    assert!(missing.is_err());
    let unknown = Graph::<Tally>::builder()
        .node("a", |s: Tally| async move { s })
        .entry("b")
        .build();
    assert!(unknown.is_err());
}

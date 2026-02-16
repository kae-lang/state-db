//! Basic SDK example: connect to a server, define a machine, spawn, transition, query.
//!
//! Prerequisites: start the SMQL server first:
//!   cargo run --bin smql -- serve
//!
//! Then run this example:
//!   cargo run --example sdk_basic

use smql_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SmqlClient::new("http://localhost:4200")?;

    // Check health
    let healthy = client.health().await?;
    println!("Server healthy: {}", healthy);

    // Define a task machine
    client
        .define_machine(
            r#"DEFINE MACHINE task (
                DATA {
                    title : TEXT
                    owner : TEXT -> OPTIONAL
                }
                STATES { todo, in_progress, done }
                INITIAL STATE todo
                TERMINAL STATES { done }
                TRANSITIONS {
                    todo -> in_progress {}
                    in_progress -> done {}
                }
            )"#,
        )
        .await?;
    println!("Machine 'task' defined.");

    // List machines
    let machines = client.list_machines().await?;
    println!("Machines: {:?}", machines);

    // Spawn instances
    let t1 = client
        .spawn("task", serde_json::json!({"title": "Write docs"}))
        .await?;
    println!("Spawned task: {} (state: {})", t1.id, t1.state);

    let t2 = client
        .spawn("task", serde_json::json!({"title": "Write tests"}))
        .await?;
    println!("Spawned task: {} (state: {})", t2.id, t2.state);

    // Transition t1
    let tr = client
        .transition("task", &t1.id, "in_progress", TransitionOptions::default())
        .await?;
    println!("Transitioned {} -> {}", tr.from_state, tr.to_state);

    // Find all todo tasks
    let todos = client.find("task").in_state("todo").execute().await?;
    println!("Found {} tasks in 'todo' state", todos.len());

    // Get trail
    let trail = client.trail(&t1.id).await?;
    println!("Trail for {}: {} entries", t1.id, trail.len());

    // Aggregate
    let agg = client
        .aggregate("task")
        .measure("COUNT()")
        .group_by_state()
        .execute()
        .await?;
    println!("Aggregate: {}", serde_json::to_string_pretty(&agg)?);

    println!("Done!");
    Ok(())
}

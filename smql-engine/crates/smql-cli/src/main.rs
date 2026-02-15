use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "smql")]
#[command(about = "SMQL Engine — State Machine Query Language")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the SMQL HTTP server
    Serve {
        /// Address to bind to
        #[arg(short, long, default_value = "127.0.0.1:4200")]
        bind: String,
    },

    /// Start an interactive REPL
    Repl,

    /// Execute a single SMQL statement from a string
    Exec {
        /// The SMQL statement to execute
        statement: String,
    },

    /// Execute SMQL from a file
    Run {
        /// Path to the .smql file
        file: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve { bind }) => {
            tracing_subscriber::fmt::init();
            let server = smql_server::SmqlServer::new();
            if let Err(e) = server.serve(&bind).await {
                eprintln!("Server error: {}", e);
                std::process::exit(1);
            }
        }

        Some(Commands::Exec { statement }) => {
            run_statements(&statement).await;
        }

        Some(Commands::Run { file }) => {
            match std::fs::read_to_string(&file) {
                Ok(content) => {
                    run_statements(&content).await;
                }
                Err(e) => {
                    eprintln!("Cannot read file {}: {}", file, e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::Repl) | None => {
            smql_cli::repl::run_repl().await;
        }
    }
}

/// Resolve $N references (1-indexed) in instance_id fields to spawned IDs.
fn resolve_ref(id: &str, spawned_ids: &[String]) -> String {
    if let Some(num_str) = id.strip_prefix('$') {
        if let Ok(n) = num_str.parse::<usize>() {
            if n >= 1 && n <= spawned_ids.len() {
                return spawned_ids[n - 1].clone();
            }
            eprintln!("Warning: ${} not yet spawned (only {} spawns so far)", n, spawned_ids.len());
        }
    }
    id.to_string()
}

async fn run_statements(input: &str) {
    use smql_ast::command::{Command, Statement};

    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let engine = smql_engine_core::Engine::new(catalog, storage);

    let stmts = match smql_parser::parse(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    };

    let mut spawned_ids: Vec<String> = Vec::new();

    for stmt in stmts {
        match stmt {
            Statement::Command(cmd) => {
                match cmd {
                    Command::Spawn(spawn_cmd) => {
                        match engine.spawn(&spawn_cmd).await {
                            Ok(result) => {
                                let id = result.instance.id.to_string();
                                println!(
                                    "Spawned {} instance: {} (state: {})",
                                    result.instance.machine, id, result.instance.state
                                );
                                spawned_ids.push(id);
                            }
                            Err(e) => eprintln!("Error: {}", e),
                        }
                    }
                    Command::Transition(mut t_cmd) => {
                        t_cmd.instance_id = resolve_ref(&t_cmd.instance_id, &spawned_ids);
                        smql_cli::repl::execute_command_public(
                            Command::Transition(t_cmd),
                            &engine,
                        )
                        .await;
                    }
                    Command::TryTransition(mut t_cmd) => {
                        t_cmd.instance_id = resolve_ref(&t_cmd.instance_id, &spawned_ids);
                        smql_cli::repl::execute_command_public(
                            Command::TryTransition(t_cmd),
                            &engine,
                        )
                        .await;
                    }
                    other => {
                        smql_cli::repl::execute_command_public(other, &engine).await;
                    }
                }
            }
            Statement::Query(mut query) => {
                // Resolve $N in instance_id fields for GET and TRAIL queries
                match &mut query {
                    smql_ast::query::Query::Get(ref mut g) => {
                        g.instance_id = resolve_ref(&g.instance_id, &spawned_ids);
                    }
                    smql_ast::query::Query::Trail(ref mut t) => {
                        t.instance_id = resolve_ref(&t.instance_id, &spawned_ids);
                    }
                    _ => {}
                }
                smql_cli::repl::execute_query_public(query, &engine).await;
            }
        }
    }
}

use clap::{Parser, Subcommand};
use smql_storage::Storage;
use std::sync::Arc;

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

        /// Storage backend: "memory" or a filesystem path for RocksDB
        #[arg(short, long, default_value = "memory")]
        storage: String,
    },

    /// Start an interactive REPL
    Repl {
        /// Storage backend: "memory" or a filesystem path for RocksDB
        #[arg(short, long, default_value = "memory")]
        storage: String,
    },

    /// Execute a single SMQL statement from a string
    Exec {
        /// The SMQL statement to execute
        statement: String,

        /// Storage backend: "memory" or a filesystem path for RocksDB
        #[arg(short, long, default_value = "memory")]
        storage: String,
    },

    /// Execute SMQL from a file
    Run {
        /// Path to the .smql file
        file: String,

        /// Storage backend: "memory" or a filesystem path for RocksDB
        #[arg(short, long, default_value = "memory")]
        storage: String,
    },
}

fn create_storage(arg: &str) -> Arc<dyn Storage> {
    if arg == "memory" {
        return Arc::new(smql_storage::MemoryStorage::new());
    }

    #[cfg(feature = "rocksdb")]
    {
        match smql_storage::RocksDBStorage::open(arg) {
            Ok(s) => return Arc::new(s),
            Err(e) => {
                eprintln!("Failed to open RocksDB at '{}': {}", arg, e);
                std::process::exit(1);
            }
        }
    }

    #[cfg(not(feature = "rocksdb"))]
    {
        eprintln!(
            "RocksDB storage requested ('{}') but the 'rocksdb' feature is not enabled.\n\
             Rebuild with: cargo build --features rocksdb",
            arg
        );
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve { bind, storage }) => {
            tracing_subscriber::fmt()
                .json()
                .with_target(true)
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .init();
            let st = create_storage(&storage);
            let server = smql_server::SmqlServer::with_storage(st);
            if let Err(e) = server.serve(&bind).await {
                eprintln!("Server error: {}", e);
                std::process::exit(1);
            }
        }

        Some(Commands::Exec { statement, storage }) => {
            let st = create_storage(&storage);
            run_statements(&statement, st).await;
        }

        Some(Commands::Run { file, storage }) => {
            let st = create_storage(&storage);
            match std::fs::read_to_string(&file) {
                Ok(content) => {
                    run_statements(&content, st).await;
                }
                Err(e) => {
                    eprintln!("Cannot read file {}: {}", file, e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::Repl { storage }) => {
            let st = create_storage(&storage);
            smql_cli::repl::run_repl_with_storage(st).await;
        }

        None => {
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

async fn run_statements(input: &str, storage: Arc<dyn Storage>) {
    use smql_ast::command::{Command, Statement};

    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
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

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
            let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
            let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
            let engine = smql_engine_core::Engine::new(catalog, storage);

            match smql_parser::parse(&statement) {
                Ok(stmts) => {
                    for stmt in stmts {
                        match stmt {
                            smql_ast::command::Statement::Command(cmd) => {
                                println!("Parsed command: {}", cmd);
                            }
                            smql_ast::command::Statement::Query(query) => {
                                match engine.execute_query(&query).await {
                                    Ok(result) => {
                                        println!("{:?}", result);
                                    }
                                    Err(e) => {
                                        eprintln!("Query error: {}", e);
                                        std::process::exit(1);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Parse error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::Run { file }) => {
            match std::fs::read_to_string(&file) {
                Ok(content) => match smql_parser::parse(&content) {
                    Ok(stmts) => {
                        println!("Parsed {} statement(s) from {}", stmts.len(), file);
                    }
                    Err(e) => {
                        eprintln!("Parse error in {}: {}", file, e);
                        std::process::exit(1);
                    }
                },
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

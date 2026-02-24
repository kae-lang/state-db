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

    /// Generate typed Rust code from .smql machine definitions
    Codegen {
        /// Input .smql files or directory
        #[arg(short, long)]
        input: Vec<String>,

        /// Output directory for generated code
        #[arg(short, long, default_value = "src/generated")]
        output: String,

        /// Target language (only "rust" supported)
        #[arg(short, long, default_value = "rust")]
        lang: String,
    },
}

fn create_storage(arg: &str) -> Arc<dyn Storage> {
    if arg == "memory" {
        return Arc::new(smql_storage::MemoryStorage::new());
    }

    #[cfg(feature = "rocksdb")]
    {
        match smql_storage::RocksDBStorage::open(arg) {
            Ok(s) => Arc::new(s),
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

        Some(Commands::Codegen {
            input,
            output,
            lang,
        }) => {
            if lang != "rust" {
                eprintln!("Unsupported language '{}'. Only 'rust' is supported.", lang);
                std::process::exit(1);
            }

            // Collect .smql files from input paths
            let mut smql_files = Vec::new();
            for path in &input {
                let meta = match std::fs::metadata(path) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Cannot access '{}': {}", path, e);
                        std::process::exit(1);
                    }
                };
                if meta.is_dir() {
                    match std::fs::read_dir(path) {
                        Ok(entries) => {
                            for entry in entries.flatten() {
                                let p = entry.path();
                                if p.extension().is_some_and(|ext| ext == "smql") {
                                    smql_files.push(p.to_string_lossy().to_string());
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Cannot read directory '{}': {}", path, e);
                            std::process::exit(1);
                        }
                    }
                } else {
                    smql_files.push(path.clone());
                }
            }

            if smql_files.is_empty() {
                eprintln!("No .smql files found in input paths");
                std::process::exit(1);
            }

            let file_refs: Vec<&str> = smql_files.iter().map(|s| s.as_str()).collect();
            let generator = match smql_codegen::CodeGenerator::from_files(&file_refs) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("Codegen error: {}", e);
                    std::process::exit(1);
                }
            };

            // Generate output
            if let Err(e) = std::fs::create_dir_all(&output) {
                eprintln!("Cannot create output directory '{}': {}", output, e);
                std::process::exit(1);
            }

            let files = generator.generate_rust();
            for file in &files {
                let full_path = format!("{}/{}", output, file.path);
                if let Err(e) = std::fs::write(&full_path, &file.content) {
                    eprintln!("Cannot write '{}': {}", full_path, e);
                    std::process::exit(1);
                }
                println!("Generated: {}", full_path);
            }

            // Also write a mod.rs
            let mod_content: String = files
                .iter()
                .map(|f| {
                    let mod_name = f.path.trim_end_matches(".rs");
                    format!("pub mod {};", mod_name)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mod_path = format!("{}/mod.rs", output);
            if let Err(e) = std::fs::write(
                &mod_path,
                format!("// Auto-generated by smql-codegen\n{}\n", mod_content),
            ) {
                eprintln!("Cannot write '{}': {}", mod_path, e);
                std::process::exit(1);
            }
            println!("Generated: {}", mod_path);
            println!("Done. {} file(s) generated.", files.len() + 1);
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
            eprintln!(
                "Warning: ${} not yet spawned (only {} spawns so far)",
                n,
                spawned_ids.len()
            );
        }
    }
    id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_ref_dollar_1() {
        let spawned = vec!["ABC123".to_string(), "DEF456".to_string()];
        assert_eq!(resolve_ref("$1", &spawned), "ABC123");
        assert_eq!(resolve_ref("$2", &spawned), "DEF456");
    }

    #[test]
    fn resolve_ref_no_dollar() {
        let spawned = vec!["ABC".to_string()];
        assert_eq!(resolve_ref("plain_id", &spawned), "plain_id");
    }

    #[test]
    fn resolve_ref_out_of_range() {
        let spawned = vec!["ABC".to_string()];
        // $5 is out of range — should return "$5" as-is (with warning printed)
        assert_eq!(resolve_ref("$5", &spawned), "$5");
    }

    #[test]
    fn resolve_ref_zero_index() {
        let spawned = vec!["ABC".to_string()];
        // $0 is not valid (1-indexed) — n >= 1 check fails
        assert_eq!(resolve_ref("$0", &spawned), "$0");
    }

    #[test]
    fn resolve_ref_non_numeric() {
        let spawned = vec!["ABC".to_string()];
        // $abc is not parseable as usize
        assert_eq!(resolve_ref("$abc", &spawned), "$abc");
    }

    #[test]
    fn resolve_ref_empty_spawned() {
        let spawned: Vec<String> = vec![];
        assert_eq!(resolve_ref("$1", &spawned), "$1");
    }

    #[test]
    fn create_storage_memory() {
        let storage = create_storage("memory");
        // Just verify it returns successfully
        assert!(std::sync::Arc::strong_count(&storage) >= 1);
    }
}

async fn run_statements(input: &str, storage: Arc<dyn Storage>) {
    use smql_ast::command::{Command, Statement};

    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
    let engine = smql_engine_core::Engine::new(catalog, storage);
    engine.wire_callback();

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
            Statement::Command(cmd) => match cmd {
                Command::Spawn(spawn_cmd) => match engine.spawn(&spawn_cmd).await {
                    Ok(result) => {
                        let id = result.instance.id.to_string();
                        println!(
                            "Spawned {} instance: {} (state: {})",
                            result.instance.machine, id, result.instance.state
                        );
                        spawned_ids.push(id);
                    }
                    Err(e) => eprintln!("Error: {}", e),
                },
                Command::Transition(mut t_cmd) => {
                    t_cmd.instance_id = resolve_ref(&t_cmd.instance_id, &spawned_ids);
                    smql_cli::repl::execute_command_public(Command::Transition(t_cmd), &engine)
                        .await;
                }
                Command::TryTransition(mut t_cmd) => {
                    t_cmd.instance_id = resolve_ref(&t_cmd.instance_id, &spawned_ids);
                    smql_cli::repl::execute_command_public(Command::TryTransition(t_cmd), &engine)
                        .await;
                }
                other => {
                    smql_cli::repl::execute_command_public(other, &engine).await;
                }
            },
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
            Statement::Transaction(stmts) => {
                match engine.execute_transaction(&stmts).await {
                    Ok(results) => {
                        println!("Transaction committed ({} steps).", results.len());
                        for (i, r) in results.iter().enumerate() {
                            println!("  Step {}: {:?}", i, r);
                        }
                    }
                    Err(e) => eprintln!("Transaction failed: {}", e),
                }
            }
        }
    }
}

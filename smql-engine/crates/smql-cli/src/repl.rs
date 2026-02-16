use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use smql_ast::command::{Command, Statement};
use smql_catalog::MachineCatalog;
use smql_engine_core::engine::Engine;
use smql_engine_core::query::QueryResult;
use smql_storage::{MemoryStorage, Storage};
use std::sync::Arc;

/// Run the SMQL REPL with a custom storage backend.
pub async fn run_repl_with_storage(storage: Arc<dyn Storage>) {
    let catalog = Arc::new(MachineCatalog::new());
    let engine = Engine::new(catalog, storage);
    engine.wire_callback();
    run_repl_inner(engine).await;
}

/// Run the SMQL REPL (Read-Eval-Print Loop).
pub async fn run_repl() {
    let catalog = Arc::new(MachineCatalog::new());
    let storage = Arc::new(MemoryStorage::new());
    let engine = Engine::new(catalog, storage);
    engine.wire_callback();
    run_repl_inner(engine).await;
}

async fn run_repl_inner(engine: Engine) {
    println!("SMQL Engine v0.1.0 — Interactive REPL");
    println!("Type SMQL statements or .help for commands. Press Ctrl+D to exit.\n");

    let mut rl = DefaultEditor::new().expect("Failed to create editor");
    let mut multiline_buf = String::new();

    loop {
        let prompt = if multiline_buf.is_empty() {
            "smql> "
        } else {
            "  ... "
        };

        match rl.readline(prompt) {
            Ok(line) => {
                let trimmed = line.trim();

                // Dot commands
                if multiline_buf.is_empty() && trimmed.starts_with('.') {
                    handle_dot_command(trimmed, &engine);
                    continue;
                }

                // Empty line
                if trimmed.is_empty() {
                    if !multiline_buf.is_empty() {
                        // Execute accumulated multiline input
                        let input = std::mem::take(&mut multiline_buf);
                        let _ = rl.add_history_entry(&input);
                        execute_input(&input, &engine).await;
                    }
                    continue;
                }

                // Accumulate multiline input
                if !multiline_buf.is_empty() {
                    multiline_buf.push(' ');
                }
                multiline_buf.push_str(trimmed);

                // Check if the statement looks complete (ends with a semicolon or is a full statement)
                if trimmed.ends_with(';') || is_complete_statement(&multiline_buf) {
                    let input = std::mem::take(&mut multiline_buf);
                    let input = input.trim_end_matches(';').to_string();
                    let _ = rl.add_history_entry(&input);
                    execute_input(&input, &engine).await;
                }
            }
            Err(ReadlineError::Interrupted) => {
                multiline_buf.clear();
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                println!("Bye!");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }
}

fn handle_dot_command(cmd: &str, engine: &Engine) {
    match cmd {
        ".help" | ".h" => {
            println!("Available commands:");
            println!("  .help          Show this help message");
            println!("  .machines      List registered machines");
            println!("  .quit          Exit the REPL");
            println!();
            println!("SMQL statements:");
            println!("  DEFINE MACHINE name {{ ... }}");
            println!("  SPAWN Machine {{ field: value }}");
            println!("  TRANSITION Machine instance_id TO state");
            println!("  GET Machine instance_id");
            println!("  FIND Machine WHERE condition");
            println!("  TRAIL OF instance_id");
            println!("  AGGREGATE Machine MEASURE COUNT()");
        }
        ".machines" | ".m" => {
            let names = engine.catalog.list();
            if names.is_empty() {
                println!("No machines registered.");
            } else {
                println!("Registered machines:");
                for name in names {
                    println!("  - {}", name);
                }
            }
        }
        ".quit" | ".q" | ".exit" => {
            println!("Bye!");
            std::process::exit(0);
        }
        _ => {
            eprintln!("Unknown command: {}", cmd);
        }
    }
}

fn is_complete_statement(input: &str) -> bool {
    // A simple heuristic: check if braces and parens are balanced
    let brace_opens = input.chars().filter(|c| *c == '{').count();
    let brace_closes = input.chars().filter(|c| *c == '}').count();
    let paren_opens = input.chars().filter(|c| *c == '(').count();
    let paren_closes = input.chars().filter(|c| *c == ')').count();
    brace_opens == brace_closes && paren_opens == paren_closes
}

async fn execute_input(input: &str, engine: &Engine) {
    let statements = match smql_parser::parse(input) {
        Ok(stmts) => stmts,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            return;
        }
    };

    for stmt in statements {
        match stmt {
            Statement::Command(cmd) => execute_command(cmd, engine).await,
            Statement::Query(query) => execute_query(query, engine).await,
        }
    }
}

/// Execute a command and print results. Public for use from CLI subcommands.
pub async fn execute_command_public(cmd: Command, engine: &Engine) {
    execute_command(cmd, engine).await;
}

/// Execute a query and print results. Public for use from CLI subcommands.
pub async fn execute_query_public(query: smql_ast::query::Query, engine: &Engine) {
    execute_query(query, engine).await;
}

async fn execute_command(cmd: Command, engine: &Engine) {
    match cmd {
        Command::DefineMachine(def) => {
            let name = def.name.clone();
            match engine.catalog.register(def) {
                Ok(warnings) => {
                    println!("Machine '{}' defined.", name);
                    for w in warnings {
                        println!("  Warning: {}", w.message);
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        Command::Spawn(spawn_cmd) => match engine.spawn(&spawn_cmd).await {
            Ok(result) => {
                println!(
                    "Spawned {} instance: {} (state: {})",
                    result.instance.machine, result.instance.id, result.instance.state
                );
            }
            Err(e) => eprintln!("Error: {}", e),
        },

        Command::Transition(t_cmd) => match engine.transition(&t_cmd).await {
            Ok(result) => {
                println!(
                    "Transitioned {} -> {} (instance: {})",
                    result.from_state, result.to_state, result.instance.id
                );
            }
            Err(e) => eprintln!("Error: {}", e),
        },

        Command::TryTransition(t_cmd) => match engine.try_transition(&t_cmd).await {
            Ok(Some(result)) => {
                println!(
                    "Transitioned {} -> {} (instance: {})",
                    result.from_state, result.to_state, result.instance.id
                );
            }
            Ok(None) => {
                println!("Transition not applied (guard conditions not met).");
            }
            Err(e) => eprintln!("Error: {}", e),
        },

        Command::BatchTransition(_) => {
            eprintln!("BATCH TRANSITION not yet implemented");
        }

        Command::AlterMachine(alter_cmd) => match engine.execute_alter_machine(&alter_cmd).await {
            Ok(result) => {
                println!(
                    "Machine '{}' altered (v{}, {} operation(s), {} instance(s) migrated).",
                    result.machine,
                    result.new_version,
                    result.operations_applied,
                    result.instances_migrated
                );
                for w in &result.warnings {
                    println!("  Warning: {}", w);
                }
            }
            Err(e) => eprintln!("Error: {}", e),
        },
    }
}

async fn execute_query(query: smql_ast::query::Query, engine: &Engine) {
    match engine.execute_query(&query).await {
        Ok(result) => print_query_result(result),
        Err(e) => eprintln!("Query error: {}", e),
    }
}

fn print_query_result(result: QueryResult) {
    match result {
        QueryResult::Instance(inst) => {
            println!("Instance: {} ({})", inst.id, inst.machine);
            println!("  State: {}", inst.state);
            println!("  Version: {}", inst.version);
            println!("  Created: {}", inst.created_at);
            for (k, v) in &inst.data {
                println!("  {}: {}", k, v);
            }
        }

        QueryResult::Instances(insts) => {
            println!("{} instance(s) found:", insts.len());
            for inst in &insts {
                println!(
                    "  {} | state: {} | {}",
                    inst.id,
                    inst.state,
                    inst.data
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }

        QueryResult::Trail(entries) => {
            println!("{} trail entries:", entries.len());
            for entry in &entries {
                println!(
                    "  [{}] {} -> {} ({})",
                    entry.sequence,
                    entry.from_state,
                    entry.to_state,
                    entry.timestamp.format("%Y-%m-%d %H:%M:%S")
                );
                if let Some(actor) = &entry.actor {
                    println!("       Actor: {}", actor);
                }
                if let Some(memo) = &entry.memo {
                    println!("       Memo: {}", memo);
                }
            }
        }

        QueryResult::Aggregate(rows) => {
            println!("{} group(s):", rows.len());
            for row in &rows {
                if !row.group_key.is_empty() {
                    let group_str = row
                        .group_key
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect::<Vec<_>>()
                        .join(", ");
                    print!("  [{}]", group_str);
                } else {
                    print!("  [all]");
                }
                for (k, v) in &row.measures {
                    print!(" {}={}", k, v);
                }
                println!();
            }
        }

        QueryResult::Paths(paths) => {
            println!("{} unique path(s):", paths.len());
            for p in &paths {
                println!("  {} -> count: {}", p.path.join(" -> "), p.count);
            }
        }

        QueryResult::Funnel(funnel) => {
            println!("Funnel analysis:");
            for stage in &funnel.stages {
                println!(
                    "  {} : {} ({:.1}%)",
                    stage.state,
                    stage.count,
                    stage.conversion_rate * 100.0
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_complete_statement_balanced_braces() {
        assert!(is_complete_statement(
            "DEFINE MACHINE Foo { STATES { open } }"
        ));
    }

    #[test]
    fn is_complete_statement_unbalanced_braces() {
        assert!(!is_complete_statement(
            "DEFINE MACHINE Foo { STATES { open }"
        ));
    }

    #[test]
    fn is_complete_statement_balanced_parens() {
        assert!(is_complete_statement("AGGREGATE Machine MEASURE COUNT()"));
    }

    #[test]
    fn is_complete_statement_unbalanced_parens() {
        assert!(!is_complete_statement("AGGREGATE Machine MEASURE COUNT("));
    }

    #[test]
    fn is_complete_statement_no_braces_or_parens() {
        assert!(is_complete_statement("TRANSITION Machine id TO closed"));
    }

    #[test]
    fn is_complete_statement_empty_string() {
        assert!(is_complete_statement(""));
    }

    #[test]
    fn is_complete_statement_mixed_balanced() {
        assert!(is_complete_statement(
            "DEFINE MACHINE Foo { HOOKS { ON SPAWN { EMIT(\"created\") } } }"
        ));
    }

    #[test]
    fn is_complete_statement_mixed_unbalanced_brace() {
        assert!(!is_complete_statement(
            "DEFINE MACHINE Foo { HOOKS { ON SPAWN { EMIT(\"created\") }"
        ));
    }

    #[test]
    fn is_complete_statement_mixed_unbalanced_paren() {
        assert!(!is_complete_statement(
            "AGGREGATE Machine MEASURE AVG(price"
        ));
    }

    #[test]
    fn is_complete_statement_nested_balanced() {
        assert!(is_complete_statement("{{ () {} }}"));
    }

    #[test]
    fn is_complete_statement_only_closing_braces() {
        // More closes than opens — the counts happen to match if same number,
        // but here closes > opens so it is unbalanced
        assert!(!is_complete_statement("}}"));
    }
}

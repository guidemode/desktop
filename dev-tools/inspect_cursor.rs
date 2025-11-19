#![allow(warnings)] // Development tool
/// Cursor Database Inspector
///
/// CLI tool for inspecting Cursor protobuf data stored in SQLite databases.
///
/// Usage:
///   # Inspect a specific session database
///   cargo run --bin inspect_cursor -- session ~/.cursor/chats/{hash}/{uuid}/store.db
///
///   # Find tool use examples in a session
///   cargo run --bin inspect_cursor -- tools ~/.cursor/chats/{hash}/{uuid}/store.db
///
///   # Export all blobs as JSON
///   cargo run --bin inspect_cursor -- export ~/.cursor/chats/{hash}/{uuid}/store.db output.json
///
///   # Inspect a single blob by hex
///   cargo run --bin inspect_cursor -- blob 0A4B43616E...
///
///   # List all Cursor sessions
///   cargo run --bin inspect_cursor -- list

use guidemode_desktop::providers::cursor::{debug, discover_sessions};
use std::path::PathBuf;

fn main() {
    // Initialize tracing for debug logs
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let command = &args[1];

    let result = match command.as_str() {
        "session" => {
            if args.len() < 3 {
                eprintln!("Error: Missing database path");
                print_usage();
                std::process::exit(1);
            }
            let db_path = PathBuf::from(&args[2]);
            debug::inspect_session_db(&db_path)
        }

        "tools" => {
            if args.len() < 3 {
                eprintln!("Error: Missing database path");
                print_usage();
                std::process::exit(1);
            }
            let db_path = PathBuf::from(&args[2]);
            debug::find_tool_use_examples(&db_path)
        }

        "export" => {
            if args.len() < 4 {
                eprintln!("Error: Missing database path or output path");
                print_usage();
                std::process::exit(1);
            }
            let db_path = PathBuf::from(&args[2]);
            let output_path = PathBuf::from(&args[3]);
            debug::export_blobs_json(&db_path, &output_path)
        }

        "blob" => {
            if args.len() < 3 {
                eprintln!("Error: Missing hex data");
                print_usage();
                std::process::exit(1);
            }
            let hex_data = &args[2];
            debug::inspect_blob_hex(hex_data)
        }

        "list" => {
            list_sessions()
        }

        _ => {
            eprintln!("Error: Unknown command '{}'", command);
            print_usage();
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {:?}", e);
        std::process::exit(1);
    }
}

fn list_sessions() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Cursor Sessions ===");

    let sessions = discover_sessions()?;

    if sessions.is_empty() {
        println!("No Cursor sessions found");
        return Ok(());
    }

    println!("Found {} sessions:\n", sessions.len());

    for (i, session) in sessions.iter().enumerate() {
        println!("{}. {}", i + 1, session.session_id);
        println!("   Name: {}", session.metadata.name);
        println!("   CWD: {}", session.cwd.as_deref().unwrap_or("unknown"));
        println!("   Database: {}", session.db_path.display());
        println!("   Hash: {}", session.hash);
        println!();
    }

    println!("\nTo inspect a session, run:");
    println!("  cargo run --bin inspect_cursor -- session <database_path>");

    Ok(())
}

fn print_usage() {
    println!("Cursor Database Inspector");
    println!();
    println!("USAGE:");
    println!("  cargo run --bin inspect_cursor -- <COMMAND> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("  list                            List all Cursor sessions");
    println!("  session <db_path>               Inspect a session database");
    println!("  tools <db_path>                 Find tool use examples in a session");
    println!("  export <db_path> <output.json>  Export all blobs as JSON");
    println!("  blob <hex_data>                 Inspect a single blob by hex data");
    println!();
    println!("EXAMPLES:");
    println!("  # List all sessions");
    println!("  cargo run --bin inspect_cursor -- list");
    println!();
    println!("  # Inspect a specific session");
    println!("  cargo run --bin inspect_cursor -- session ~/.cursor/chats/abc123/def456/store.db");
    println!();
    println!("  # Find tool use");
    println!("  cargo run --bin inspect_cursor -- tools ~/.cursor/chats/abc123/def456/store.db");
    println!();
    println!("  # Export to JSON");
    println!("  cargo run --bin inspect_cursor -- export ~/.cursor/chats/abc123/def456/store.db output.json");
}

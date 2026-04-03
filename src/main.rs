use clap::{ArgAction, Parser};
use facter::facts::collection::Collection;
use facter::resolvers;
use std::process;

#[derive(Parser)]
#[command(
    name = "facter",
    about = "Collect and display facts about the system.\n\nIf no queries are given, then all facts will be returned.\n\nExample queries:\n  facter kernel\n  facter networking.ip\n  facter processors.models.0"
)]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Output in JSON format
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,

    /// Output in YAML format
    #[arg(long, action = ArgAction::SetTrue)]
    yaml: bool,

    /// Enable debug logging
    #[arg(long, action = ArgAction::SetTrue)]
    debug: bool,

    /// Enable verbose (info-level) logging
    #[arg(long, action = ArgAction::SetTrue)]
    verbose: bool,

    /// Set log level (debug, info, warn, error)
    #[arg(long = "log-level", default_value = "warn", value_name = "LEVEL")]
    log_level: String,

    /// Disable colorized output
    #[arg(long = "no-color", action = ArgAction::SetTrue)]
    no_color: bool,

    /// Enable colorized output
    #[arg(long, action = ArgAction::SetTrue)]
    color: bool,

    /// Show legacy (flat) facts alongside structured ones
    #[arg(long = "show-legacy", action = ArgAction::SetTrue)]
    show_legacy: bool,

    /// Error on unknown fact queries
    #[arg(long, action = ArgAction::SetTrue)]
    strict: bool,

    /// Fact names or dotted-path queries (e.g. `os.family`)
    queries: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    // Initialise logging
    let level = if cli.debug {
        "debug"
    } else if cli.verbose {
        "info"
    } else {
        cli.log_level.as_str()
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level)).init();

    // Resolve all facts
    let resolvers = resolvers::default_resolvers();
    let mut collection = Collection::new();
    collection.resolve_all(&resolvers);

    let queries = &cli.queries;

    // Strict mode: exit 1 for any query that returns nothing
    if cli.strict {
        for q in queries {
            if collection.query(q).is_none() {
                eprintln!("error: unknown fact \"{}\"", q);
                process::exit(1);
            }
        }
    }

    // Format and print
    let output = if cli.json {
        match collection.write_json(queries) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        }
    } else if cli.yaml {
        match collection.write_yaml(queries) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {}", e);
                process::exit(1);
            }
        }
    } else {
        collection.write_hash(queries)
    };

    // facter always ends output with a newline; write_hash already adds one
    // per line, so only add a trailing newline for JSON/YAML.
    if cli.json || cli.yaml {
        println!("{}", output);
    } else {
        print!("{}", output);
    }
}

use clap::{ArgAction, Parser};
use facter::config;
use facter::external;
use facter::facts::collection::Collection;
use facter::resolvers;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(
    name = "facter",
    about = "Collect and display facts about the system.\n\nIf no queries are given all facts will be returned.\n\nExample queries:\n  facter kernel\n  facter networking.ip\n  facter processors.models.0"
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

    /// Path to config file
    #[arg(long, value_name = "FILE")]
    config: Option<String>,

    /// Block a resolver group (may be repeated)
    #[arg(long = "block", value_name = "GROUP", action = ArgAction::Append)]
    block: Vec<String>,

    /// Disable the blocklist from config (run all resolvers)
    #[arg(long = "no-block", action = ArgAction::SetTrue)]
    no_block: bool,

    /// Disable fact caching
    #[arg(long = "no-cache", action = ArgAction::SetTrue)]
    no_cache: bool,

    /// Additional directory for external facts (may be repeated)
    #[arg(long = "external-dir", value_name = "DIR", action = ArgAction::Append)]
    external_dir: Vec<String>,

    /// Disable external facts
    #[arg(long = "no-external-facts", action = ArgAction::SetTrue)]
    no_external_facts: bool,

    /// Additional directory for custom facts (may be repeated)
    #[arg(long = "custom-dir", value_name = "DIR", action = ArgAction::Append)]
    custom_dir: Vec<String>,

    /// Disable custom facts
    #[arg(long = "no-custom-facts", action = ArgAction::SetTrue)]
    no_custom_facts: bool,

    /// List resolver groups that can be blocked
    #[arg(long = "list-block-groups", action = ArgAction::SetTrue)]
    list_block_groups: bool,

    /// List resolver groups that can be cached
    #[arg(long = "list-cache-groups", action = ArgAction::SetTrue)]
    list_cache_groups: bool,

    /// Fact names or dotted-path queries (e.g. `os.family`)
    queries: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    // ── Load config file ──────────────────────────────────────────────────────
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| config::default_config_path().to_string());
    let cfg = config::load(&config_path).unwrap_or_default();

    // ── Initialise logging ────────────────────────────────────────────────────
    let level = if cli.debug || cfg.debug {
        "debug"
    } else if cli.verbose || cfg.verbose {
        "info"
    } else if let Some(ref l) = cfg.log_level {
        l.as_str()
    } else {
        cli.log_level.as_str()
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level)).init();

    // ── Build blocklist ───────────────────────────────────────────────────────
    let mut blocklist: HashSet<String> = HashSet::new();
    if !cli.no_block {
        blocklist.extend(cfg.blocklist.iter().cloned());
        blocklist.extend(cli.block.iter().cloned());
    }

    // ── Build TTL map ─────────────────────────────────────────────────────────
    let ttls: HashMap<String, u64> = cfg.ttls.clone();

    // ── Handle --list-* flags before resolving ────────────────────────────────
    let resolvers = resolvers::default_resolvers();

    if cli.list_block_groups || cli.list_cache_groups {
        for name in Collection::resolver_group_names(&resolvers) {
            if cli.list_cache_groups {
                if let Some(ttl) = ttls.get(name) {
                    println!("{} ({}s)", name, ttl);
                    continue;
                }
            }
            if cli.list_block_groups {
                println!("{}", name);
            }
        }
        return;
    }

    // ── Resolve facts ─────────────────────────────────────────────────────────
    let mut collection = Collection::new();
    collection.resolve_all(&resolvers, &blocklist, &ttls, cli.no_cache);

    // ── External facts ────────────────────────────────────────────────────────
    if !cli.no_external_facts && !cfg.no_external_facts {
        let mut ext_dirs: Vec<PathBuf> = external::default_dirs();
        // Prepend CLI dirs so they take precedence
        let extra: Vec<PathBuf> = cli
            .external_dir
            .iter()
            .chain(cfg.external_dirs.iter())
            .map(PathBuf::from)
            .collect();
        ext_dirs.splice(0..0, extra);

        for (k, v) in external::load(&ext_dirs) {
            collection.add(k, v);
        }
        // Re-sort after adding external facts
        // (collection doesn't expose sort directly; add a helper or accept unsorted externals)
    }

    // ── Strict mode ───────────────────────────────────────────────────────────
    let queries = &cli.queries;
    if cli.strict {
        for q in queries {
            if collection.query(q).is_none() {
                eprintln!("error: unknown fact \"{}\"", q);
                process::exit(1);
            }
        }
    }

    // ── Format and print ──────────────────────────────────────────────────────
    let output = if cli.json {
        match collection.write_json(queries) {
            Ok(s) => s,
            Err(e) => { eprintln!("error: {}", e); process::exit(1); }
        }
    } else if cli.yaml {
        match collection.write_yaml(queries) {
            Ok(s) => s,
            Err(e) => { eprintln!("error: {}", e); process::exit(1); }
        }
    } else {
        collection.write_hash(queries)
    };

    if cli.json || cli.yaml {
        println!("{}", output);
    } else {
        print!("{}", output);
    }
}

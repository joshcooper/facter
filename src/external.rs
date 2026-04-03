//! External facts loader.
//!
//! Scans `facts.d` directories for:
//!   - `.json`  files  → parsed as JSON objects
//!   - `.yaml` / `.yml` files → parsed as YAML mappings
//!   - `.txt`  files  → parsed as `key=value` per line
//!   - executable files → run, stdout parsed as YAML then key=value fallback

use crate::facts::value::FactValue;
use indexmap::IndexMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Default system + user facts.d search directories (POSIX order).
pub fn default_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/puppetlabs/facter/facts.d"),
        PathBuf::from("/etc/facter/facts.d"),
        PathBuf::from("/etc/puppetlabs/facter/facts.d"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".facter/facts.d"));
        dirs.push(
            PathBuf::from(std::env::var_os("HOME").unwrap())
                .join(".puppetlabs/opt/facter/facts.d"),
        );
    }
    dirs
}

/// Load external facts from all given directories. Silently skips missing dirs.
pub fn load(dirs: &[PathBuf]) -> Vec<(String, FactValue)> {
    let mut facts = Vec::new();
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        paths.sort();

        for path in paths {
            facts.extend(load_file(&path));
        }
    }
    facts
}

fn load_file(path: &Path) -> Vec<(String, FactValue)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "json" => load_json(path),
        "yaml" | "yml" => load_yaml(path),
        "txt" => load_text(path),
        _ => {
            if is_executable(path) {
                load_executable(path)
            } else {
                vec![]
            }
        }
    }
}

// ── Custom (Ruby) dirs ────────────────────────────────────────────────────────

/// Load custom facts from explicitly-specified directories.
/// Unlike `load()`, these dirs are scanned directly (not a `facts.d` subdir)
/// and `.rb` files are executed via a Ruby shim.
/// `trace` enables full Ruby backtraces on errors.
pub fn load_custom(dirs: &[PathBuf], trace: bool) -> Vec<(String, FactValue)> {
    let mut facts = Vec::new();
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        paths.sort();
        for path in paths {
            facts.extend(load_file_custom(&path, trace));
        }
    }
    facts
}

fn load_file_custom(path: &Path, trace: bool) -> Vec<(String, FactValue)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "rb" => load_ruby(path, trace),
        _ => load_file(path),
    }
}

// ── JSON ──────────────────────────────────────────────────────────────────────

fn load_json(path: &Path) -> Vec<(String, FactValue)> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("external facts: cannot read {:?}: {}", path, e);
            return vec![];
        }
    };
    let v: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("external facts: JSON parse error in {:?}: {}", path, e);
            return vec![];
        }
    };
    let serde_json::Value::Object(map) = v else {
        log::warn!("external facts: {:?} root is not a JSON object", path);
        return vec![];
    };
    map.into_iter()
        .map(|(k, v)| (k.to_lowercase(), json_to_fact(&v)))
        .collect()
}

fn json_to_fact(v: &serde_json::Value) -> FactValue {
    match v {
        serde_json::Value::Null => FactValue::String(String::new()),
        serde_json::Value::Bool(b) => FactValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { FactValue::Integer(i) }
            else { FactValue::Float(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s) => FactValue::String(s.clone()),
        serde_json::Value::Array(arr) => FactValue::Array(arr.iter().map(json_to_fact).collect()),
        serde_json::Value::Object(obj) => {
            let map: IndexMap<String, FactValue> =
                obj.iter().map(|(k, v)| (k.clone(), json_to_fact(v))).collect();
            FactValue::Map(map)
        }
    }
}

// ── YAML ──────────────────────────────────────────────────────────────────────

fn load_yaml(path: &Path) -> Vec<(String, FactValue)> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("external facts: cannot read {:?}: {}", path, e);
            return vec![];
        }
    };
    let v: serde_yaml::Value = match serde_yaml::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("external facts: YAML parse error in {:?}: {}", path, e);
            return vec![];
        }
    };
    let serde_yaml::Value::Mapping(map) = v else {
        return vec![];
    };
    map.into_iter()
        .filter_map(|(k, v)| {
            let key = match &k {
                serde_yaml::Value::String(s) => s.to_lowercase(),
                _ => return None,
            };
            Some((key, yaml_to_fact(&v)))
        })
        .collect()
}

fn yaml_to_fact(v: &serde_yaml::Value) -> FactValue {
    match v {
        serde_yaml::Value::Null => FactValue::String(String::new()),
        serde_yaml::Value::Bool(b) => FactValue::Boolean(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() { FactValue::Integer(i) }
            else { FactValue::Float(n.as_f64().unwrap_or(0.0)) }
        }
        serde_yaml::Value::String(s) => FactValue::String(s.clone()),
        serde_yaml::Value::Sequence(arr) => {
            FactValue::Array(arr.iter().map(yaml_to_fact).collect())
        }
        serde_yaml::Value::Mapping(obj) => {
            let map: IndexMap<String, FactValue> = obj
                .iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s.clone(),
                        _ => return None,
                    };
                    Some((key, yaml_to_fact(v)))
                })
                .collect();
            FactValue::Map(map)
        }
        serde_yaml::Value::Tagged(t) => yaml_to_fact(&t.value),
    }
}

// ── Text (key=value) ──────────────────────────────────────────────────────────

fn load_text(path: &Path) -> Vec<(String, FactValue)> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("external facts: cannot read {:?}: {}", path, e);
            return vec![];
        }
    };
    parse_kv(&content)
}

fn parse_kv(text: &str) -> Vec<(String, FactValue)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_lowercase(), FactValue::String(value.trim().to_string())))
        })
        .collect()
}

// ── Executable ────────────────────────────────────────────────────────────────

fn load_executable(path: &Path) -> Vec<(String, FactValue)> {
    // Guard against recursive facter calls
    if std::env::var("INSIDE_FACTER").as_deref() == Ok("true") {
        log::debug!("skipping external executable {:?}: INSIDE_FACTER is set", path);
        return vec![];
    }

    let out = match Command::new(path).env("INSIDE_FACTER", "true").output() {
        Ok(o) => o,
        Err(e) => {
            log::warn!("external facts: failed to execute {:?}: {}", path, e);
            return vec![];
        }
    };

    if !out.status.success() {
        log::warn!(
            "external facts: {:?} exited with status {}",
            path,
            out.status
        );
    }
    if !out.stderr.is_empty() {
        log::warn!(
            "external facts: {:?} stderr: {}",
            path,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();

    // Try YAML first, fall back to key=value
    if let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(&stdout) {
        if matches!(v, serde_yaml::Value::Mapping(_)) {
            return load_yaml_from_str(&stdout);
        }
    }
    parse_kv(&stdout)
}

fn load_yaml_from_str(text: &str) -> Vec<(String, FactValue)> {
    let v: serde_yaml::Value = match serde_yaml::from_str(text) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let serde_yaml::Value::Mapping(map) = v else { return vec![]; };
    map.into_iter()
        .filter_map(|(k, v)| {
            let key = match &k {
                serde_yaml::Value::String(s) => s.to_lowercase(),
                _ => return None,
            };
            Some((key, yaml_to_fact(&v)))
        })
        .collect()
}

// ── Ruby (.rb) custom facts ───────────────────────────────────────────────────

/// Minimal Facter DSL shim written to a temp file and executed via `ruby`.
const RUBY_SHIM: &str = r#"
require 'json'
module Facter
  @@facts = {}
  class Fact
    def setcode(cmd = nil, &block)
      @val = block ? block.call : `#{cmd}`.chomp
    end
    def confine(*); end
    def weight(*); end
    def has_weight(*); end
    def on_flush(*); end
    def val; @val; end
  end
  def self.add(name, opts = {}, &block)
    f = Fact.new
    f.instance_eval(&block) if block
    @@facts[name.to_s] = f.val
  end
  def self.value(n); @@facts[n.to_s]; end
  def self.[]=(n, v); @@facts[n.to_s] = v; end
  def self.facts; @@facts; end
end
trace = ENV['FACTER_TRACE'] == '1'
ARGV.each do |f|
  if trace
    load f
  else
    begin; load f; rescue => e; $stderr.puts "facter: #{f}: #{e}"; end
  end
end
puts JSON.generate(Facter.facts)
"#;

fn load_ruby(path: &Path, trace: bool) -> Vec<(String, FactValue)> {
    // Write shim to a per-process temp file (avoid repeated writes if called multiple times)
    let shim_path = std::env::temp_dir().join(format!(".facter_rb_shim_{}.rb", std::process::id()));
    if let Err(e) = fs::write(&shim_path, RUBY_SHIM) {
        log::warn!("custom facts: cannot write Ruby shim: {}", e);
        return vec![];
    }

    let mut cmd = Command::new("ruby");
    cmd.arg(&shim_path).arg(path).env("INSIDE_FACTER", "true");
    if trace {
        cmd.env("FACTER_TRACE", "1");
    }
    let out = cmd.output();

    let _ = fs::remove_file(&shim_path);

    let out = match out {
        Ok(o) => o,
        Err(e) => {
            log::warn!("custom facts: failed to run ruby for {:?}: {}", path, e);
            return vec![];
        }
    };

    if !out.stderr.is_empty() {
        log::warn!(
            "custom facts: {:?} stderr: {}",
            path,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let map: serde_json::Map<String, serde_json::Value> =
        match serde_json::from_str(stdout.trim()) {
            Ok(serde_json::Value::Object(m)) => m,
            _ => {
                log::warn!("custom facts: {:?} did not produce a JSON object", path);
                return vec![];
            }
        };
    map.into_iter()
        .map(|(k, v)| (k.to_lowercase(), json_to_fact(&v)))
        .collect()
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

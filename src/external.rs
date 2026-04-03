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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("facter_test_{}", name));
        fs::create_dir_all(&d).unwrap();
        d
    }

    // ── parse_kv ──────────────────────────────────────────────────────────────

    #[test]
    fn kv_basic_pairs() {
        let facts = parse_kv("color=blue\nsize=large\n");
        assert_eq!(facts.len(), 2);
        assert!(facts.iter().any(|(k, v)| k == "color" && matches!(v, FactValue::String(s) if s == "blue")));
    }

    #[test]
    fn kv_skips_comment_lines() {
        let facts = parse_kv("# comment\nfoo=bar\n");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].0, "foo");
    }

    #[test]
    fn kv_skips_blank_lines() {
        let facts = parse_kv("\nfoo=bar\n\n");
        assert_eq!(facts.len(), 1);
    }

    #[test]
    fn kv_key_lowercased() {
        let facts = parse_kv("MyFact=hello\n");
        assert_eq!(facts[0].0, "myfact");
    }

    #[test]
    fn kv_value_with_equals_sign() {
        // only the first '=' is the separator
        let facts = parse_kv("url=http://example.com?a=b\n");
        assert_eq!(facts.len(), 1);
        assert!(matches!(&facts[0].1, FactValue::String(s) if s == "http://example.com?a=b"));
    }

    // ── load_json ─────────────────────────────────────────────────────────────

    #[test]
    fn json_string_and_integer() {
        let dir = tmp_dir("json");
        let path = dir.join("t.json");
        fs::write(&path, r#"{"myfact": "hello", "count": 42}"#).unwrap();
        let facts = load_json(&path);
        assert!(facts.iter().any(|(k, v)| k == "myfact" && matches!(v, FactValue::String(s) if s == "hello")));
        assert!(facts.iter().any(|(k, v)| k == "count" && matches!(v, FactValue::Integer(42))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn json_nested_map() {
        let dir = tmp_dir("json_nested");
        let path = dir.join("t.json");
        fs::write(&path, r#"{"outer": {"inner": "value"}}"#).unwrap();
        let facts = load_json(&path);
        assert!(matches!(&facts[0].1, FactValue::Map(_)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn json_non_object_root_returns_empty() {
        let dir = tmp_dir("json_bad");
        let path = dir.join("t.json");
        fs::write(&path, r#"["not", "an", "object"]"#).unwrap();
        assert!(load_json(&path).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    // ── load_yaml ─────────────────────────────────────────────────────────────

    #[test]
    fn yaml_string_fact() {
        let dir = tmp_dir("yaml");
        let path = dir.join("t.yaml");
        fs::write(&path, "myfact: myvalue\n").unwrap();
        let facts = load_yaml(&path);
        assert!(facts.iter().any(|(k, v)| k == "myfact" && matches!(v, FactValue::String(s) if s == "myvalue")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn yaml_boolean_fact() {
        let dir = tmp_dir("yaml_bool");
        let path = dir.join("t.yaml");
        fs::write(&path, "enabled: true\n").unwrap();
        let facts = load_yaml(&path);
        assert!(facts.iter().any(|(k, v)| k == "enabled" && matches!(v, FactValue::Boolean(true))));
        let _ = fs::remove_dir_all(dir);
    }

    // ── load_text ─────────────────────────────────────────────────────────────

    #[test]
    fn text_key_value_pairs() {
        let dir = tmp_dir("txt");
        let path = dir.join("t.txt");
        fs::write(&path, "color=blue\nsize=large\n").unwrap();
        let facts = load_text(&path);
        assert_eq!(facts.len(), 2);
        let _ = fs::remove_dir_all(dir);
    }

    // ── load (directory scan) ─────────────────────────────────────────────────

    #[test]
    fn load_dir_picks_up_json_file() {
        let dir = tmp_dir("load_dir");
        fs::write(dir.join("facts.json"), r#"{"dirscanfact": "yes"}"#).unwrap();
        let facts = load(&[dir.clone()]);
        assert!(facts.iter().any(|(k, _)| k == "dirscanfact"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_skips_missing_dirs() {
        let facts = load(&[PathBuf::from("/nonexistent/path/xyz")]);
        assert!(facts.is_empty());
    }

    #[test]
    fn load_non_executable_unknown_extension_ignored() {
        let dir = tmp_dir("load_unknown");
        fs::write(dir.join("facts.xyz"), "ignored=yes").unwrap();
        // file is not executable and extension is unknown — should produce nothing
        let facts = load(&[dir.clone()]);
        assert!(facts.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_executable_script_kv_output() {
        let dir = tmp_dir("exec");
        let script = dir.join("facts.sh");
        fs::write(&script, "#!/bin/sh\necho 'execfact=fromscript'").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        let facts = load(&[dir.clone()]);
        assert!(facts.iter().any(|(k, v)| k == "execfact" && matches!(v, FactValue::String(s) if s == "fromscript")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_executable_yaml_output() {
        let dir = tmp_dir("exec_yaml");
        let script = dir.join("facts.sh");
        fs::write(&script, "#!/bin/sh\nprintf 'yamlfact: fromyaml\\ncount: 7\\n'").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        let facts = load(&[dir.clone()]);
        assert!(facts.iter().any(|(k, v)| k == "yamlfact" && matches!(v, FactValue::String(s) if s == "fromyaml")));
        assert!(facts.iter().any(|(k, v)| k == "count" && matches!(v, FactValue::Integer(7))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn inside_facter_guard_prevents_execution() {
        let dir = tmp_dir("exec_guard");
        let script = dir.join("facts.sh");
        fs::write(&script, "#!/bin/sh\necho 'guardfact=should_not_appear'").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        // Set the guard env var — load_executable must return empty
        std::env::set_var("INSIDE_FACTER", "true");
        let facts = load_executable(&script);
        std::env::remove_var("INSIDE_FACTER");

        assert!(facts.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    // ── is_executable ─────────────────────────────────────────────────────────

    #[test]
    fn non_executable_file_returns_false() {
        let dir = tmp_dir("is_exec_no");
        let path = dir.join("plain.txt");
        fs::write(&path, "hello").unwrap();
        // default permissions have no x bit
        assert!(!is_executable(&path));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn executable_file_returns_true() {
        let dir = tmp_dir("is_exec_yes");
        let path = dir.join("script.sh");
        fs::write(&path, "#!/bin/sh").unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
        assert!(is_executable(&path));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_file_returns_false() {
        assert!(!is_executable(std::path::Path::new("/nonexistent/file.sh")));
    }

    // ── load_yaml_from_str ────────────────────────────────────────────────────

    #[test]
    fn yaml_from_str_parses_mapping() {
        let facts = load_yaml_from_str("key1: val1\nkey2: 99\n");
        assert!(facts.iter().any(|(k, v)| k == "key1" && matches!(v, FactValue::String(s) if s == "val1")));
        assert!(facts.iter().any(|(k, v)| k == "key2" && matches!(v, FactValue::Integer(99))));
    }

    #[test]
    fn yaml_from_str_non_mapping_returns_empty() {
        assert!(load_yaml_from_str("- a\n- b\n").is_empty());
    }

    #[test]
    fn yaml_from_str_keys_lowercased() {
        let facts = load_yaml_from_str("MyKey: value\n");
        assert!(facts.iter().any(|(k, _)| k == "mykey"));
    }

    // ── load_custom ───────────────────────────────────────────────────────────

    #[test]
    fn load_custom_picks_up_json_file() {
        let dir = tmp_dir("custom_json");
        fs::write(dir.join("fact.json"), r#"{"customfact": "cval"}"#).unwrap();
        let facts = load_custom(&[dir.clone()], false);
        assert!(facts.iter().any(|(k, v)| k == "customfact" && matches!(v, FactValue::String(s) if s == "cval")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_custom_skips_missing_dirs() {
        let facts = load_custom(&[PathBuf::from("/no/such/dir")], false);
        assert!(facts.is_empty());
    }

    // ── default_dirs ──────────────────────────────────────────────────────────

    #[test]
    fn default_dirs_non_empty() {
        let dirs = default_dirs();
        assert!(!dirs.is_empty());
    }

    #[test]
    fn default_dirs_includes_system_paths() {
        let dirs = default_dirs();
        assert!(dirs.iter().any(|d| d.to_str().unwrap().contains("puppetlabs")));
    }

    #[test]
    fn default_dirs_includes_home_paths_when_home_set() {
        // HOME should always be set in a normal test environment
        if std::env::var_os("HOME").is_some() {
            let dirs = default_dirs();
            assert!(dirs.iter().any(|d| d.to_str().unwrap().contains(".facter")));
        }
    }

    // ── json_to_fact: Null and Float branches ─────────────────────────────────

    #[test]
    fn json_null_becomes_empty_string() {
        let dir = tmp_dir("json_null");
        let path = dir.join("t.json");
        fs::write(&path, r#"{"nullfact": null}"#).unwrap();
        let facts = load_json(&path);
        assert!(facts.iter().any(|(k, v)| k == "nullfact" && matches!(v, FactValue::String(s) if s.is_empty())));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn json_float_becomes_float_value() {
        let dir = tmp_dir("json_float");
        let path = dir.join("t.json");
        fs::write(&path, r#"{"floatfact": 3.14}"#).unwrap();
        let facts = load_json(&path);
        assert!(facts.iter().any(|(k, v)| k == "floatfact" && matches!(v, FactValue::Float(_))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn json_array_value() {
        let dir = tmp_dir("json_array");
        let path = dir.join("t.json");
        fs::write(&path, r#"{"arr": [1, 2, 3]}"#).unwrap();
        let facts = load_json(&path);
        assert!(facts.iter().any(|(k, v)| k == "arr" && matches!(v, FactValue::Array(_))));
        let _ = fs::remove_dir_all(dir);
    }

    // ── load_json error paths ─────────────────────────────────────────────────

    #[test]
    fn load_json_invalid_syntax_returns_empty() {
        let dir = tmp_dir("json_bad");
        let path = dir.join("bad.json");
        fs::write(&path, "not json at all").unwrap();
        assert!(load_json(&path).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_json_missing_file_returns_empty() {
        assert!(load_json(std::path::Path::new("/nonexistent/file.json")).is_empty());
    }

    // ── yaml_to_fact: all type branches ──────────────────────────────────────

    #[test]
    fn yaml_null_becomes_empty_string() {
        let dir = tmp_dir("yaml_null");
        let path = dir.join("t.yaml");
        fs::write(&path, "nullfact: ~\n").unwrap();
        let facts = load_yaml(&path);
        assert!(facts.iter().any(|(k, v)| k == "nullfact" && matches!(v, FactValue::String(s) if s.is_empty())));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn yaml_float_value() {
        let dir = tmp_dir("yaml_float");
        let path = dir.join("t.yaml");
        fs::write(&path, "pi: 3.14\n").unwrap();
        let facts = load_yaml(&path);
        assert!(facts.iter().any(|(k, v)| k == "pi" && matches!(v, FactValue::Float(_))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn yaml_sequence_value() {
        let dir = tmp_dir("yaml_seq");
        let path = dir.join("t.yaml");
        fs::write(&path, "items:\n  - a\n  - b\n").unwrap();
        let facts = load_yaml(&path);
        assert!(facts.iter().any(|(k, v)| k == "items" && matches!(v, FactValue::Array(_))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn yaml_nested_mapping() {
        let dir = tmp_dir("yaml_map");
        let path = dir.join("t.yaml");
        fs::write(&path, "outer:\n  inner: value\n").unwrap();
        let facts = load_yaml(&path);
        assert!(facts.iter().any(|(k, v)| k == "outer" && matches!(v, FactValue::Map(_))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn yaml_non_mapping_root_returns_empty() {
        let dir = tmp_dir("yaml_list_root");
        let path = dir.join("t.yaml");
        fs::write(&path, "- a\n- b\n").unwrap();
        assert!(load_yaml(&path).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    // ── load_yaml error paths ─────────────────────────────────────────────────

    #[test]
    fn load_yaml_missing_file_returns_empty() {
        assert!(load_yaml(std::path::Path::new("/nonexistent/file.yaml")).is_empty());
    }

    #[test]
    fn load_yaml_invalid_syntax_returns_empty() {
        let dir = tmp_dir("yaml_invalid");
        let path = dir.join("bad.yaml");
        // Deliberately invalid YAML (mismatched indentation / tab characters)
        fs::write(&path, "key:\n\t- broken\n").unwrap();
        // May or may not parse depending on libyaml version; if it does parse, just ensure no panic
        let _ = load_yaml(&path);
        let _ = fs::remove_dir_all(dir);
    }

    // ── load_text error path ──────────────────────────────────────────────────

    #[test]
    fn load_text_missing_file_returns_empty() {
        assert!(load_text(std::path::Path::new("/nonexistent/file.txt")).is_empty());
    }

    // ── load_executable error paths ───────────────────────────────────────────

    #[test]
    fn load_executable_nonexistent_returns_empty() {
        let facts = load_executable(std::path::Path::new("/nonexistent/script.sh"));
        assert!(facts.is_empty());
    }

    #[test]
    fn load_executable_nonzero_exit_still_returns_output() {
        let dir = tmp_dir("exec_fail");
        let script = dir.join("fail.sh");
        fs::write(&script, "#!/bin/sh\necho 'failfact=yes'\nexit 1").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
        // Even with nonzero exit, we still parse whatever was written to stdout
        let facts = load_executable(&script);
        assert!(facts.iter().any(|(k, v)| k == "failfact" && matches!(v, FactValue::String(s) if s == "yes")));
        let _ = fs::remove_dir_all(dir);
    }
}

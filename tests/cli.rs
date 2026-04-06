use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

fn facter() -> Command {
    Command::new(env!("CARGO_BIN_EXE_facter"))
}

fn stdout(cmd: &mut Command) -> String {
    let out = cmd.output().expect("facter failed to run");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn tmp_dir(label: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("facter_cli_test_{}", label));
    fs::create_dir_all(&d).unwrap();
    d
}

// ── Basic output ──────────────────────────────────────────────────────────────

#[test]
fn kernel_returns_linux() {
    let out = stdout(facter().arg("kernel"));
    assert_eq!(out.trim(), "Linux");
}

#[test]
fn single_query_no_key_prefix() {
    // A single query prints only the value, no "key =>" prefix
    let out = stdout(facter().arg("kernel"));
    assert!(!out.contains("=>"));
}

#[test]
fn multiple_queries_show_key_prefix() {
    let out = stdout(facter().args(["kernel", "os.name"]));
    assert!(out.contains("kernel =>"));
    assert!(out.contains("os.name =>"));
}

#[test]
fn dotted_query_returns_scalar() {
    let out = stdout(facter().arg("os.name"));
    assert!(!out.trim().is_empty());
    assert!(!out.contains("=>"));
}

// ── JSON output ───────────────────────────────────────────────────────────────

#[test]
fn json_single_query_is_bare_value() {
    let out = stdout(facter().args(["--json", "kernel"]));
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v.as_str().unwrap(), "Linux");
}

#[test]
fn json_multiple_queries_is_object() {
    let out = stdout(facter().args(["--json", "kernel", "os.name"]));
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert!(v.is_object());
    assert!(v.get("kernel").is_some());
    assert!(v.get("os.name").is_some());
}

#[test]
fn json_missing_query_returns_null() {
    let out = stdout(facter().args(["--json", "nonexistentfact_xyz"]));
    assert_eq!(out.trim(), "null");
}

// ── YAML output ───────────────────────────────────────────────────────────────

#[test]
fn yaml_output_contains_value() {
    let out = stdout(facter().args(["--yaml", "kernel"]));
    assert!(out.contains("Linux"));
}

// ── Version and help ──────────────────────────────────────────────────────────

#[test]
fn version_flag() {
    let out = stdout(facter().arg("--version"));
    assert!(out.contains("3.14.25"));
}

#[test]
fn list_block_groups_includes_resolvers() {
    let out = stdout(facter().arg("--list-block-groups"));
    assert!(out.contains("kernel"));
    assert!(out.contains("networking"));
    assert!(out.contains("ssh"));
}

// ── --exclude ─────────────────────────────────────────────────────────────────

#[test]
fn exclude_top_level_fact() {
    let out = stdout(facter().args(["--json", "--exclude", "kernel", "kernel"]));
    assert_eq!(out.trim(), "null");
}

#[test]
fn exclude_nested_key_leaves_parent() {
    let out = stdout(facter().args(["--json", "--exclude", "os.release", "os"]));
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert!(v.get("release").is_none(), "os.release should be excluded");
    assert!(v.get("name").is_some(), "os.name should still be present");
}

#[test]
fn exclude_repeated_flag() {
    let out = stdout(facter().args(["--json", "--exclude", "kernel", "--exclude", "os", "kernel", "os.name"]));
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert!(v.get("kernel").is_none());
    assert!(v.get("os.name").is_none());
}

// ── --show-legacy ─────────────────────────────────────────────────────────────

#[test]
fn legacy_fact_hidden_by_default() {
    let out = stdout(facter().args(["--json", "operatingsystem"]));
    assert_eq!(out.trim(), "null");
}

#[test]
fn legacy_fact_visible_with_show_legacy() {
    let out = stdout(facter().args(["--show-legacy", "--json", "operatingsystem"]));
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert!(v.is_string(), "operatingsystem should be a string");
}

// ── FACTER_* env overrides ────────────────────────────────────────────────────

#[test]
fn facter_env_override_replaces_fact() {
    let out = stdout(facter().arg("kernel").env("FACTER_kernel", "mykernel"));
    assert_eq!(out.trim(), "mykernel");
}

#[test]
fn facter_env_creates_new_fact() {
    let out = stdout(facter().args(["--json", "custom_env_fact_xyz"]).env("FACTER_custom_env_fact_xyz", "hello"));
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v.as_str().unwrap(), "hello");
}

// ── --strict ─────────────────────────────────────────────────────────────────

#[test]
fn strict_mode_exits_nonzero_for_unknown_fact() {
    let status = facter()
        .args(["--strict", "nonexistentfact_xyz_abc"])
        .status()
        .unwrap();
    assert!(!status.success());
}

#[test]
fn strict_mode_succeeds_for_known_fact() {
    let status = facter().args(["--strict", "kernel"]).status().unwrap();
    assert!(status.success());
}

// ── --external-dir ────────────────────────────────────────────────────────────

#[test]
fn external_dir_loads_json_fact() {
    let dir = tmp_dir("ext_json");
    fs::write(dir.join("test.json"), r#"{"ext_json_fact_xyz": "jsonvalue"}"#).unwrap();

    let out = stdout(facter().args(["--external-dir", dir.to_str().unwrap(), "ext_json_fact_xyz"]));
    assert_eq!(out.trim(), "jsonvalue");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn external_dir_loads_yaml_fact() {
    let dir = tmp_dir("ext_yaml");
    fs::write(dir.join("test.yaml"), "ext_yaml_fact_xyz: yamlvalue\n").unwrap();

    let out = stdout(facter().args(["--external-dir", dir.to_str().unwrap(), "ext_yaml_fact_xyz"]));
    assert_eq!(out.trim(), "yamlvalue");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn external_dir_loads_text_fact() {
    let dir = tmp_dir("ext_txt");
    fs::write(dir.join("test.txt"), "ext_txt_fact_xyz=txtvalue\n").unwrap();

    let out = stdout(facter().args(["--external-dir", dir.to_str().unwrap(), "ext_txt_fact_xyz"]));
    assert_eq!(out.trim(), "txtvalue");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn external_dir_loads_executable_fact() {
    let dir = tmp_dir("ext_exec");
    let script = dir.join("facts.sh");
    fs::write(&script, "#!/bin/sh\necho 'ext_exec_fact_xyz=execvalue'").unwrap();
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();

    let out = stdout(facter().args(["--external-dir", dir.to_str().unwrap(), "ext_exec_fact_xyz"]));
    assert_eq!(out.trim(), "execvalue");
    let _ = fs::remove_dir_all(dir);
}

// ── --block ───────────────────────────────────────────────────────────────────

#[test]
fn block_prevents_resolver_from_running() {
    // Block the kernel resolver — kernel fact should disappear
    let out = stdout(facter().args(["--json", "--block", "kernel", "kernel"]));
    assert_eq!(out.trim(), "null");
}

// ── output ordering ───────────────────────────────────────────────────────────

#[test]
fn all_facts_output_is_sorted() {
    let out = stdout(facter().args(["--json"]));
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "all-facts JSON output should be alphabetically sorted");
}

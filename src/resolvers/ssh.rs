use crate::facts::value::FactValue;
use crate::resolvers::Resolver;
use base64::{engine::general_purpose::STANDARD, Engine};
use hex;
use indexmap::IndexMap;
use sha1::{Digest as _, Sha1};
use sha2::Sha256;
use std::fs;
use std::path::Path;

pub struct SshResolver;

/// Search directories for SSH host public key files (POSIX order).
const SEARCH_DIRS: &[&str] = &[
    "/etc/ssh",
    "/usr/local/etc/ssh",
    "/etc",
    "/usr/local/etc",
    "/etc/opt/ssh",
];

/// (file_basename, algo_name, SSHFP_type_code)
const KEY_FILES: &[(&str, &str, u8)] = &[
    ("ssh_host_rsa_key.pub",     "ssh-rsa",     1),
    ("ssh_host_dsa_key.pub",     "ssh-dss",     2),
    ("ssh_host_ecdsa_key.pub",   "ecdsa-sha2",  3),
    ("ssh_host_ed25519_key.pub", "ssh-ed25519", 4),
];

impl Resolver for SshResolver {
    fn name(&self) -> &'static str {
        "ssh"
    }

    fn resolve(&self) -> Vec<(String, FactValue)> {
        let mut facts = Vec::new();
        let mut ssh_map: IndexMap<String, FactValue> = IndexMap::new();

        for (filename, _algo, type_code) in KEY_FILES {
            let Some(path) = find_key_file(filename) else {
                continue;
            };
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };

            let Some(key) = parse_pub_key(&content) else {
                continue;
            };

            let (sha1_fp, sha256_fp) = compute_fingerprints(&key.raw_bytes, *type_code);
            let key_name = key_map_name(*type_code);

            // Legacy flat facts
            facts.push((legacy_key_fact(*type_code), FactValue::String(key.base64.clone())));
            if !sha1_fp.is_empty() {
                facts.push((legacy_fp_fact(*type_code), FactValue::String(sha1_fp.clone())));
            }

            // Structured entry
            let mut entry: IndexMap<String, FactValue> = IndexMap::new();
            entry.insert("key".into(), FactValue::String(key.base64));
            entry.insert("type".into(), FactValue::String(key.algo));

            let mut fingerprints: IndexMap<String, FactValue> = IndexMap::new();
            if !sha1_fp.is_empty() {
                fingerprints.insert("sha1".into(), FactValue::String(sha1_fp));
            }
            if !sha256_fp.is_empty() {
                fingerprints.insert("sha256".into(), FactValue::String(sha256_fp));
            }
            if !fingerprints.is_empty() {
                entry.insert("fingerprints".into(), FactValue::Map(fingerprints));
            }

            ssh_map.insert(key_name.into(), FactValue::Map(entry));
        }

        if !ssh_map.is_empty() {
            facts.push(("ssh".into(), FactValue::Map(ssh_map)));
        }
        facts
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

struct PubKey {
    algo: String,
    base64: String,
    raw_bytes: Vec<u8>,
}

fn find_key_file(filename: &str) -> Option<String> {
    for dir in SEARCH_DIRS {
        let p = format!("{}/{}", dir, filename);
        if Path::new(&p).is_file() {
            return Some(p);
        }
    }
    None
}

fn parse_pub_key(content: &str) -> Option<PubKey> {
    let line = content.lines().find(|l| !l.starts_with('#'))?;
    let mut parts = line.split_whitespace();
    let algo = parts.next()?.to_string();
    let b64 = parts.next()?.to_string();
    let raw_bytes = STANDARD.decode(&b64).ok()?;
    Some(PubKey { algo, base64: b64, raw_bytes })
}

fn compute_fingerprints(raw: &[u8], type_code: u8) -> (String, String) {
    let sha1_hex = hex::encode(Sha1::digest(raw));
    let sha256_hex = hex::encode(Sha256::digest(raw));
    let sha1_fp = format!("SSHFP {} 1 {}", type_code, sha1_hex);
    let sha256_fp = format!("SSHFP {} 2 {}", type_code, sha256_hex);
    (sha1_fp, sha256_fp)
}

fn key_map_name(type_code: u8) -> &'static str {
    match type_code {
        1 => "rsa",
        2 => "dsa",
        3 => "ecdsa",
        4 => "ed25519",
        _ => "unknown",
    }
}

fn legacy_key_fact(type_code: u8) -> String {
    match type_code {
        1 => "sshrsakey",
        2 => "sshdsakey",
        3 => "sshecdsakey",
        4 => "sshed25519key",
        _ => "sshkey",
    }
    .into()
}

fn legacy_fp_fact(type_code: u8) -> String {
    match type_code {
        1 => "sshfp_rsa",
        2 => "sshfp_dsa",
        3 => "sshfp_ecdsa",
        4 => "sshfp_ed25519",
        _ => "sshfp",
    }
    .into()
}

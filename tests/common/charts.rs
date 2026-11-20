//! Shared helpers for the public `sidescope charts` tests. Every helper drives
//! the real binary; nothing here reaches into the library's internals.

#![allow(dead_code)]

use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const ROOT: &str = env!("CARGO_MANIFEST_DIR");

pub fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_sidescope")
}

pub fn fixture(path: &str) -> String {
    format!("{ROOT}/tests/fixtures/charts/{path}")
}

/// A scratch directory under the target directory, emptied on creation.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("charts-cli-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    pub fn text(&self, name: &str) -> String {
        self.path(name).to_string_lossy().into_owned()
    }

    pub fn write(&self, name: &str, content: impl AsRef<[u8]>) -> String {
        std::fs::write(self.path(name), content).unwrap();
        self.text(name)
    }

    pub fn config(&self, toml: &str) -> String {
        self.write("config.toml", toml)
    }

    /// An executable `sh` script that appends its pid to `<script>.marker`
    /// before running `body`. The marker is the source-side effect witness.
    pub fn producer(&self, name: &str, body: &str) -> String {
        let path = self.path(name);
        let script = format!("#!/bin/sh\necho $$ >> \"$0.marker\"\n{body}\n");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    pub fn marker_lines(&self, name: &str) -> usize {
        std::fs::read_to_string(format!("{}.marker", self.path(name).display()))
            .map_or(0, |text| text.lines().count())
    }

    pub fn exists(&self, name: &str) -> bool {
        self.path(name).exists()
    }
}

/// A producer body printing one file.
pub fn cat(file: &str) -> String {
    format!("cat '{file}'")
}

pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub json: Value,
}

impl Out {
    pub fn error_ids(&self) -> Vec<String> {
        self.json["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap().to_string())
            .collect()
    }

    pub fn card(&self, id: &str) -> &Value {
        self.json["cards"]
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["id"] == id)
            .unwrap_or_else(|| panic!("card {id} in {}", self.stdout))
    }

    pub fn source(&self, id: &str) -> &Value {
        self.json["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["id"] == id)
            .unwrap_or_else(|| panic!("source {id} in {}", self.stdout))
    }

    pub fn check(&self, id: &str) -> &Value {
        self.json["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|check| check["id"] == id)
            .unwrap_or_else(|| panic!("check {id}"))
    }
}

fn command(args: &[&str], env: &[(&str, &str)]) -> Command {
    let quiet = Scratch::new("quiet-config");
    let mut command = Command::new(binary());
    command
        .args(args)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .env("XDG_CONFIG_HOME", &quiet.0);
    for (key, value) in env {
        command.env(key, value);
    }
    command
}

/// Runs the binary and requires exactly one JSON document on stdout.
pub fn run_with_env(args: &[&str], env: &[(&str, &str)]) -> Out {
    let output = command(args, env).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("stdout is not one JSON document ({error}): {stdout:?}"));
    Out {
        code: output.status.code().unwrap(),
        stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        json,
    }
}

pub fn run(args: &[&str]) -> Out {
    run_with_env(args, &[])
}

/// Runs the binary and returns raw text (for help and text format).
pub fn run_text(args: &[&str]) -> (i32, String, String) {
    let output = command(args, &[]).output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// One source configured with the native format, ready for a candidate file.
pub fn source_toml(id: &str, command: &[&str], extra: &str) -> String {
    let argv = command
        .iter()
        .map(|part| format!("{part:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "[[chart_sources]]\nid = \"{id}\"\ncommand = [{argv}]\nformat = \"sidescope-chart-v1\"\nwindow = \"2m\"\ntimezone = \"UTC\"\n{extra}\n"
    )
}

pub fn card_toml(id: &str, source: &str, kind: &str, metric: &str, extra: &str) -> String {
    format!(
        "[[chart_cards]]\nid = \"{id}\"\nsource = \"{source}\"\nkind = \"{kind}\"\nmetric = \"{metric}\"\n{extra}\n"
    )
}

/// The facts a consumer asserts: per source the dataset and error kind, per
/// card the outcome, intent, summary, intervals, groups and rows. Provenance
/// (`display`, `attempt`, `clocks`, `checks`, `data_origin`) is excluded.
pub fn semantic_facts(report: &Value) -> Value {
    let sources: Vec<Value> = report["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| {
            serde_json::json!({
                "id": source["id"],
                "dataset": source["dataset"],
                "error_kind": source["error"]["kind"],
            })
        })
        .collect();
    let cards: Vec<Value> = report["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|card| {
            serde_json::json!({
                "id": card["id"],
                "outcome": card["outcome"],
                "intent": card["intent"],
                "summary": card["summary"],
                "intervals": card["intervals"],
                "groups": card["groups"],
                "rows": card["rows"],
                "details": card["details"],
            })
        })
        .collect();
    serde_json::json!({ "sources": sources, "cards": cards })
}

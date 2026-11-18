//! Bake the source revision into the binary so a running window can name the
//! build it came from without reading a log or running git by hand.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    // Watch the commit pointer too: a commit changes no source file, but it does
    // change the revision this build should report.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/index");
    }
    let mut revision = git(&["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".into());
    // Untracked files are deliberately ignored: scratch output is not part of the build.
    if git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty()) {
        revision.push_str("-dirty");
    }
    println!("cargo:rustc-env=SIDESCOPE_REVISION={revision}");
    println!("cargo:rerun-if-env-changed=SIDESCOPE_REVISION");
}

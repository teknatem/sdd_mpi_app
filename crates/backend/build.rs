use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../../config.toml");

    // Get the output directory where the binary will be placed
    let out_dir = env::var("OUT_DIR").unwrap();
    let cargo_profile = env::var("PROFILE").unwrap(); // Cargo reports only "debug" or "release"

    // Build identity for dataset snapshot manifests: the receiving instance must
    // be able to tell which build produced a snapshot. Absence of git must never
    // fail the build — release tarballs are built outside a repository.
    let out_path = Path::new(&out_dir);
    let target_dir = out_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("Could not find target profile directory");
    let profile = target_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&cargo_profile);

    println!("cargo:rustc-env=BUILD_PROFILE={}", profile);
    println!("cargo:rustc-env=GIT_COMMIT={}", detect_git_commit());

    // Source config.toml from workspace root.
    //
    // `env::var`, NOT `env!`: the macro resolves at the time this build script is
    // compiled and freezes that absolute path into the build-script binary. The
    // frozen copy survives a move of the repository, so after renaming the project
    // directory the build failed looking for config.toml under the OLD path — and
    // only a clean rebuild of the script would have fixed it. Cargo sets this
    // variable on every run, so reading it at runtime always tracks the real location.
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let workspace_root = Path::new(&manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("Could not find workspace root");

    let source_config = workspace_root.join("config.toml");
    let dest_config = target_dir.join("config.toml");

    if !source_config.exists() {
        panic!(
            "Required config.toml not found at {:?}. Create it from config.toml.example and set an absolute [data].root.",
            source_config
        );
    }

    fs::copy(&source_config, &dest_config)
        .unwrap_or_else(|e| panic!("Failed to copy config.toml: {}", e));
}

/// Short git commit of the working tree, or `"unknown"` when git is unavailable
/// (no repository, no git binary). Never panics.
fn detect_git_commit() -> String {
    // Runtime lookup for the same reason as above: a path baked in by `env!`
    // outlives a move of the repository and then points nowhere.
    let manifest_dir = match env::var("CARGO_MANIFEST_DIR") {
        Ok(dir) => dir,
        Err(_) => return "unknown".to_string(),
    };
    let head = Path::new(&manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join(".git").join("HEAD"));
    if let Some(head) = head {
        if head.exists() {
            // Rerun when the checked-out commit changes.
            println!("cargo:rerun-if-changed={}", head.display());
        }
    }

    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&manifest_dir)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|commit| commit.trim().to_string())
        .filter(|commit| !commit.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=ETV_VERSION");
    let version = std::env::var("ETV_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| format!("{}+local", env!("CARGO_PKG_VERSION")));
    println!("cargo:rustc-env=ETV_VERSION_STRING={version}");

    // docker has no .git
    println!("cargo:rerun-if-env-changed=ETV_COMMIT");
    let commit = std::env::var("ETV_COMMIT")
        .ok()
        .filter(|c| !c.trim().is_empty())
        .or_else(git_commit);
    if let Some(commit) = commit {
        println!("cargo:rustc-env=ETV_COMMIT_STRING={commit}");
    }
}

fn git_commit() -> Option<String> {
    let commit = git(&["rev-parse", "HEAD"])?;

    // a commit moves the branch ref (loose or packed), not HEAD
    let mut watched = vec![String::from("HEAD"), String::from("packed-refs")];
    watched.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for path in watched {
        // a missing path makes cargo rerun every build
        if let Some(p) = git(&["rev-parse", "--path-format=absolute", "--git-path", &path])
            && Path::new(&p).exists()
        {
            println!("cargo:rerun-if-changed={p}");
        }
    }

    Some(commit)
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let out = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!out.is_empty()).then_some(out)
}

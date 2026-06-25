#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! ```
//!
//! GitHub environment diagnostic for autopilot-orchestrator.
//! Checks whether the current environment can support the orchestrator's GitHub mode.
//!
//! Run: ./scripts/env-check.rs

use std::process::Command;

#[derive(Debug, PartialEq)]
enum CheckResult {
    Pass(String),
    Fail(String),
}

fn check_gh_installed() -> CheckResult {
    match Command::new("gh").arg("--version").output() {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if let Some(line) = stdout.lines().next() {
                CheckResult::Pass(format!("installed ✓  ({})", line))
            } else {
                CheckResult::Pass("installed ✓".to_string())
            }
        }
        _ => CheckResult::Fail("NOT FOUND ✗\n  → install: brew install gh".to_string()),
    }
}

fn check_gh_authenticated() -> CheckResult {
    match Command::new("gh").args(["auth", "status"]).output() {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.contains("Logged in") {
                CheckResult::Pass("authenticated ✓".to_string())
            } else {
                CheckResult::Fail("NOT AUTHENTICATED ✗\n  → run: gh auth login".to_string())
            }
        }
        Err(_) => CheckResult::Fail("NOT AUTHENTICATED ✗\n  → run: gh auth login".to_string()),
    }
}

fn check_git_remote() -> CheckResult {
    match Command::new("git").args(["remote", "get-url", "origin"]).output() {
        Ok(o) if o.status.success() => {
            let remote = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if remote.is_empty() {
                CheckResult::Fail("NOT SET ✗\n  → run: git remote add origin <url>".to_string())
            } else {
                CheckResult::Pass(format!("origin → {} ✓", remote))
            }
        }
        _ => CheckResult::Fail("NOT SET ✗\n  → run: git remote add origin <url>".to_string()),
    }
}

fn main() {
    let mut all_pass = true;
    println!("=== GitHub Environment Check ===");

    let checks: [(&str, fn() -> CheckResult); 3] = [
        ("gh CLI:", check_gh_installed),
        ("gh auth:", check_gh_authenticated),
        ("git remote:", check_git_remote),
    ];

    for (label, check_fn) in &checks {
        match check_fn() {
            CheckResult::Pass(msg) => println!("{:<16}{}", label, msg),
            CheckResult::Fail(msg) => {
                println!("{:<16}{}", label, msg);
                all_pass = false;
            }
        }
    }

    println!();
    if all_pass {
        println!("All checks passed.");
    } else {
        println!("Some checks failed. See above for fixes.");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_gh_installed_returns_result() {
        let result = check_gh_installed();
        match result {
            CheckResult::Pass(_) | CheckResult::Fail(_) => {} // either is valid depending on env
        }
    }

    #[test]
    fn test_check_gh_authenticated_returns_result() {
        let result = check_gh_authenticated();
        match result {
            CheckResult::Pass(_) | CheckResult::Fail(_) => {}
        }
    }

    #[test]
    fn test_check_git_remote_returns_result() {
        let result = check_git_remote();
        match result {
            CheckResult::Pass(_) | CheckResult::Fail(_) => {}
        }
    }

    #[test]
    fn test_check_result_enum_variants() {
        let pass = CheckResult::Pass("ok".to_string());
        let fail = CheckResult::Fail("err".to_string());
        assert_ne!(pass, fail);
        match &pass {
            CheckResult::Pass(msg) => assert_eq!(msg, "ok"),
            _ => panic!("expected Pass"),
        }
        match &fail {
            CheckResult::Fail(msg) => assert_eq!(msg, "err"),
            _ => panic!("expected Fail"),
        }
    }
}

# Distill ships as a precompiled Rust CLI

Distill's state machine, persistence, locking, and contract validation live in a Rust workspace crate behind a thin CLI. Releases ship precompiled macOS and Linux executables selected by the installer, and the Codex, Reasonix, and Kimi skill variants call the same installed binary; end users do not need Rust or `rust-script`. Windows remains outside the toolkit's current bash-based platform boundary.

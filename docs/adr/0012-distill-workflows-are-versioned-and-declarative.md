# Distill workflows are versioned and declarative

The Distill runner reads an ordered declarative workflow definition instead of hard-coding the current four executor names. Each run snapshots the exact definition and version it starts with, so executor substitutions and later interaction improvements apply only to new runs while interrupted runs remain reproducible and resumable. The runner enforces stage order and completion contracts independently of which executor is assigned to a stage.

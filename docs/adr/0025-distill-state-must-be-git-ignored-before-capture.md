# Distill state must be Git-ignored before capture

Before writing any project-local run data, `distill start` ensures the root `.gitignore` contains an effective `/.distill/` rule, appending the exact rule when necessary and reporting the working-tree change without committing it. If the runner cannot establish that protection safely, it fails closed before capturing requirement content so potentially sensitive snapshots never appear as trackable project files.

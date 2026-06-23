# install.sh must not silently delete real directories

`install.sh` previously had logic to remove a real directory (non-symlink) at `~/.agents/skills/<name>` and replace it with a symlink. This is destructive — the real directory could contain a skill installed by another plugin or manually by the user.

We decided install.sh must skip (with a warning) any entry where a real directory exists at the target path, rather than deleting it. The selfcheck reports such entries as FAIL so the user can resolve them deliberately.

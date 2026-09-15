//! Command-line parsing for the `director` binary.

use std::path::PathBuf;

pub(crate) const USAGE: &str = "\
Usage:
  director init --worktree <path> --spec-issue <n> --slug <slug>
  director inspect --worktree <path>
  director ticket add --worktree <path> --ticket <n> --title <title> [--blocked-by <n>]...
  director ticket transition --worktree <path> --ticket <n> --to <status>
  director run transition --worktree <path> --to <status>
  director round open --worktree <path> (--ticket <n> | --spec)
  director round close --worktree <path> (--ticket <n> | --spec) --round <k>
  director finding record --worktree <path> (--ticket <n> | --spec) --round <k>
      --axis <standards|spec> --id <id> --hash <hash> --summary <text>
      [--fixed <commit> | --rejected <reason>]
  director finding dispose --worktree <path> (--ticket <n> | --spec) --round <k>
      --id <id> (--fixed <commit> | --rejected <reason>)
  director dispatch begin --worktree <path> --ticket <n> --worker <id>
  director dispatch finish --worktree <path> --ticket <n> --outcome <ok|failed>
      [--reason <text>]
  director report validate --worktree <path> --ticket <n> [--file <path>]
  director gate --worktree <path> [--ticket <n>]

Gate layers: `--ticket <n>` selects one ticket; `--spec` selects the aggregate
spec diff. `report validate` reads the `WORKER_REPORT:` envelope from stdin
unless `--file` names one (see WORKER_REPORT.md). Every command prints one JSON
object on stdout; errors go to stderr and exit non-zero.";

/// Parsed flags, each with the value it was given (`None` for a bare flag).
#[derive(Debug)]
pub(crate) struct Flags {
    entries: Vec<(String, Option<String>)>,
}

impl Flags {
    pub(crate) fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut entries: Vec<(String, Option<String>)> = Vec::new();
        let mut iter = args.into_iter().peekable();
        while let Some(arg) = iter.next() {
            if !arg.starts_with("--") {
                return Err(format!("unexpected argument: {arg}"));
            }
            let (name, inline) = match arg.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (arg, None),
            };
            let value = match inline {
                Some(value) => Some(value),
                None => match iter.peek() {
                    Some(next) if !next.starts_with("--") => iter.next(),
                    _ => None,
                },
            };
            if name != "--blocked-by" && entries.iter().any(|(seen, _)| *seen == name) {
                return Err(format!("duplicate flag: {name}"));
            }
            entries.push((name, value));
        }
        Ok(Self { entries })
    }

    fn take(&mut self, name: &str) -> Option<Option<String>> {
        let position = self.entries.iter().position(|(seen, _)| seen == name)?;
        let (_, value) = self.entries.remove(position);
        Some(value)
    }

    /// The flag's value, when present; a bare flag is an error.
    pub(crate) fn optional(&mut self, name: &str) -> Result<Option<String>, String> {
        match self.take(name) {
            Some(Some(value)) if !value.is_empty() => Ok(Some(value)),
            Some(_) => Err(format!("{name} requires a value")),
            None => Ok(None),
        }
    }

    pub(crate) fn required(&mut self, name: &str) -> Result<String, String> {
        match self.take(name) {
            Some(Some(value)) if !value.is_empty() => Ok(value),
            Some(_) => Err(format!("{name} requires a value")),
            None => Err(format!("{name} is required")),
        }
    }

    /// A valueless flag; giving it a value is an error.
    pub(crate) fn boolean(&mut self, name: &str) -> Result<bool, String> {
        match self.take(name) {
            Some(None) => Ok(true),
            Some(Some(_)) => Err(format!("{name} does not take a value")),
            None => Ok(false),
        }
    }

    /// Every value given for a repeatable flag.
    pub(crate) fn repeated(&mut self, name: &str) -> Result<Vec<String>, String> {
        let mut values = Vec::new();
        let mut index = 0;
        while index < self.entries.len() {
            if self.entries[index].0 == name {
                let (_, value) = self.entries.remove(index);
                match value {
                    Some(value) if !value.is_empty() => values.push(value),
                    _ => return Err(format!("{name} requires a value")),
                }
            } else {
                index += 1;
            }
        }
        Ok(values)
    }

    pub(crate) fn reject_unknown(&self) -> Result<(), String> {
        match self.entries.first() {
            Some((name, _)) => Err(format!("unknown flag: {name}\n{USAGE}")),
            None => Ok(()),
        }
    }
}

pub(crate) fn worktree(flags: &mut Flags) -> Result<PathBuf, String> {
    Ok(PathBuf::from(flags.required("--worktree")?))
}

pub(crate) fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{flag} must be a positive integer, got {value:?}"))?;
    if parsed == 0 {
        return Err(format!("{flag} must be a positive integer, got {value:?}"));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Flags {
        Flags::parse(args.iter().map(|arg| arg.to_string()).collect()).unwrap()
    }

    #[test]
    fn bare_and_valued_flags_parse_apart() {
        let mut flags = parsed(&["--worktree", "/tmp/wt", "--spec", "--round", "2"]);
        assert_eq!(flags.required("--worktree").unwrap(), "/tmp/wt");
        assert!(flags.boolean("--spec").unwrap());
        assert_eq!(flags.required("--round").unwrap(), "2");
        flags.reject_unknown().unwrap();
    }

    #[test]
    fn inline_values_parse() {
        let mut flags = parsed(&["--worktree=/tmp/wt", "--round=3"]);
        assert_eq!(flags.required("--worktree").unwrap(), "/tmp/wt");
        assert_eq!(flags.required("--round").unwrap(), "3");
    }

    #[test]
    fn repeated_flags_collect_in_order() {
        let mut flags = parsed(&["--blocked-by", "130", "--blocked-by=131"]);
        assert_eq!(flags.repeated("--blocked-by").unwrap(), vec!["130", "131"]);
    }

    #[test]
    fn bare_flag_where_a_value_is_required_is_refused() {
        let mut flags = parsed(&["--worktree", "--spec"]);
        assert!(flags.required("--worktree").is_err());
    }

    #[test]
    fn duplicate_single_value_flags_are_refused() {
        let error = Flags::parse(vec![
            "--worktree".to_string(),
            "/a".to_string(),
            "--worktree".to_string(),
            "/b".to_string(),
        ])
        .unwrap_err();
        assert!(error.contains("duplicate flag"), "got: {error}");
    }

    #[test]
    fn unknown_flags_are_refused() {
        let mut flags = parsed(&["--worktree", "/tmp/wt", "--nope"]);
        flags.required("--worktree").unwrap();
        let error = flags.reject_unknown().unwrap_err();
        assert!(error.contains("--nope"), "got: {error}");
    }

    #[test]
    fn positional_arguments_are_refused() {
        let error = Flags::parse(vec!["ticket".to_string()]).unwrap_err();
        assert!(error.contains("unexpected argument"), "got: {error}");
    }
}

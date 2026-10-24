//! A small argument parser: positionals, `--flag value` and bare `--switch`.
//! Unused flags are an error, so a typo never silently does nothing.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{anyhow, bail, Result};

/// Flags that take no value.
const SWITCHES: [&str; 1] = ["compact"];

pub struct Args {
    positionals: Vec<String>,
    flags: BTreeMap<String, String>,
    switches: Vec<String>,
    used: RefCell<Vec<String>>,
}

impl Args {
    pub fn parse(raw: Vec<String>) -> Result<Args> {
        let mut positionals = Vec::new();
        let mut flags = BTreeMap::new();
        let mut switches = Vec::new();
        let mut it = raw.into_iter();
        while let Some(arg) = it.next() {
            match arg.strip_prefix("--") {
                Some(name) if SWITCHES.contains(&name) => switches.push(name.to_string()),
                Some(name) => {
                    let value = it.next().ok_or_else(|| anyhow!("--{name} needs a value"))?;
                    flags.insert(name.to_string(), value);
                }
                None => positionals.push(arg),
            }
        }
        Ok(Args { positionals, flags, switches, used: RefCell::new(Vec::new()) })
    }

    pub fn positional(&self, i: usize) -> Option<String> {
        self.positionals.get(i).cloned()
    }

    pub fn flag(&self, name: &str) -> Option<String> {
        self.used.borrow_mut().push(name.to_string());
        self.flags.get(name).cloned()
    }

    pub fn parsed<T: FromStr>(&self, name: &str) -> Result<Option<T>> {
        match self.flag(name) {
            None => Ok(None),
            Some(v) => v.parse().map(Some).map_err(|_| anyhow!("--{name}: can't parse {v:?}")),
        }
    }

    pub fn switch(&self, name: &str) -> bool {
        self.used.borrow_mut().push(name.to_string());
        self.switches.iter().any(|s| s == name)
    }

    /// Error on any flag the command didn't read.
    pub fn finish(&self) -> Result<()> {
        let used = self.used.borrow();
        let unknown: Vec<&String> = self.flags.keys().chain(&self.switches).filter(|k| !used.contains(k)).collect();
        if !unknown.is_empty() {
            let names: Vec<String> = unknown.iter().map(|k| format!("--{k}")).collect();
            bail!("unknown option(s) for this command: {}", names.join(", "));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Args {
        Args::parse(s.split_whitespace().map(String::from).collect()).unwrap()
    }

    #[test]
    fn mixes_positionals_flags_and_switches() {
        let a = parse("timeline p.rink.json --interval 500 --compact");
        assert_eq!(a.positional(1).as_deref(), Some("p.rink.json"));
        assert_eq!(a.parsed::<u32>("interval").unwrap(), Some(500));
        assert!(a.switch("compact"));
        a.finish().unwrap();
    }

    #[test]
    fn unknown_flag_is_an_error() {
        let a = parse("snapshot p.rink.json 100 --intreval 5");
        assert!(a.finish().is_err());
    }

    #[test]
    fn negative_numbers_are_values() {
        let a = parse("suggest p --x -18.5 --y -3");
        assert_eq!(a.parsed::<f32>("x").unwrap(), Some(-18.5));
        assert_eq!(a.parsed::<f32>("y").unwrap(), Some(-3.0));
    }
}

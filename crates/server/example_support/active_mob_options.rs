//! Active-mob benchmark CLI, shared with the existing server unit-test target.
use std::path::PathBuf;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Bootstrap,
    Inspect,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Options {
    pub(super) steps: usize,
    pub(super) mode: Option<(Mode, PathBuf)>,
    pub(super) trace_executions: Option<usize>,
    pub(super) help: bool,
}
impl Options {
    pub(super) fn parse(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Self> {
        let mut options = Self {
            steps: 64,
            mode: None,
            trace_executions: None,
            help: false,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_ref() {
                "--smoke" => options.steps = 16,
                "--steps" => {
                    options.steps = args.next().ok_or("missing steps")?.as_ref().parse()?
                }
                "--bootstrap" | "--inspect" => {
                    if options.mode.is_some() {
                        return Err("multiple modes".into());
                    }
                    let mode = if arg.as_ref() == "--inspect" {
                        Mode::Inspect
                    } else {
                        Mode::Bootstrap
                    };
                    let path = PathBuf::from(args.next().ok_or("missing save")?.as_ref());
                    options.mode = Some((mode, path));
                }
                "--trace-executions" => {
                    if options.trace_executions.is_some() {
                        return Err("repeated --trace-executions".into());
                    }
                    let value = args.next().ok_or("missing trace executions")?;
                    let value = value.as_ref();
                    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                        return Err("trace executions must be a positive integer".into());
                    }
                    let count: usize = value.parse()?;
                    if count == 0 {
                        return Err("trace executions must be positive".into());
                    }
                    options.trace_executions = Some(count);
                }
                "--help" => {
                    options.help = true;
                    return Ok(options);
                }
                arg => return Err(format!("unknown argument {arg}").into()),
            }
        }
        if options.trace_executions.is_some() && !matches!(&options.mode, Some((Mode::Inspect, _)))
        {
            return Err("--trace-executions requires --inspect".into());
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_modes_keep_existing_defaults_and_steps() {
        let ordinary = Options::parse(std::iter::empty::<&str>()).unwrap();
        assert_eq!(ordinary.steps, 64);
        assert_eq!(ordinary.mode, None);
        assert_eq!(ordinary.trace_executions, None);
        assert!(!ordinary.help);
        assert_eq!(Options::parse(["--smoke"]).unwrap().steps, 16);
        assert_eq!(Options::parse(["--steps", "8"]).unwrap().steps, 8);
        assert_eq!(
            Options::parse(["--bootstrap", "new.db"]).unwrap().mode,
            Some((Mode::Bootstrap, PathBuf::from("new.db")))
        );
        assert!(Options::parse(["--help"]).unwrap().help);
    }
    #[test]
    fn inspection_defaults_to_all_or_accepts_an_explicit_prefix_in_either_order() {
        let all = Options::parse(["--inspect", "closed.db"]).unwrap();
        assert_eq!(all.mode, Some((Mode::Inspect, PathBuf::from("closed.db"))));
        assert_eq!(all.trace_executions, None);
        for args in [
            ["--inspect", "closed.db", "--trace-executions", "1631"],
            ["--trace-executions", "1631", "--inspect", "closed.db"],
        ] {
            let selected = Options::parse(args).unwrap();
            assert_eq!(selected.mode, all.mode);
            assert_eq!(selected.trace_executions, Some(1631));
        }
    }
    #[test]
    fn trace_cli_rejects_wrong_mode_missing_repeated_and_invalid_counts() {
        for args in [
            vec!["--trace-executions", "1"],
            vec!["--bootstrap", "new.db", "--trace-executions", "1"],
            vec!["--inspect", "closed.db", "--trace-executions"],
            vec![
                "--inspect",
                "closed.db",
                "--trace-executions",
                "1",
                "--trace-executions",
                "1",
            ],
            vec!["--inspect", "closed.db", "--bootstrap", "new.db"],
        ] {
            assert!(Options::parse(&args).is_err(), "accepted {args:?}");
        }
        for count in ["0", "-1", "+1", "1.5", "", "NaN", "184467440737095516160"] {
            assert!(
                Options::parse(["--inspect", "closed.db", "--trace-executions", count]).is_err(),
                "accepted count {count:?}"
            );
        }
    }
}

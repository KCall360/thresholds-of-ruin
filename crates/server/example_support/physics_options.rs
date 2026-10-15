//! Private benchmark arguments, shared with the existing server unit-test target.

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Options {
    pub(super) profile: bool,
    pub(super) smoke: bool,
}

impl Options {
    pub(super) fn parse(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Self, String> {
        let mut options = Self::default();
        for arg in args {
            match arg.as_ref() {
                "--profile" if !options.profile => options.profile = true,
                "--smoke" if !options.smoke => options.smoke = true,
                arg => {
                    return Err(format!(
                        "unknown or repeated argument: {arg}; usage: physics_bench [--profile] [--smoke]"
                    ));
                }
            }
        }
        Ok(options)
    }

    pub(super) fn sample_count(&self) -> usize {
        if self.smoke {
            1
        } else {
            3
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Options;

    #[test]
    fn ordinary_and_profiled_workloads_keep_three_samples_per_shape() {
        let ordinary = Options::parse(std::iter::empty::<&str>()).unwrap();
        assert_eq!(ordinary.sample_count(), 3);
        assert!(!ordinary.profile);
        assert!(!ordinary.smoke);
        let profiled = Options::parse(["--profile"]).unwrap();
        assert_eq!(profiled.sample_count(), 3);
        assert!(profiled.profile);
        assert!(!profiled.smoke);
    }

    #[test]
    fn explicit_smoke_runs_one_sample_and_preserves_optional_profiling() {
        let smoke = Options::parse(["--smoke"]).unwrap();
        assert_eq!(smoke.sample_count(), 1);
        assert!(!smoke.profile);
        assert!(smoke.smoke);
        for args in [["--profile", "--smoke"], ["--smoke", "--profile"]] {
            let profiled = Options::parse(args).unwrap();
            assert_eq!(profiled.sample_count(), 1);
            assert!(profiled.profile);
            assert!(profiled.smoke);
        }
    }

    #[test]
    fn unknown_ambiguous_and_repeated_options_are_rejected() {
        for args in [
            vec!["--unknown"],
            vec!["--prof"],
            vec!["--samples", "1"],
            vec!["--smoke=true"],
            vec!["1"],
            vec!["--smoke", "--smoke"],
            vec!["--profile", "--profile"],
            vec!["--smoke", "--profile", "--unknown"],
        ] {
            assert!(Options::parse(&args).is_err(), "accepted {args:?}");
        }
    }
}

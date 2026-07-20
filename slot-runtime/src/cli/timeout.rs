use std::time::Duration;

use super::CliCommand;

const RESCUE_TIMEOUT: Duration = Duration::from_secs(10);
const ORDINARY_TIMEOUT: Duration = Duration::from_mins(30);

pub(super) const fn for_command(command: &CliCommand) -> Duration {
    if matches!(command, CliCommand::Rescue { .. }) {
        RESCUE_TIMEOUT
    } else {
        ORDINARY_TIMEOUT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rescue_is_short_and_ordinary_work_is_long_but_bounded() {
        let rescue = CliCommand::Rescue {
            package: crate::target::PACKAGE.to_owned(),
            to_base: true,
        };
        assert_eq!(for_command(&rescue), Duration::from_secs(10));
        assert_eq!(for_command(&CliCommand::Probe), Duration::from_mins(30));
    }
}

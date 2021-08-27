use integration_test_commons::test::prelude::*;

use super::test_package::TestPackage;

/// The echo-service prints the content of the environment variable
/// `LOG_OUTPUT` to standard output and falls asleep.
///
/// A new line is appended so the content in `LOG_OUTPUT` should not end
/// with a new line character.
///
/// The capacity of environment variables depends on the system but it
/// is safe to store up to 120 kB in `LOG_OUTPUT`.
///
/// # Escape sequences
///
/// The following escape sequences are recognized:
///
/// - `\n`     new line
/// - `\\`     backslash
///
/// # Example
///
/// ```
/// $ LOG_OUTPUT='first line\nbackslash: \\\nheart: ♥' start.sh
/// first line
/// backslash: \
/// heart: ♥
/// ```
#[allow(dead_code)]
pub fn echo_service() -> TestPackage {
    TestPackage {
        name: String::from("echo-service"),
        version: String::from("1.0.0"),
        job: false,
        script: String::from(indoc!(
            r#"
            #!/bin/sh

            # Adding /run/current-system/sw/bin to PATH for NixOS support
            PATH=$PATH:/run/current-system/sw/bin

            printf '%b\n' "$LOG_OUTPUT"

            sleep 1d
            "#,
        )),
    }
}

/// The exit-service terminates immediately with the exit code contained
/// in the environment variable `EXIT_CODE`. If the environment variable
/// is not set then the exit code is 0.
#[allow(dead_code)]
pub fn exit_service() -> TestPackage {
    TestPackage {
        name: String::from("exit-service"),
        version: String::from("1.0.0"),
        job: true,
        script: String::from(indoc!(
            "
            #!/bin/sh

            exit ${EXIT_CODE:-0}
            "
        )),
    }
}

/// This service performs no operation and just sleeps.
#[allow(dead_code)]
pub fn noop_service() -> TestPackage {
    TestPackage {
        name: String::from("noop-service"),
        version: String::from("1.0.0"),
        job: false,
        script: String::from(indoc!(
            "
            #!/bin/sh

            # Adding /run/current-system/sw/bin to PATH for NixOS support
            PATH=$PATH:/run/current-system/sw/bin

            echo test-service started

            sleep 1d
            "
        )),
    }
}

/// The nostop-service performs no action, it just sleeps. The
/// difference to the noop service is, that this service will ignore
/// SIGINT and SIGTERM, which effectively means that it will not stop
/// when systemd asks it to stop.
#[allow(dead_code)]
pub fn nostop_service() -> TestPackage {
    TestPackage {
        name: String::from("nostop-service"),
        version: String::from("1.0.1"),
        job: false,
        script: String::from(indoc!(
            "
            #!/bin/sh

            # Adding /run/current-system/sw/bin to PATH for NixOS support
            PATH=$PATH:/run/current-system/sw/bin

            echo nostop-service started

            trap '' INT TERM
            sleep 1d
            "
        )),
    }
}

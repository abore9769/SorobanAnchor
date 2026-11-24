//! Narrow help-output assertions for offline commands and the deploy dry-run flag.
//!
//! Each test checks only the lines it is responsible for. Unrelated help text is
//! not snapshotted, so wording elsewhere can change without breaking these tests.
//! `--help` is handled by clap before any command logic runs, so none of these
//! tests touch the network, the filesystem config loader, or any credentials.

use assert_cmd::Command;
use predicates::str::contains;

fn cmd() -> Command {
    let mut cmd = Command::cargo_bin("anchorkit").expect("anchorkit binary not found");
    // Make sure no ambient credentials or contract IDs leak into the run.
    cmd.env_remove("ANCHOR_CONTRACT_ID")
        .env_remove("ANCHOR_ADMIN_SECRET")
        .env_remove("ANCHORKIT_EPHEMERAL_TOKEN")
        .env_remove("STELLAR_NETWORK");
    cmd
}

#[test]
fn offline_validate_help_shows_config_example() {
    cmd()
        .args(["offline", "validate", "--help"])
        .assert()
        .success()
        .stdout(contains("Examples:"))
        .stdout(contains(
            "anchorkit offline validate --config configs/remittance-anchor.toml",
        ));
}

#[test]
fn offline_simulate_help_shows_workflow_example() {
    cmd()
        .args(["offline", "simulate", "--help"])
        .assert()
        .success()
        .stdout(contains("Examples:"))
        .stdout(contains(
            "anchorkit offline simulate --workflow deploy --config configs/remittance-anchor.toml",
        ));
}

/// The deployment walkthrough tells operators to run `deploy --dry-run` first.
/// Guard the exact flag name so the guide cannot drift from the CLI.
#[test]
fn deploy_help_lists_dry_run_flag() {
    cmd()
        .args(["deploy", "--help"])
        .assert()
        .success()
        .stdout(contains("--dry-run"))
        .stdout(contains("Run pre-deployment checks only"));
}

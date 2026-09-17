mod config;
mod git;
mod http;
mod jev;
mod output;
mod review;

use anyhow::Result;
use clap::Parser;
use config::Args;
use std::process::ExitCode;

fn execute(args: Args) -> Result<u8> {
    args.validate()?;
    let ci = args.context()?;
    let (policy, hash) = review::load_policy(&args)?;
    let snapshot = git::collect(&args, &ci)?;
    eprintln!("ActionJev: {} eligible files, {} excluded or skipped", snapshot.files.len(), snapshot.skipped.len());
    let (report, traces) = review::run(&args, snapshot, &policy, hash)?;
    let directory = output::write_reports(&args, &report, &traces)?;
    eprintln!("ActionJev: reports written to {}", directory.display());
    if args.comment && !args.dry_run && !output::comment(&args, &ci, &report)? {
        eprintln!("ActionJev: PR head changed; stale report was not published");
    }
    if !report.complete && !args.allow_incomplete && !args.dry_run { return Ok(2); }
    if report.gate_failed(&args) { return Ok(3); }
    Ok(0)
}
fn main() -> ExitCode {
    match execute(Args::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("ActionJev error: {}", format!("{error:#}").replace(['\n', '\r'], " "));
            ExitCode::from(1)
        }
    }
}

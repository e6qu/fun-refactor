use clap::Args;

#[derive(Args)]
pub struct Options {
    #[arg(short, long, help = "Commit message; at most 16 KiB of UTF-8 text.")]
    message: String,
    #[arg(
        long,
        help = "Require the reviewed index, branch, parent, message and identities."
    )]
    basis: Option<String>,
    #[arg(long, requires = "basis", help = "Commit the entire reviewed index.")]
    write: bool,
    #[arg(
        long,
        default_value_t = 20,
        help = "Maximum changed paths to report, from 1 to 500."
    )]
    limit: usize,
}

#[cfg(unix)]
mod host;
#[cfg(unix)]
mod publish;
#[cfg(unix)]
pub(super) use host::report;

#[cfg(not(unix))]
pub(super) fn report(_: &std::path::Path, _: &Options) -> anyhow::Result<serde_json::Value> {
    anyhow::bail!("reviewed commits require Unix index lock ownership checks.")
}

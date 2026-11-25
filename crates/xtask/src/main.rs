use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about = "Repository automation for Placemat")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render THIRD-PARTY-NOTICES.html through cargo-about, failing on an unaccepted licence.
    Notices {
        /// Generate into a temporary directory and keep nothing (CI gate).
        #[arg(long)]
        check: bool,
        /// Where the notices page is written.
        #[arg(long, required_unless_present = "check")]
        out: Option<std::path::PathBuf>,
    },
    /// Write the complete cargo-packager config, with signing from the environment.
    PackagerConfig {
        /// Directory cargo-packager writes packages into.
        #[arg(long)]
        out_dir: std::path::PathBuf,
        /// File the JSON config is written to, readable only by the owner.
        #[arg(long)]
        output: std::path::PathBuf,
        /// The generated THIRD-PARTY-NOTICES.html, added to the resources.
        #[arg(long)]
        notices: std::path::PathBuf,
        /// Target triple whose release directory holds the binaries.
        #[arg(long)]
        target: Option<String>,
    },
    /// Render the PNG, ICO and ICNS exports into assets/icon/generated.
    Icons {
        /// Report exports that differ from a fresh render instead of writing them.
        #[arg(long)]
        check: bool,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Cmd::Notices { check, out } => exit(xtask::notices::run(check, out.as_deref())),
        Cmd::PackagerConfig {
            out_dir,
            output,
            notices,
            target,
        } => xtask::packager_config::run(&out_dir, &output, target.as_deref(), &notices),
        Cmd::Icons { check } => exit(xtask::icons::run(check)),
    }
}

// An error is exit 2: returning Err from main would exit 1, which means "failed the check".
fn exit(outcome: anyhow::Result<i32>) -> ! {
    match outcome {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("xtask: {e:#}");
            std::process::exit(2)
        }
    }
}

use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, disable_help_flag = true)]
pub(crate) struct CliArgs {
    /// Server bind address
    #[arg(short, long, default_value_t = ("0.0.0.0").to_string())]
    pub host: String,

    /// Server bind port
    #[arg(short, long, default_value_t = 1987)]
    pub port: u32,

    /// Transmuxer timeout in seconds
    #[arg(short, long, default_value_t = 5)]
    pub timeout: u64,

    /// Transcoder(aka ffmpeg) configuration file
    #[arg(short, long)]
    pub config: Option<String>,

    /// Print this help
    #[arg(id = "help", action = clap::ArgAction::Help,long)]
    pub help: Option<String>,
}

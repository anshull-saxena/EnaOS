use clap::Parser;

/// EnaOS native AI bar — GTK4/libadwaita layer-shell panel.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to enad's Unix domain socket.
    #[arg(short, long, default_value = "/tmp/enad.sock")]
    pub socket_path: String,

    /// Enable verbose logging.
    #[arg(short, long)]
    pub verbose: bool,
}

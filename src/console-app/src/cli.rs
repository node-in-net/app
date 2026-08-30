use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(long, default_value_t = false)]
    pub watch_bin: bool,

    #[arg(long, default_value_t = false)]
    pub setup: bool,

    #[arg(long, default_value_t = false)]
    pub check_update: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Join without recording this machine on the server: it shows to the account's \
                other devices while it is running and leaves their lists when it stops"
    )]
    pub guest: bool,

    #[arg(long)]
    pub token: Option<String>,

    #[arg(long)]
    pub exec: Option<String>,
}

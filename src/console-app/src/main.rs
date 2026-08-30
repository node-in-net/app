mod app_init;
mod cli;
mod event_loop;
mod exec_command;
mod setup;

use clap::Parser;

#[tokio::main]
async fn main() {
    let args = cli::Args::parse();
    app_init::start_application(args).await;
}

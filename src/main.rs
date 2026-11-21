mod cli;
mod net;
mod openai;
mod proof;
mod quota;
mod run;
mod socket;
mod support;
mod thread;

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    net::client::install_crypto();

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        support::log::finish_status();
        support::log::lr("Script stopped by user");
        std::process::exit(0);
    });

    support::log::arm();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--quiet") {
        support::log::mute_live(true);
    }

    support::banner::print_banner();

    match cli::dispatch::dispatch(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            support::log::lr(&message);
            ExitCode::FAILURE
        }
    }
}

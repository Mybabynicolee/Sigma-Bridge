mod cli;
mod net;
mod openai;
mod proof;
mod quota;
mod run;
mod socket;
mod support;
mod thread;

fn main() {
    net::client::install_crypto();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--quiet") {
        support::log::mute_live(true);
    }

    support::banner::print_banner();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            support::log::lr(&format!("The runtime could not start: {}", err));
            std::process::exit(1);
        }
    };

    let outcome = runtime.block_on(cli::dispatch::dispatch(&args));

    if let Err(message) = outcome {
        support::log::lr(&message);
        std::process::exit(1);
    }
}

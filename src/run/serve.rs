use std::path::Path;
use std::sync::Arc;

use crate::openai::route::Bridge;
use crate::openai::server::Server;
use crate::proof::vault::Vault;
use crate::quota::guard::{Guard, Settings};
use crate::support::cfg;
use crate::support::consts::{MODEL_ID, POW_ACTION};
use crate::support::error::AppResult;
use crate::support::log::{gap, lg, ly};
use crate::thread::ledger::Ledger;

pub async fn boot(root: &Path) -> AppResult<Arc<Bridge>> {
    let setup = cfg::load(root)?;
    let ledger = match crate::cli::probe::access_from_env() {
        Some(access) => {
            ly("SIGMA_ACCESS is set, serving that one access token instead of data.txt");
            Ledger::from_access(&access)?
        }
        None => Ledger::load(&setup.data_path())?,
    };
    let count = ledger.len();

    let settings = Settings {
        round_robin: setup.config.round_robin(),
        sticky: setup.config.sticky(),
    };
    let guard = Guard::new(settings, count)?;
    let vaults: Vec<Arc<Vault>> = (0..count)
        .map(|_| Arc::new(Vault::new(POW_ACTION)))
        .collect();

    lg(&format!(
        "Loaded {} account{}, {} proxy line{}",
        count,
        if count == 1 { "" } else { "s" },
        setup.proxies.len(),
        if setup.proxies.len() == 1 { "" } else { "s" }
    ));

    Ok(Arc::new(Bridge {
        settings: setup,
        ledger: Arc::new(ledger),
        guard: Arc::new(guard),
        vaults,
    }))
}

pub async fn serve(root: &Path) -> AppResult<()> {
    let bridge = boot(root).await?;
    let port = bridge.settings.config.port();

    lg(&format!("Serving {} on port {}", MODEL_ID, port));
    lg(&format!(
        "Sleep window {} seconds between sweeps",
        bridge.settings.config.sleep_seconds()
    ));
    gap();

    let server = Server {
        bridge: bridge.clone(),
    };
    server.serve(port).await
}

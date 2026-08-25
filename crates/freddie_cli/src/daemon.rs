//! The hidden `daemon` verb: run the daemon in this process.

use tracing::{error, info};

use crate::verb::DaemonVerbArgs;
use crate::{App, Instance};

/// Take the lock and run the app's daemon in this process.
pub(crate) fn run_in_foreground<TApp: App>(
    instance: &Instance,
    args: &DaemonVerbArgs<TApp::Id, TApp::DaemonArgs>,
) {
    info!(path = %instance.log_file().display(), "logging");

    // Before anything that touches the machine. Must outlive the call (`let _held`, never `let _`):
    // dropping the binding releases the lock.
    let _held = match freddie_single_instance::acquire_at(instance.lock_file()) {
        Ok(held) => held,
        Err(e) => {
            error!(daemon = instance.display_name(), error = %e, "already running; `stop` ends it");
            return;
        }
    };

    TApp::run_daemon(&args.id, &args.app);
}

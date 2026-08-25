//! `mercury install` and `mercury uninstall`: the login launch agent.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use freddie_cli::{App, DAEMON_VERB};
use serde::Serialize;
use tracing::{debug, info, warn};

use crate::Mercury;

/// install and uninstall the launch agent.
#[derive(clap::Subcommand, Debug)]
pub(crate) enum MercuryVerb {
    /// Register this binary as a login agent, so mercury starts with the session.
    Install,
    /// Take the login agent back out.
    Uninstall,
}

/// The reverse-DNS prefix mercury's launch agent sits under.
const AGENT_PREFIX: &str = "hg.freddie.";

/// Launchd job name, keyed to the app name so a rename gets its own job.
fn label() -> String {
    format!("{AGENT_PREFIX}{}", Mercury::NAME)
}

fn log_to_mercurys_file() {
    if let Ok(instance) = Mercury::instance(&freddie_cli::NoArgs) {
        freddie_cli::init_client_logging(&instance);
    }
}

/// Absolute so PATH cannot redirect it.
const LAUNCHCTL: &str = "/bin/launchctl";

/// Absolute; the workspace forbids the unsafe `getuid` binding.
const ID: &str = "/usr/bin/id";

/// A binary under this is not somewhere an agent should point for long: `cargo clean` deletes it.
const TRANSIENT: &str = "/target/";

/// The launch agent plist. Serialized so a program path containing `&` is escaped.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Agent {
    /// Keyed to the app name so a rename gets its own job.
    label: String,

    /// The daemon verb, not the bare binary: `mercury` spawns a detached daemon and exits, which would leave launchd watching the job vanish.
    program_arguments: Vec<String>,

    run_at_load: bool,

    /// `Aqua`: `CGEventTap` needs a window server, `NSWorkspace`, and per-user TCC, which a root daemon at the login window does not have.
    limit_load_to_session_type: String,

    keep_alive: KeepAlive,

    /// Seconds. A crash loop must not respawn something that eats the keyboard ten times a second.
    throttle_interval: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct KeepAlive {
    /// `false`: revive a crash, leave a clean exit down. Every deliberate quit and every refused start exits 0.
    successful_exit: bool,
}

impl Agent {
    fn running(program: &Path) -> Self {
        Self {
            label: label(),
            program_arguments: vec![
                program.to_string_lossy().into_owned(),
                DAEMON_VERB.to_owned(),
            ],
            run_at_load: true,
            limit_load_to_session_type: "Aqua".to_owned(),
            keep_alive: KeepAlive {
                successful_exit: false,
            },
            throttle_interval: 10,
        }
    }
}

/// `~/Library/LaunchAgents/<label>.plist`.
fn plist_path() -> Option<PathBuf> {
    Some(
        PathBuf::from(std::env::var_os("HOME")?)
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", label())),
    )
}

enum NotInstalled {
    NoHome,
    NoExe(io::Error),
    Unwritable(io::Error),
    Unserializable(plist::Error),
    Refused(io::Error),
}

impl fmt::Display for NotInstalled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHome => {
                f.write_str("no home directory to install the agent into; is HOME set?")
            }
            Self::NoExe(e) => write!(f, "could not read this binary's path: {e}"),
            Self::Unwritable(e) => write!(f, "could not write the agent: {e}"),
            Self::Unserializable(e) => write!(f, "could not build the agent: {e}"),
            Self::Refused(e) => write!(f, "{e}"),
        }
    }
}

/// Register this binary as a login agent.
///
/// Idempotent. Boots out a previously loaded job before bootstrapping, so re-running after `cargo install` points the agent at the rebuilt binary.
pub(crate) fn install() -> ExitCode {
    log_to_mercurys_file();
    match install_agent() {
        Ok(program) => {
            info!("mercury installed ({})", program.display());
            if program.to_string_lossy().contains(TRANSIENT) {
                warn!(
                    "mercury: that binary is under target/, which `cargo clean` deletes; \
                     `cargo install --path crates/mercury` and install again to point at a lasting one"
                );
            }
            ExitCode::SUCCESS
        }
        Err(failure) => {
            warn!("mercury: {failure}");
            ExitCode::FAILURE
        }
    }
}

fn install_agent() -> Result<PathBuf, NotInstalled> {
    let program = std::env::current_exe().map_err(NotInstalled::NoExe)?;
    let path = plist_path().ok_or(NotInstalled::NoHome)?;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(NotInstalled::Unwritable)?;
    }
    plist::to_file_xml(&path, &Agent::running(&program)).map_err(NotInstalled::Unserializable)?;
    debug!(plist = %path.display(), program = %program.display(), "wrote the agent");

    // First install has nothing loaded to boot out. Traced so the log still says what launchd made of it.
    if let Err(e) = bootout() {
        debug!(%e, "nothing was loaded to boot out");
    }
    launchctl(&["bootstrap", &domain()?, &path.to_string_lossy()])?;
    Ok(program)
}

/// Take the login agent back out.
///
/// Exits 0 when nothing was installed, so a teardown script can call it without knowing the state.
pub(crate) fn uninstall() -> ExitCode {
    log_to_mercurys_file();
    match uninstall_agent() {
        Ok(()) => {
            info!("mercury uninstalled");
            ExitCode::SUCCESS
        }
        Err(failure) => {
            warn!("mercury: {failure}");
            ExitCode::FAILURE
        }
    }
}

fn uninstall_agent() -> Result<(), NotInstalled> {
    let path = plist_path().ok_or(NotInstalled::NoHome)?;
    // Boot out before removing the plist, or launchd is left holding a job whose plist is gone. Failure is nothing loaded.
    if let Err(e) = bootout() {
        debug!(%e, "nothing was loaded to boot out");
    }
    match std::fs::remove_file(&path) {
        Ok(()) => {
            debug!(plist = %path.display(), "removed the agent");
            Ok(())
        }
        // Nothing installed is not a failure.
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(NotInstalled::Unwritable(e)),
    }
}

/// launchd GUI domain for this user.
fn domain() -> Result<String, NotInstalled> {
    Ok(format!("gui/{}", users_uid()?))
}

fn bootout() -> Result<(), NotInstalled> {
    launchctl(&["bootout", &format!("{}/{}", domain()?, label())])
}

/// Run `launchctl`. Captures stderr so a `bootout` with nothing loaded does not print beside "mercury installed".
fn launchctl(args: &[&str]) -> Result<(), NotInstalled> {
    let out = Command::new(LAUNCHCTL)
        .args(args)
        .output()
        .map_err(NotInstalled::Refused)?;
    if out.status.success() {
        Ok(())
    } else {
        let said = String::from_utf8_lossy(&out.stderr);
        Err(NotInstalled::Refused(io::Error::other(format!(
            "{LAUNCHCTL} {} exited with {}: {}",
            args.join(" "),
            out.status,
            said.trim()
        ))))
    }
}

/// This user's uid, which names the launchd domain.
///
/// Spawned via `/usr/bin/id` because the workspace forbids `unsafe` and `getuid` is an unsafe extern.
fn users_uid() -> Result<u32, NotInstalled> {
    let out = Command::new(ID)
        .arg("-u")
        .output()
        .map_err(NotInstalled::Refused)?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .map_err(|_| NotInstalled::Refused(io::Error::other(format!("{ID} -u printed no number"))))
}

#[cfg(test)]
mod tests {
    use super::{Agent, label};
    use std::path::Path;

    fn agent_xml(program: &str) -> String {
        let mut xml = Vec::new();
        plist::to_writer_xml(&mut xml, &Agent::running(Path::new(program))).expect("serializing");
        String::from_utf8(xml).expect("plists are utf8")
    }

    #[test]
    fn the_label_is_keyed_to_the_app() {
        assert_eq!(label(), "hg.freddie.mercury");
    }

    #[test]
    fn the_agent_runs_the_daemon_verb() {
        let xml = agent_xml("/Users/somebody/.cargo/bin/mercury");
        assert!(xml.contains("<string>/Users/somebody/.cargo/bin/mercury</string>"));
        assert!(xml.contains("<string>daemon</string>"));
        assert!(xml.contains("<key>Label</key>"));
    }

    #[test]
    fn a_program_path_is_escaped() {
        let xml = agent_xml("/Users/a&b/.cargo/bin/mercury");
        assert!(xml.contains("/Users/a&amp;b/.cargo/bin/mercury"));
        assert!(!xml.contains("/Users/a&b/"));
    }

    #[test]
    fn the_agent_only_revives_an_unclean_exit() {
        let xml = agent_xml("/usr/bin/true");
        assert!(xml.contains("<key>SuccessfulExit</key>"));
        assert!(xml.contains("<false/>"));
    }
}

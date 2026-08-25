//! The lifecycle verbs, and the flags each one takes.

use crate::App;
use crate::logging::Terminal;

/// Lifecycle verbs for an app to flatten into its own command line.
///
/// Each variant's doc comment is its line in `--help`.
#[derive(clap::Subcommand)]
pub enum Verb<TApp: App> {
    /// Start the daemon if it is not running, and exit.
    Start(StartArgs<TApp::Id, TApp::DaemonArgs>),
    /// Stop the running daemon and start a fresh one.
    Restart(RestartArgs<TApp::Id, TApp::DaemonArgs>),
    /// Report whether the daemon is running, and its pid.
    Status(IdArgs<TApp::Id>),
    /// Follow the log, starting nothing.
    Logs(LogsArgs<TApp::Id>),
    /// Ask the running daemon to quit.
    Stop(StopArgs<TApp::Id>),
    /// Run the daemon in this process. Not for typing: `start` spawns it.
    #[command(hide = true)]
    Daemon(DaemonVerbArgs<TApp::Id, TApp::DaemonArgs>),
}

impl<TApp: App> Verb<TApp> {
    pub(crate) const fn terminal(&self) -> Terminal {
        match self {
            Self::Daemon(_) => Terminal::Daemon,
            Self::Start(_) | Self::Restart(_) | Self::Status(_) | Self::Logs(_) | Self::Stop(_) => {
                Terminal::Client
            }
        }
    }

    pub(crate) const fn id(&self) -> &TApp::Id {
        match self {
            Self::Start(args) => &args.id,
            Self::Restart(args) => &args.id,
            Self::Status(args) => &args.id,
            Self::Logs(args) => &args.id,
            Self::Stop(args) => &args.id,
            Self::Daemon(args) => &args.id,
        }
    }
}

/// Which daemon to be, and the app's own daemon flags.
#[derive(clap::Args, Debug)]
pub struct DaemonVerbArgs<I: clap::Args, F: clap::Args> {
    #[command(flatten)]
    pub id: I,

    #[command(flatten)]
    pub app: F,
}

/// Flags `start` forwards to the daemon it spawns.
#[derive(clap::Args, Debug)]
pub struct StartArgs<I: clap::Args, F: clap::Args> {
    #[command(flatten)]
    pub id: I,

    #[command(flatten)]
    pub app: F,
}

/// `start`'s flags, plus how hard to stop what is running.
#[derive(clap::Args, Debug)]
pub struct RestartArgs<I: clap::Args, F: clap::Args> {
    /// Destroy the running daemon with SIGKILL instead of asking it to quit.
    ///
    /// For a daemon that no longer answers. Destructors do not run.
    #[arg(long)]
    pub force: bool,

    #[command(flatten)]
    pub id: I,

    #[command(flatten)]
    pub app: F,
}

/// Flags that name a daemon, for verbs that only find one.
#[derive(clap::Args, Debug)]
pub struct IdArgs<I: clap::Args> {
    #[command(flatten)]
    pub id: I,
}

/// Flags for `logs`.
#[derive(clap::Args, Debug)]
pub struct LogsArgs<I: clap::Args> {
    /// Least severe records to show: `error`, `warn`, `info`, `debug`, or `trace`.
    ///
    /// The file always records `debug`. This only filters the terminal.
    #[arg(long, default_value = crate::logging::DEFAULT_LOG_LEVEL)]
    pub level: tracing::Level,

    /// Include the model state on each dispatch record. Off unless asked for: it is most of the line.
    #[arg(long)]
    pub include_state: bool,

    /// Write each record as the JSON it is stored as, for `jq`.
    #[arg(long)]
    pub json: bool,

    #[command(flatten)]
    pub id: I,
}

/// Flags for `stop`.
#[derive(clap::Args, Debug)]
pub struct StopArgs<I: clap::Args> {
    /// Destroy the daemon with SIGKILL instead of asking it to quit.
    ///
    /// For a daemon that no longer answers. Destructors do not run.
    #[arg(long)]
    pub force: bool,

    #[command(flatten)]
    pub id: I,
}

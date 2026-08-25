//! Lifecycle verbs for one daemon: `start`, `restart`, `status`, `logs`, `stop`, and a hidden `daemon`.
//!
//! An app declares its own [`clap::Parser`] and flattens [`Verb`] into it.

use std::ffi::OsString;
use std::fmt;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{ArgMatches, Args as _, Command, FromArgMatches as _};

mod client;
mod daemon;
mod instance;
mod logging;
mod stdio_inherit;
mod verb;

pub use instance::{Instance, NoUserDir};
pub use logging::LOG_LEVEL;
pub use verb::{DaemonVerbArgs, IdArgs, LogsArgs, RestartArgs, StartArgs, StopArgs, Verb};

/// The hidden daemon subcommand name, for an app building an argv that has to reach it.
pub const DAEMON_VERB: &str = "daemon";

/// One impl per binary. The lifecycle verbs are generic over this.
pub trait App {
    /// Flags that name one of this app's daemons. [`NoArgs`] for an app with one global daemon.
    ///
    /// Two values of this type are two daemons only if [`instance`](Self::instance) produces
    /// two instances. Fields that `instance` ignores still reach [`run_daemon`](Self::run_daemon),
    /// but two `start`s that differ only in those fields are one daemon.
    type Id: clap::Args + fmt::Debug;

    /// Flags this app's daemon takes beyond [`Id`](Self::Id). [`NoArgs`] if none.
    ///
    /// Separate from `Id` so verbs that only find a daemon refuse the flags that configure one.
    type DaemonArgs: clap::Args + fmt::Debug;

    /// The binary name, which is what the log directory is called.
    const NAME: &'static str;

    /// Which daemon the command line named.
    ///
    /// Called before the lock, the log file, and any subprocess. An id names what the flag
    /// points at: two paths to one file are one daemon.
    ///
    /// # Errors
    ///
    /// The log file has no name yet (it comes from the returned instance), so the error is
    /// returned rather than logged.
    fn instance(id: &Self::Id) -> Result<Instance, Box<dyn std::error::Error + Send + Sync>>;

    /// Be the daemon. Called with the lock held and logging initialized. Returning drops the lock.
    fn run_daemon(id: &Self::Id, args: &Self::DaemonArgs);

    /// Ask the running daemon to quit. Windows only.
    ///
    /// Unix `stop` without `--force` is SIGTERM. Windows has none: `taskkill` without `/F`
    /// posts `WM_CLOSE`, which a daemon with no window never sees. Takes [`Instance`], not
    /// [`Id`](Self::Id): `stop` has already resolved the daemon.
    #[cfg(windows)]
    fn ask_to_quit(_instance: &Instance) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("graceful stop is not yet available on Windows; use --force".into())
    }
}

/// Empty clap args: no id flags, or no extra daemon flags.
#[derive(clap::Args, Debug)]
pub struct NoArgs;

/// Send this process's tracing to `instance`'s log file and to a client verb's terminal.
///
/// For an app's own verbs. [`run_lifecycle_verb`] already does this for the verbs it runs.
pub fn init_client_logging(instance: &Instance) {
    logging::init(instance, logging::Terminal::Client);
}

/// Resolve which daemon a lifecycle verb means, start logging, and run the verb.
///
/// Takes the whole `ArgMatches` because verbs that spawn a daemon re-emit the flags this
/// invocation typed.
#[must_use]
pub fn run_lifecycle_verb<TApp: App>(verb: Verb<TApp>, matches: &ArgMatches) -> ExitCode {
    let instance = match TApp::instance(verb.id()) {
        Ok(instance) => instance,
        // No log yet: its path comes from the instance this failed to produce. A bad id is a
        // clap parse error.
        Err(e) => clap::Error::raw(ErrorKind::ValueValidation, format!("{e}\n")).exit(),
    };

    logging::init(&instance, verb.terminal());
    run_verb_on::<TApp>(verb, &instance, TypedArgs::of(matches))
}

fn run_verb_on<TApp: App>(verb: Verb<TApp>, instance: &Instance, typed: TypedArgs<'_>) -> ExitCode {
    match verb {
        Verb::Start(_) => client::start::<TApp>(instance, typed),
        Verb::Restart(args) => client::restart::<TApp>(instance, args.force, typed),
        Verb::Status(_) => client::status(instance),
        Verb::Logs(args) => client::logs(
            instance,
            client::LogsView {
                least: args.level,
                include_state: args.include_state,
                json: args.json,
            },
        ),
        Verb::Stop(args) => client::stop::<TApp>(instance, args.force),
        Verb::Daemon(args) => {
            daemon::run_in_foreground::<TApp>(instance, &args);
            ExitCode::SUCCESS
        }
    }
}

/// `start` with every flag left unsaid. For an app's `None` arm, when the parser saw no verb.
///
/// # Panics
///
/// If the derived parser and the derived command disagree.
#[must_use]
pub fn verb_for_bare_invocation<TApp: App>() -> Verb<TApp> {
    let command = StartArgs::<TApp::Id, TApp::DaemonArgs>::augment_args(Command::new(TApp::NAME));
    let matches = match command.try_get_matches_from([TApp::NAME]) {
        Ok(matches) => matches,
        Err(e) => e.exit(),
    };
    Verb::Start(
        StartArgs::from_arg_matches(&matches)
            .expect("the derived type matches the command it derived"),
    )
}

/// Flags this invocation typed, for re-emitting to a spawned daemon. `None` is the bare binary.
#[derive(Clone, Copy)]
pub(crate) struct TypedArgs<'a>(Option<&'a ArgMatches>);

impl<'a> TypedArgs<'a> {
    fn of(matches: &'a ArgMatches) -> Self {
        Self(matches.subcommand().map(|(_name, verb)| verb))
    }
}

impl TypedArgs<'_> {
    /// Re-emit typed flags as argv for a spawned daemon. Defaults and env values are left out:
    /// the child resolves those the same way.
    pub(crate) fn argv<TApp: App>(self) -> Vec<OsString> {
        let Some(matches) = self.0 else {
            return Vec::new();
        };
        let mut argv = Vec::new();
        Self::emit::<TApp::Id>(matches, &mut argv);
        Self::emit::<TApp::DaemonArgs>(matches, &mut argv);
        argv
    }

    fn emit<T: clap::Args>(matches: &ArgMatches, argv: &mut Vec<OsString>) {
        use clap::builder::ArgAction;
        use clap::parser::ValueSource;

        for arg in T::augment_args(Command::new("probe")).get_arguments() {
            let id = arg.get_id().as_str();
            let carried = matches.value_source(id) == Some(ValueSource::CommandLine);
            // A positional has no flag to re-emit it under.
            let Some(long) = arg.get_long().filter(|_| carried) else {
                continue;
            };
            let flag = OsString::from(format!("--{long}"));
            match arg.get_action() {
                ArgAction::SetTrue | ArgAction::SetFalse => argv.push(flag),
                _ => {
                    for value in matches.try_get_raw(id).ok().flatten().into_iter().flatten() {
                        argv.push(flag.clone());
                        argv.push(value.to_owned());
                    }
                }
            }
        }
    }
}

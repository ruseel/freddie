//! The mercury binary.

use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use freddie_cli::{App, Instance, NoArgs};

mod automation_socket;
mod daemon;
mod launch_agent;

#[derive(Parser)]
#[command(name = "mercury", version, about = "A layered keyboard remapper.", long_about = None)]
struct MercuryCli {
    #[command(subcommand)]
    verb: Option<MercuryVerb>,
}

#[derive(Subcommand)]
enum MercuryVerb {
    /// start, restart, status, logs, stop, and the hidden daemon.
    #[command(flatten)]
    Lifecycle(freddie_cli::Verb<Mercury>),

    /// install and uninstall the launch agent.
    #[command(flatten)]
    Agent(launch_agent::MercuryVerb),
}

#[derive(clap::Args, Debug)]
pub struct MercuryArgs {
    /// The loopback port the event socket listens on.
    #[arg(long, env = "MERCURY_PORT", default_value_t = mercury::DEFAULT_PORT)]
    pub port: u16,

    /// The loopback port that receives Button Decks automation commands.
    #[arg(long, env = "MERCURY_AUTOMATION_PORT", default_value_t = automation_socket::DEFAULT_PORT)]
    pub automation_port: u16,
}

pub struct Mercury;

impl App for Mercury {
    // One mercury per machine, so no instance id.
    type Id = NoArgs;
    type DaemonArgs = MercuryArgs;

    const NAME: &'static str = "mercury";

    fn instance(_: &NoArgs) -> Result<Instance, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Instance::global(Self::NAME)?)
    }

    fn run_daemon(_: &NoArgs, args: &MercuryArgs) {
        daemon::run(args.port, args.automation_port);
    }
}

fn main() -> ExitCode {
    // Parse first so `--help` and a bad flag exit before the lock, the keyboard, or the icon.
    // `run_lifecycle_verb` rereads these matches to forward flags to the daemon it spawns.
    let matches = MercuryCli::command().get_matches();
    let cli = MercuryCli::from_arg_matches(&matches)
        .expect("the derived type matches the command it derived");

    match cli.verb {
        Some(MercuryVerb::Lifecycle(verb)) => {
            freddie_cli::run_lifecycle_verb::<Mercury>(verb, &matches)
        }
        Some(MercuryVerb::Agent(launch_agent::MercuryVerb::Install)) => launch_agent::install(),
        Some(MercuryVerb::Agent(launch_agent::MercuryVerb::Uninstall)) => launch_agent::uninstall(),
        None => freddie_cli::run_lifecycle_verb::<Mercury>(
            freddie_cli::verb_for_bare_invocation::<Mercury>(),
            &matches,
        ),
    }
}

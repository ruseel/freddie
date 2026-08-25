//! Mercury: a layered keyboard remapper. Re-exports the model and the event-socket vocabulary.

mod external;

pub use external::{DEFAULT_PORT, on_message};
pub use mercury_model::*;

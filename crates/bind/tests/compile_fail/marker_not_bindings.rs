// The `#[binds(..)]` type must implement `Bindings`. The node is otherwise complete, so the only failure is the missing impl.
use bind::Bind;

struct NotBindings;

#[derive(Bind)]
#[node(root)]
#[binds(NotBindings)]
struct Nav {}

enum R<'a> {
    Nav(&'a mut Nav),
}

fn main() {}

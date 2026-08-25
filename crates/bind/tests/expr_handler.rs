//! A handler position accepts any expression, not just a fn path. The derive splices the rhs into call position.

mod common;

use bind::{AscendState, Bind};
use common::{Demo, KeyEvent, Keyboard, key};
use laserbeam::Completed;

fn plus(
    n: usize,
) -> impl for<'a, 'b> Fn(
    &KeyEvent,
    (),
    AscendState<'a, &'b mut ExprRoot>,
) -> (Vec<usize>, Completed<&'b mut ExprRoot>) {
    move |ev, (), st| (vec![ev.key.len() + n], st.complete())
}

#[derive(Bind)]
#[node(root)]
#[binds(Demo)]
#[bind(Keyboard("x") => plus(10))]
struct ExprRoot;

#[test]
fn expression_handler_is_called() {
    let mut root = ExprRoot;
    assert_eq!(
        bind::dispatch::<Demo, ExprRoot, _>(&mut root, &key("x")),
        vec![11]
    );
    assert_eq!(
        bind::dispatch::<Demo, ExprRoot, _>(&mut root, &key("y")),
        vec![]
    );
}

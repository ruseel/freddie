//! Accumulation over the shared tree.

mod common;

use std::collections::HashSet;

use common::{
    App, Armed, ArmedChild, Clash, ClashChild, Deep, Demo, Empty, Layer, Nav, Typing, fg, kb,
};

#[test]
fn through_enum_and_child() {
    let mut app = App {
        hits: 0,
        layer: Layer::Nav(Nav { hits: 0 }),
    };
    let set = bind::accumulate::<Demo, App>(&mut app).unwrap();
    assert_eq!(
        set,
        HashSet::from([kb("esc"), kb("f1"), kb("g"), fg("Slack")])
    );
}

#[test]
fn through_boxed_child() {
    let mut app = App {
        hits: 0,
        layer: Layer::Typing(Typing {
            hits: 0,
            deep: Box::new(Deep { hits: 0 }),
        }),
    };
    let set = bind::accumulate::<Demo, App>(&mut app).unwrap();
    assert_eq!(
        set,
        HashSet::from([kb("esc"), kb("f1"), kb("bksp"), kb("d")])
    );
}

#[test]
fn duplicate_trigger_is_error() {
    let mut clash = Clash {
        child: ClashChild {},
    };
    assert_eq!(
        bind::accumulate::<Demo, Clash>(&mut clash),
        Err(bind::BindError::DuplicateTrigger)
    );
}

#[test]
fn no_binds_is_empty() {
    let set = bind::accumulate::<Demo, Empty>(&mut Empty {}).unwrap();
    assert!(set.is_empty());
}

#[test]
fn a_closure_trigger_is_not_collected() {
    let mut armed = Armed {
        waiting_for: Some("g"),
        for_child: Some("up"),
        child: ArmedChild { wants: Some("z") },
    };
    let set = bind::accumulate::<Demo, Armed>(&mut armed).unwrap();
    assert_eq!(set, HashSet::from([kb("esc")]));
}

#[test]
fn two_nodes_with_nothing_to_match_are_not_a_duplicate() {
    let mut armed = Armed {
        waiting_for: None,
        for_child: None,
        child: ArmedChild { wants: None },
    };
    assert!(bind::accumulate::<Demo, Armed>(&mut armed).is_ok());
}

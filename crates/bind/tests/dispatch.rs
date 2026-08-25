//! Dispatch over the shared tree. Handlers return the fired key's length, so the return value identifies which one ran.

mod common;

use common::{
    Album, App, Armed, ArmedChild, Deep, Demo, Layer, Media, Nav, Song, Title, Typing, foreground,
    key,
};

const fn nav_app() -> App {
    App {
        hits: 0,
        layer: Layer::Nav(Nav { hits: 0 }),
    }
}

fn typing_app() -> App {
    App {
        hits: 0,
        layer: Layer::Typing(Typing {
            hits: 0,
            deep: Box::new(Deep { hits: 0 }),
        }),
    }
}

#[test]
fn leaf_binding_fires() {
    let mut app = nav_app();
    let out = bind::dispatch::<Demo, App, _>(&mut app, &key("g"));
    assert_eq!(out, vec![1]);
    let Layer::Nav(nav) = &app.layer else {
        unreachable!()
    };
    assert_eq!(nav.hits, 1);
    assert_eq!(app.hits, 0);
}

#[test]
fn ancestor_binding_fires_after_subtree_misses() {
    let mut app = nav_app();
    let out = bind::dispatch::<Demo, App, _>(&mut app, &key("esc"));
    assert_eq!(out, vec![3]);
    assert_eq!(app.hits, 1);
    let Layer::Nav(nav) = &app.layer else {
        unreachable!()
    };
    assert_eq!(nav.hits, 0);
}

#[test]
fn enum_binding_fires() {
    let mut app = nav_app();
    let out = bind::dispatch::<Demo, App, _>(&mut app, &key("f1"));
    assert_eq!(out, vec![2]);
}

#[test]
fn through_typing_variant() {
    let mut app = typing_app();
    let out = bind::dispatch::<Demo, App, _>(&mut app, &key("bksp"));
    assert_eq!(out, vec![4]);
    let Layer::Typing(t) = &app.layer else {
        unreachable!()
    };
    assert_eq!(t.hits, 1);
}

#[test]
fn through_box_to_deep() {
    let mut app = typing_app();
    let out = bind::dispatch::<Demo, App, _>(&mut app, &key("d"));
    assert_eq!(out, vec![1]);
    let Layer::Typing(t) = &app.layer else {
        unreachable!()
    };
    assert_eq!(t.deep.hits, 1);
}

#[test]
fn foreground_binding_fires() {
    let mut app = nav_app();
    let out = bind::dispatch::<Demo, App, _>(&mut app, &foreground("Slack"));
    assert_eq!(out, vec![5]);
}

#[test]
fn unbound_event_is_none() {
    let mut app = nav_app();
    let out = bind::dispatch::<Demo, App, _>(&mut app, &key("x"));
    assert_eq!(out, vec![]);
    assert_eq!(app.hits, 0);
    let Layer::Nav(nav) = &app.layer else {
        unreachable!()
    };
    assert_eq!(nav.hits, 0);
}

#[test]
fn binding_on_inactive_variant_is_none() {
    let mut app = typing_app();
    let out = bind::dispatch::<Demo, App, _>(&mut app, &key("g"));
    assert_eq!(out, vec![]);
}

#[test]
fn unmatched_foreground_is_none() {
    let mut app = nav_app();
    let out = bind::dispatch::<Demo, App, _>(&mut app, &foreground("Other"));
    assert_eq!(out, vec![]);
}

#[test]
fn multi_parent_leaf_via_album() {
    let mut media = Media::Album(Album {
        title: Title { hits: 0 },
    });
    let out = bind::dispatch::<Demo, Media, _>(&mut media, &key("t"));
    assert_eq!(out, vec![1]);
    let Media::Album(a) = &media else {
        unreachable!()
    };
    assert_eq!(a.title.hits, 1);
}

#[test]
fn multi_parent_leaf_via_song() {
    let mut media = Media::Song(Song {
        title: Title { hits: 0 },
    });
    let out = bind::dispatch::<Demo, Media, _>(&mut media, &key("t"));
    assert_eq!(out, vec![1]);
    let Media::Song(s) = &media else {
        unreachable!()
    };
    assert_eq!(s.title.hits, 1);
}

#[test]
fn multi_parent_ancestor_recover() {
    let mut media = Media::Album(Album {
        title: Title { hits: 0 },
    });
    let out = bind::dispatch::<Demo, Media, _>(&mut media, &key("a"));
    assert_eq!(out, vec![1]);
    let Media::Album(a) = &media else {
        unreachable!()
    };
    assert_eq!(a.title.hits, 0);
}

#[test]
fn a_closure_trigger_matches_only_what_its_node_waits_for() {
    let mut armed = Armed {
        waiting_for: Some("g"),
        for_child: None,
        child: ArmedChild { wants: None },
    };
    assert_eq!(
        bind::dispatch::<Demo, Armed, _>(&mut armed, &key("g")),
        vec![1]
    );
    assert_eq!(armed.waiting_for, None, "the handler ran and cleared it");
}

#[test]
fn a_closure_trigger_matching_nothing_dispatches_nothing() {
    let mut armed = Armed {
        waiting_for: Some("g"),
        for_child: None,
        child: ArmedChild { wants: None },
    };
    assert_eq!(
        bind::dispatch::<Demo, Armed, _>(&mut armed, &key("h")),
        vec![]
    );
    assert_eq!(armed.waiting_for, Some("g"), "nothing was cleared");
}

#[test]
fn a_node_waiting_for_nothing_matches_nothing() {
    let mut armed = Armed {
        waiting_for: None,
        for_child: None,
        child: ArmedChild { wants: None },
    };
    assert_eq!(
        bind::dispatch::<Demo, Armed, _>(&mut armed, &key("g")),
        vec![]
    );
}

#[test]
fn a_constant_trigger_still_works_beside_a_closure_one() {
    let mut armed = Armed {
        waiting_for: Some("g"),
        for_child: None,
        child: ArmedChild { wants: None },
    };
    assert_eq!(
        bind::dispatch::<Demo, Armed, _>(&mut armed, &key("esc")),
        vec![3]
    );
}

#[test]
fn a_closure_trigger_on_a_deeper_node_reads_through_its_path() {
    let mut armed = Armed {
        waiting_for: None,
        for_child: None,
        child: ArmedChild { wants: Some("z") },
    };
    assert_eq!(
        bind::dispatch::<Demo, Armed, _>(&mut armed, &key("z")),
        vec![1]
    );
    assert_eq!(armed.child.wants, None, "the child's handler ran");
}

#[test]
fn a_closure_trigger_can_read_its_parent() {
    let mut armed = Armed {
        waiting_for: None,
        for_child: Some("up"),
        child: ArmedChild { wants: None },
    };
    assert_eq!(
        bind::dispatch::<Demo, Armed, _>(&mut armed, &key("up")),
        vec![102]
    );
}

#[test]
fn an_absent_option_trigger_matches_nothing() {
    let mut armed = Armed {
        waiting_for: None,
        for_child: None,
        child: ArmedChild { wants: None },
    };
    assert_eq!(
        bind::dispatch::<Demo, Armed, _>(&mut armed, &key("z")),
        vec![]
    );
}

#[test]
fn a_present_option_trigger_matches_its_key() {
    let mut armed = Armed {
        waiting_for: None,
        for_child: None,
        child: ArmedChild { wants: Some("z") },
    };
    assert_eq!(
        bind::dispatch::<Demo, Armed, _>(&mut armed, &key("z")),
        vec![1]
    );
}

/// A leave off a routed node. The parent-side fold matches the live route out of the Up enum; the staying tests never reach that arm.
#[test]
fn a_route_parented_leave_folds_back_through_its_own_route() {
    let mut media = Media::Album(Album {
        title: Title { hits: 0 },
    });
    assert_eq!(
        bind::dispatch::<Demo, Media, _>(&mut media, &key("home")),
        vec![]
    );
    let Media::Album(a) = &media else {
        unreachable!()
    };
    assert_eq!(a.title.hits, 0, "the leave touched nothing on the way out");

    let mut media = Media::Song(Song {
        title: Title { hits: 0 },
    });
    assert_eq!(
        bind::dispatch::<Demo, Media, _>(&mut media, &key("home")),
        vec![]
    );
    let Media::Song(s) = &media else {
        unreachable!()
    };
    assert_eq!(s.title.hits, 0);
}

#[test]
fn a_route_ancestor_does_not_fire_behind_a_leave() {
    let mut media = Media::Album(Album {
        title: Title { hits: 0 },
    });
    assert_eq!(
        bind::dispatch::<Demo, Media, _>(&mut media, &key("home")),
        vec![]
    );
}

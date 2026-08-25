//! Two-phase sync of a mirrored value: [`Synced`], and the token that pairs a read with its
//! placeholder.

/// The placeholder's half of a correlation token. Not `Copy`, not `Clone`, not comparable.
#[derive(Debug)]
pub struct HeldGeneration(u64);

/// The travelling half. Handed to [`Synced::commit`] by reference, because dispatch hands
/// every handler `&E` and an event may be handled more than once.
#[derive(Debug)]
pub struct RidingGeneration(u64);

/// Two riding halves compare equal under `testing` whatever their ids. A rebuilt expected
/// effect cannot know the id.
#[cfg(feature = "testing")]
impl PartialEq for RidingGeneration {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[cfg(feature = "testing")]
impl Eq for RidingGeneration {}

/// A counter on the root. One per model: a per-key counter would restart when the OS reuses
/// a pid or window id, and could pair a zombie read with a fresh placeholder.
#[derive(Default, Debug)]
pub struct GenerationMinter(u64);

impl GenerationMinter {
    /// The placeholder's half, and the half that rides the read effect home.
    pub fn mint(&mut self) -> (HeldGeneration, RidingGeneration) {
        self.0 += 1;
        (HeldGeneration(self.0), RidingGeneration(self.0))
    }
}

/// One value synced from the outside world in two phases.
///
/// The fact empties the entry. The value fills it only if it brings the matching half home.
/// The placeholder is written in the same dispatch that emits the read effect, so a value
/// event cannot exist before its placeholder does.
#[derive(Debug)]
pub enum Synced<V> {
    /// The fact arrived; the matching read is in flight. No current value.
    Pending(HeldGeneration),
    /// What the last landed read answered.
    Known(V),
}

impl<V> Synced<V> {
    /// The old value dies; the placeholder takes its half of the pair.
    pub fn change(&mut self, held: HeldGeneration) {
        *self = Self::Pending(held);
    }

    /// Applied only if the riding half matches the placeholder. A slow read after a newer
    /// fact changes nothing.
    pub fn commit(&mut self, riding: &RidingGeneration, value: V) {
        if matches!(self, Self::Pending(HeldGeneration(held)) if *held == riding.0) {
            *self = Self::Known(value);
        }
    }

    /// The current value, if the sync has one. `Pending` is no value, never the old one.
    #[must_use]
    pub const fn known(&self) -> Option<&V> {
        match self {
            Self::Known(v) => Some(v),
            Self::Pending(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GenerationMinter, Synced};

    #[test]
    fn the_happy_pair_lands() {
        let mut mint = GenerationMinter::default();
        let (held, riding) = mint.mint();
        let mut entry: Synced<u32> = Synced::Pending(held);
        entry.commit(&riding, 7);
        assert_eq!(entry.known(), Some(&7));
    }

    #[test]
    fn a_commit_after_a_newer_fact_changes_nothing() {
        let mut mint = GenerationMinter::default();
        let (first_held, first_riding) = mint.mint();
        let mut entry: Synced<u32> = Synced::Pending(first_held);
        let (second_held, second_riding) = mint.mint();
        entry.change(second_held);
        entry.commit(&first_riding, 7);
        assert_eq!(entry.known(), None);
        entry.commit(&second_riding, 9);
        assert_eq!(entry.known(), Some(&9));
    }

    #[test]
    fn a_foreign_riding_half_does_not_land() {
        let mut mint = GenerationMinter::default();
        let (held, _riding) = mint.mint();
        let (_other_held, other_riding) = mint.mint();
        let mut entry: Synced<u32> = Synced::Pending(held);
        entry.commit(&other_riding, 7);
        assert_eq!(entry.known(), None);
    }

    #[test]
    fn a_late_half_meets_a_known_entry_and_nothing_moves() {
        let mut mint = GenerationMinter::default();
        let (held, riding) = mint.mint();
        let mut entry: Synced<u32> = Synced::Pending(held);
        entry.commit(&riding, 7);
        let (_late_held, late_riding) = mint.mint();
        entry.commit(&late_riding, 8);
        assert_eq!(entry.known(), Some(&7));
    }
}

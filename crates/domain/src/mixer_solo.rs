//! Responsibility: decides what a global-mixer SOLO does to one strip.
//!
//! #1007: a solo silences the strips of its own group (inputs with inputs,
//! outputs with outputs) that are not soloed themselves. Several solos add
//! up; with none left, every strip plays again. The fader is never touched.

/// Whether a strip is silenced by a solo in its group.
pub fn solo_silenced(soloed: bool, group_has_solo: bool) -> bool {
    group_has_solo && !soloed
}

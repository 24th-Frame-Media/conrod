//! What the photographer told the setup wizard: the shoot types they work
//! in, and their own defaults for what Conrod looks for and reads there.
//!
//! It lives in the global settings (`user_profile`) and is read again only
//! when an album is created: the album takes a copy of the defaults for its
//! shoot type (`Settings::user_defaults`), so changing the profile later
//! never moves an album that already exists. The order for any one switch,
//! first present wins: the album's own override, the photographer's default
//! for the type, the type's policy (see `Lanes::resolve_with`).
//!
//! Stored as a JSON object and read tolerantly, like every other setting:
//! unknown keys, unknown shoot types and non-boolean switches are dropped.
//!
//! ```json
//! {
//!   "done": true,
//!   "default_preset": "motorsport-track",
//!   "shoots": ["motorsport-track", "wedding"],
//!   "blur": "strict",
//!   "all": { "moments_rank": false },
//!   "presets": { "motorsport-track": { "faces": true } }
//! }
//! ```

use crate::profile::{Overrides, ShootPreset};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// The blur words a profile may carry (the same four the album setting has).
pub const BLUR_WORDS: [&str; 4] = ["critical", "strict", "moderate", "lenient"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UserProfile {
    /// The wizard ran to its end, or was skipped on purpose. A skipped wizard
    /// is a finished one: it must not open again by itself.
    pub done: bool,
    /// The shoot type new imports start on.
    pub default_preset: Option<ShootPreset>,
    /// Every shoot type the photographer said they work in, most-chosen first.
    pub shoots: Vec<ShootPreset>,
    /// How strict the blur check is for new albums; `None` leaves each shoot
    /// type's own level.
    pub blur: Option<&'static str>,
    /// Switches that apply to every shoot type.
    pub all: Overrides,
    /// Switches for one shoot type; they win over `all`.
    pub presets: BTreeMap<&'static str, Overrides>,
}

fn preset_of(name: &str) -> Option<ShootPreset> {
    // `ShootPreset::parse` falls back to motorsport; only exact names count.
    let wanted = name.trim().to_ascii_lowercase();
    ShootPreset::ALL.into_iter().find(|p| p.name() == wanted)
}

fn switches(value: Option<&Value>) -> Overrides {
    match value {
        Some(Value::Object(map)) => Overrides::from_map(map),
        _ => Overrides::default(),
    }
}

impl UserProfile {
    /// Read the stored object, dropping anything that is not understood.
    pub fn from_map(map: &Map<String, Value>) -> UserProfile {
        let mut shoots = Vec::new();
        if let Some(Value::Array(list)) = map.get("shoots") {
            for preset in list.iter().filter_map(|v| preset_of(v.as_str()?)) {
                if !shoots.contains(&preset) {
                    shoots.push(preset);
                }
            }
        }
        let mut presets = BTreeMap::new();
        if let Some(Value::Object(by_type)) = map.get("presets") {
            for (name, value) in by_type {
                if let Some(preset) = preset_of(name) {
                    let set = switches(Some(value));
                    if !set.is_empty() {
                        presets.insert(preset.name(), set);
                    }
                }
            }
        }
        UserProfile {
            done: map.get("done").and_then(Value::as_bool).unwrap_or(false),
            default_preset: map
                .get("default_preset")
                .and_then(Value::as_str)
                .and_then(preset_of),
            shoots,
            blur: map
                .get("blur")
                .and_then(Value::as_str)
                .and_then(|w| BLUR_WORDS.into_iter().find(|b| *b == w.trim())),
            all: switches(map.get("all")),
            presets,
        }
    }

    /// The normalised object (what is stored and sent to the UI). An empty
    /// profile is an empty object.
    pub fn to_map(&self) -> Map<String, Value> {
        let mut out = Map::new();
        if self.done {
            out.insert("done".into(), Value::Bool(true));
        }
        if let Some(preset) = self.default_preset {
            out.insert("default_preset".into(), preset.name().into());
        }
        if !self.shoots.is_empty() {
            out.insert(
                "shoots".into(),
                self.shoots
                    .iter()
                    .map(|p| p.name())
                    .collect::<Vec<_>>()
                    .into(),
            );
        }
        if let Some(blur) = self.blur {
            out.insert("blur".into(), blur.into());
        }
        if !self.all.is_empty() {
            out.insert("all".into(), Value::Object(self.all.to_map()));
        }
        if !self.presets.is_empty() {
            let by_type: Map<String, Value> = self
                .presets
                .iter()
                .map(|(name, set)| ((*name).to_string(), Value::Object(set.to_map())))
                .collect();
            out.insert("presets".into(), Value::Object(by_type));
        }
        out
    }

    /// Read and re-write: the stored object with everything unknown gone.
    pub fn sanitize(map: &Map<String, Value>) -> Map<String, Value> {
        UserProfile::from_map(map).to_map()
    }

    /// The photographer's switches for one shoot type: what applies to every
    /// type, then what they chose for this one.
    pub fn defaults_for(&self, preset: ShootPreset) -> Overrides {
        let mut merged = self.all.clone();
        if let Some(set) = self.presets.get(preset.name()) {
            for feature in crate::profile::Feature::ALL {
                if let Some(on) = set.get(feature) {
                    merged.set(feature, Some(on));
                }
            }
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Feature, Lanes};
    use crate::settings::Settings;

    fn profile(json: &str) -> UserProfile {
        let Value::Object(map) = serde_json::from_str(json).unwrap() else {
            panic!("an object")
        };
        UserProfile::from_map(&map)
    }

    #[test]
    fn an_empty_or_junk_profile_changes_nothing() {
        assert_eq!(profile("{}"), UserProfile::default());
        let junk = profile(
            r#"{"done":"yes","default_preset":"banquet","shoots":["wedding","nope","wedding"],
                "blur":"extreme","all":{"faces":"on","nonsense":true},
                "presets":{"nope":{"faces":true},"portrait":{"faces":1}}}"#,
        );
        assert!(!junk.done && junk.default_preset.is_none() && junk.blur.is_none());
        assert_eq!(junk.shoots, vec![ShootPreset::Wedding]);
        assert!(junk.all.is_empty() && junk.presets.is_empty());
        assert!(UserProfile::default().to_map().is_empty());
    }

    #[test]
    fn a_profile_round_trips() {
        let p = profile(
            r#"{"done":true,"default_preset":"motorsport-track",
                "shoots":["motorsport-track","wedding"],"blur":"strict",
                "all":{"moments_rank":false},
                "presets":{"motorsport-track":{"faces":true,"numbers":false}}}"#,
        );
        assert!(p.done);
        assert_eq!(p.default_preset, Some(ShootPreset::MotorsportTrack));
        assert_eq!(p.blur, Some("strict"));
        assert_eq!(UserProfile::from_map(&p.to_map()), p);
        assert_eq!(UserProfile::sanitize(&p.to_map()), p.to_map());
    }

    #[test]
    fn a_types_own_switch_wins_over_the_all_types_switch() {
        let p = profile(
            r#"{"all":{"faces":false,"moments_rank":false},
                "presets":{"wedding":{"faces":true}}}"#,
        );
        let wedding = p.defaults_for(ShootPreset::Wedding);
        assert_eq!(wedding.get(Feature::Faces), Some(true));
        assert_eq!(wedding.get(Feature::MomentsRank), Some(false));
        let portrait = p.defaults_for(ShootPreset::Portrait);
        assert_eq!(portrait.get(Feature::Faces), Some(false));
    }

    /// The whole order, on one switch: faces at a track day.
    #[test]
    fn album_override_beats_user_default_beats_preset_policy() {
        use ShootPreset::MotorsportTrack as Track;
        let none = Overrides::default();
        let mut user = Overrides::default();
        user.set(Feature::Faces, Some(true));
        let mut album = Overrides::default();

        // The type's policy: faces are the photographer's choice, so off.
        assert!(!Lanes::resolve_with(Track, false, &none, &none).faces);
        // The photographer's default turns them on...
        let l = Lanes::resolve_with(Track, false, &none, &user);
        assert!(l.faces && !l.overridden(Feature::Faces));
        // ...and the person judging a person frame like a portrait follows.
        assert!(l.people_judged);
        // The album's override wins over the photographer's default, both ways.
        album.set(Feature::Faces, Some(false));
        assert!(!Lanes::resolve_with(Track, false, &album, &user).faces);
        // And the other way round: a default of off, an album that wants them.
        user.set(Feature::Faces, Some(false));
        album.set(Feature::Faces, Some(true));
        assert!(Lanes::resolve_with(Track, false, &album, &user).faces);
        // Clearing the override falls back to the user's default, not the policy.
        album.set(Feature::Faces, None);
        user.set(Feature::Faces, Some(true));
        assert!(Lanes::resolve_with(Track, false, &album, &user).faces);
        // The summary says where clearing goes.
        let s = Lanes::resolve_with(Track, false, &album, &user).summary();
        assert_eq!(s["detects"][2]["default"], true);
        assert_eq!(s["detects"][2]["overridden"], false);
        // The older include_people switch still turns faces on.
        user.set(Feature::Faces, Some(false));
        assert!(Lanes::resolve_with(Track, true, &none, &user).faces);
    }

    #[test]
    fn a_user_default_is_read_through_the_album_settings() {
        let mut settings = Settings {
            scan_profile: "motorsport-rally".into(),
            ..Settings::default()
        };
        assert!(!settings.lanes().faces);
        // What the engine writes into a new album from the global profile.
        let global = profile(r#"{"presets":{"motorsport-rally":{"faces":true}}}"#);
        settings.user_defaults = global.defaults_for(ShootPreset::MotorsportRally).to_map();
        assert!(settings.lanes().faces);
        // The album's own override still wins.
        settings
            .overrides
            .insert("faces".into(), Value::Bool(false));
        assert!(!settings.lanes().faces);
        // The global profile on its own never touches an album that has no copy.
        let old = Settings {
            scan_profile: "motorsport-rally".into(),
            user_profile: global.to_map(),
            ..Settings::default()
        };
        assert!(!old.lanes().faces);
    }
}

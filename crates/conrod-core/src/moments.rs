//! What makes a keeper at each kind of shoot: the moment vocabulary, what a
//! judge read off a frame, and how a clearly visible moment lifts the frame's
//! rank among its burst.
//!
//! The vocabulary is data, one short list per preset (6-12 tags at most), each
//! tag with a plain question for the judge and a weight for the lift. A moment
//! is a tie-break among equally sharp frames: it never rescues a frame the
//! sharpness gate rejected and never lowers a sharp one (see [`counts_for_rank`]).

use crate::profile::ShootPreset;
use serde::{Deserialize, Serialize};

/// One thing worth keeping a frame for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moment {
    /// Stored tag, `snake_case`.
    pub key: &'static str,
    /// Plain name for the UI ("First kiss").
    pub label: &'static str,
    /// The question put to the judge, written for a photograph.
    pub ask: &'static str,
    /// How much a clearly visible moment lifts the frame, 0..=1.
    pub weight: f64,
}

const fn m(key: &'static str, label: &'static str, ask: &'static str, weight: f64) -> Moment {
    Moment {
        key,
        label,
        ask,
        weight,
    }
}

const SMOKE: Moment = m(
    "tyre_smoke",
    "Tyre smoke",
    "Is there clearly visible tyre smoke from a vehicle?",
    0.6,
);
const FLAMES: Moment = m(
    "flames",
    "Flames",
    "Are there flames visible from an exhaust or a vehicle?",
    0.7,
);
const SPARKS: Moment = m(
    "sparks",
    "Sparks",
    "Are there sparks flying from a vehicle?",
    0.6,
);
const WHEEL_LIFT: Moment = m(
    "wheel_lift",
    "Wheel lift",
    "Is a wheel clearly off the ground?",
    0.6,
);
const KERB: Moment = m(
    "kerb_strike",
    "Kerb strike",
    "Is a car clearly riding over a kerb?",
    0.5,
);
const CLOSE: Moment = m(
    "close_racing",
    "Close racing",
    "Are two or more vehicles racing side by side or nose to tail?",
    0.7,
);
const CONTACT: Moment = m(
    "contact",
    "Contact",
    "Are two vehicles, or a vehicle and a barrier, clearly touching?",
    0.9,
);
const CRASH: Moment = m(
    "crash",
    "Crash or spin",
    "Is a vehicle crashing, spinning or clearly out of control?",
    1.0,
);
const AIRBORNE: Moment = m(
    "airborne",
    "Airborne",
    "Is the main subject clearly off the ground?",
    0.9,
);
const CHEQUERED: Moment = m(
    "chequered_flag",
    "Chequered flag",
    "Is a chequered flag being shown?",
    0.7,
);
const PODIUM: Moment = m(
    "podium",
    "Podium",
    "Is there a podium celebration, with trophies or champagne?",
    0.6,
);
const CROWD: Moment = m(
    "crowd_reaction",
    "Crowd reaction",
    "Is a crowd visibly reacting, cheering or watching the action?",
    0.5,
);
const LAUGH: Moment = m("laughter", "Laughter", "Is someone clearly laughing?", 0.8);
const SMILE: Moment = m(
    "genuine_smile",
    "Genuine smile",
    "Is someone smiling in a natural, unposed way?",
    0.8,
);
const EYE: Moment = m(
    "eye_contact",
    "Eye contact",
    "Is the main subject looking straight at the camera?",
    0.6,
);
const CANDID: Moment = m(
    "candid",
    "Candid moment",
    "Is this an unposed moment between people?",
    0.5,
);
const CELEBRATE: Moment = m(
    "celebration",
    "Celebration",
    "Are people celebrating, with raised arms or cheering?",
    0.7,
);
const PEAK_JUMP: Moment = m(
    "peak_jump",
    "Peak of a jump",
    "Is an athlete at the very top of a jump?",
    0.8,
);
const BALL: Moment = m(
    "ball_contact",
    "Ball contact",
    "Is the ball in contact with a player's hand, foot or equipment?",
    0.8,
);

const MOTORSPORT: &[Moment] = &[
    SMOKE, FLAMES, SPARKS, KERB, WHEEL_LIFT, CLOSE, CONTACT, CRASH, CHEQUERED, PODIUM,
];
const TRACK: &[Moment] = &[
    SMOKE,
    SPARKS,
    KERB,
    WHEEL_LIFT,
    CLOSE,
    CONTACT,
    CRASH,
    m(
        "pit_lane",
        "Pit lane work",
        "Is a pit crew visibly working on a car?",
        0.4,
    ),
];
const BURNOUTS: &[Moment] = &[
    m(
        "smoke_cloud",
        "Smoke cloud",
        "Is the vehicle surrounded by a thick cloud of tyre smoke?",
        0.7,
    ),
    FLAMES,
    m(
        "tyre_burst",
        "Tyre burst",
        "Is a tyre visibly shredding or bursting?",
        0.7,
    ),
    SPARKS,
    CROWD,
];
const RALLY: &[Moment] = &[
    AIRBORNE,
    m(
        "dust",
        "Dust",
        "Is there a clearly visible dust cloud behind or around a vehicle?",
        0.6,
    ),
    m(
        "water_splash",
        "Water splash",
        "Is a vehicle throwing up water or mud?",
        0.7,
    ),
    m(
        "sideways",
        "Sideways",
        "Is a car clearly sliding sideways?",
        0.7,
    ),
    WHEEL_LIFT,
    CRASH,
    CROWD,
];
const MEET: &[Moment] = &[
    m(
        "owner_with_car",
        "Owner with car",
        "Is an owner or driver posing or talking next to their car?",
        0.5,
    ),
    m(
        "badge_detail",
        "Badge detail",
        "Is a badge, emblem or lettering the clear subject?",
        0.4,
    ),
    m(
        "engine_bay",
        "Engine bay",
        "Is an open engine bay the clear subject?",
        0.4,
    ),
    m(
        "interior",
        "Interior",
        "Is a car interior the clear subject?",
        0.4,
    ),
    m(
        "wheel_detail",
        "Wheel detail",
        "Is a wheel or brake the clear subject?",
        0.3,
    ),
];
const SHOWS: &[Moment] = &[
    m(
        "performer_peak",
        "Performer at peak",
        "Is a performer at the peak of their act?",
        0.7,
    ),
    CROWD,
    m(
        "animal_in_motion",
        "Animal in motion",
        "Is an animal clearly moving, jumping or running?",
        0.6,
    ),
    m(
        "animal_portrait",
        "Animal portrait",
        "Is an animal looking at the camera?",
        0.6,
    ),
    m(
        "prize_moment",
        "Prize moment",
        "Is a prize, ribbon or trophy being presented?",
        0.5,
    ),
];
const EVENT: &[Moment] = &[
    m(
        "speaker_gesture",
        "Speaker gesture",
        "Is a speaker mid-gesture while addressing people?",
        0.6,
    ),
    CROWD,
    m(
        "presentation",
        "Presentation",
        "Is a handshake, award or presentation happening?",
        0.5,
    ),
    LAUGH,
];
const PARTIES: &[Moment] = &[
    LAUGH,
    m(
        "toast",
        "Toast",
        "Are people raising glasses in a toast?",
        0.6,
    ),
    m("dancing", "Dancing", "Are people dancing?", 0.7),
    m("hug", "Hug", "Are people hugging?", 0.6),
    CANDID,
    m(
        "cake_candles",
        "Cake and candles",
        "Is someone blowing out candles or cutting a cake?",
        0.7,
    ),
];
const WEDDING: &[Moment] = &[
    m("first_kiss", "First kiss", "Are the couple kissing?", 1.0),
    m(
        "ring_exchange",
        "Ring exchange",
        "Are rings being exchanged or placed on fingers?",
        0.9,
    ),
    m(
        "vows",
        "Vows",
        "Is someone reading vows or speaking during the ceremony?",
        0.8,
    ),
    m(
        "first_dance",
        "First dance",
        "Is the couple dancing together?",
        0.8,
    ),
    m(
        "confetti",
        "Confetti",
        "Is confetti, petals or rice in the air?",
        0.7,
    ),
    m(
        "tears_or_laughter",
        "Tears or laughter",
        "Is someone visibly crying with emotion or laughing?",
        0.8,
    ),
    m(
        "bouquet_toss",
        "Bouquet toss",
        "Is a bouquet being thrown or caught?",
        0.7,
    ),
    m(
        "walk_aisle",
        "Walk down the aisle",
        "Is someone walking down the aisle?",
        0.8,
    ),
];
const PORTRAIT: &[Moment] = &[SMILE, LAUGH, EYE, CANDID];
const GROUP: &[Moment] = &[
    m(
        "everyone_looking",
        "Everyone looking",
        "Is every person in the group looking at the camera?",
        0.8,
    ),
    SMILE,
    LAUGH,
];
const COUPLE: &[Moment] = &[
    m("embrace", "Embrace", "Is the couple embracing?", 0.7),
    m(
        "laughing_together",
        "Laughing together",
        "Are the couple laughing together?",
        0.8,
    ),
    EYE,
    m("kiss", "Kiss", "Is the couple kissing?", 0.8),
];
const PETS: &[Moment] = &[
    m(
        "pet_eye_contact",
        "Eye contact",
        "Is the animal looking straight at the camera?",
        0.7,
    ),
    m(
        "playful_action",
        "Playful action",
        "Is the animal playing, jumping or running?",
        0.8,
    ),
    m(
        "happy_face",
        "Happy face",
        "Does the animal look relaxed and happy, for instance with its mouth open?",
        0.5,
    ),
];
const FIELD: &[Moment] = &[
    BALL,
    m(
        "goal_or_try",
        "Goal or try",
        "Is a goal, try or point being scored?",
        1.0,
    ),
    CELEBRATE,
    m(
        "tackle",
        "Tackle",
        "Is a tackle or challenge in progress?",
        0.8,
    ),
    PEAK_JUMP,
];
const COURT: &[Moment] = &[
    m(
        "shot_release",
        "Shot release",
        "Is a player releasing a shot or serve?",
        0.8,
    ),
    m(
        "dunk_or_spike",
        "Dunk or spike",
        "Is a player dunking, spiking or smashing?",
        1.0,
    ),
    CELEBRATE,
    m("block", "Block", "Is a player blocking a shot?", 0.8),
    PEAK_JUMP,
];
const RODEO: &[Moment] = &[
    m(
        "rider_airborne",
        "Rider airborne",
        "Is the rider clearly off the animal or the ground?",
        0.9,
    ),
    m(
        "bucking_peak",
        "Bucking peak",
        "Is the animal at the peak of a buck?",
        1.0,
    ),
    m(
        "dismount_or_fall",
        "Dismount or fall",
        "Is the rider dismounting or falling?",
        0.8,
    ),
    m(
        "clown_action",
        "Rodeo clown",
        "Is a rodeo clown in action?",
        0.5,
    ),
];
const EQUESTRIAN: &[Moment] = &[
    m(
        "jump_apex",
        "Jump apex",
        "Is the horse at the top of a jump?",
        1.0,
    ),
    m(
        "hooves_off_ground",
        "All hooves off ground",
        "Are all four hooves clearly off the ground?",
        0.9,
    ),
    m(
        "landing",
        "Landing",
        "Is the horse landing over a fence?",
        0.6,
    ),
    m(
        "rider_focus",
        "Rider focus",
        "Is the rider looking ahead with clear concentration?",
        0.4,
    ),
];
const CYCLING: &[Moment] = &[
    AIRBORNE,
    m(
        "sprint_finish",
        "Sprint finish",
        "Is a rider sprinting for the line or crossing it?",
        1.0,
    ),
    CRASH,
    m(
        "peloton",
        "Peloton",
        "Is a tight pack of riders in frame?",
        0.5,
    ),
    m(
        "climb_effort",
        "Climbing effort",
        "Is a rider visibly straining on a climb?",
        0.5,
    ),
];
const ACTION: &[Moment] = &[
    AIRBORNE,
    m(
        "trick_peak",
        "Trick at its peak",
        "Is an athlete at the peak of a trick?",
        1.0,
    ),
    m(
        "spray_or_dust",
        "Spray or dust",
        "Is there spray, snow or dust thrown up by the athlete?",
        0.6,
    ),
    CRASH,
    m(
        "grab",
        "Grab",
        "Is the athlete holding their board or bike mid-air?",
        0.7,
    ),
];
const CONCERT: &[Moment] = &[
    m("jump", "Jump", "Is a performer jumping?", 0.8),
    m(
        "crowd_surf",
        "Crowd surfing",
        "Is someone being carried over the crowd?",
        0.8,
    ),
    m(
        "pyro",
        "Pyro",
        "Is there fire, fireworks or confetti cannons on stage?",
        0.9,
    ),
    m(
        "guitar_raised",
        "Guitar raised",
        "Is an instrument held high or swung?",
        0.7,
    ),
    m(
        "singer_crowd",
        "Singer and crowd",
        "Is the singer interacting directly with the crowd?",
        0.7,
    ),
];
const AVIATION: &[Moment] = &[
    m(
        "vapour",
        "Vapour",
        "Are there visible vapour trails or condensation from the aircraft?",
        0.6,
    ),
    m(
        "afterburner",
        "Afterburner",
        "Is an afterburner flame visible?",
        0.9,
    ),
    m(
        "formation",
        "Formation",
        "Are several aircraft flying in formation?",
        0.8,
    ),
    m(
        "gear_down",
        "Gear down",
        "Is landing gear extended in flight?",
        0.5,
    ),
    m(
        "landing_flare",
        "Landing flare",
        "Is an aircraft at the moment of touchdown?",
        0.6,
    ),
    m(
        "knife_edge",
        "Banked hard",
        "Is an aircraft banked steeply or inverted?",
        0.7,
    ),
];
const WILDLIFE: &[Moment] = &[
    m(
        "in_flight",
        "In flight",
        "Is a bird or animal clearly in flight?",
        0.8,
    ),
    m("catch", "Catch", "Is an animal catching prey or food?", 0.9),
    m(
        "animal_eye_contact",
        "Eye contact",
        "Is the animal looking at the camera?",
        0.7,
    ),
    m(
        "wings_apex",
        "Wings at apex",
        "Are a bird's wings at the top of the stroke?",
        0.8,
    ),
    m(
        "interaction",
        "Interaction",
        "Are animals clearly interacting?",
        0.6,
    ),
];
const MIX: &[Moment] = &[AIRBORNE, SMOKE, FLAMES, LAUGH, CELEBRATE, CROWD];
const LANDSCAPE: &[Moment] = &[
    m(
        "golden_hour",
        "Golden hour",
        "Is the scene lit by warm low golden-hour light?",
        0.6,
    ),
    m(
        "dramatic_light",
        "Dramatic light",
        "Is there dramatic light, such as light beams or a stormy break in the clouds?",
        0.7,
    ),
];
const ASTRO: &[Moment] = &[
    m(
        "milky_way_core",
        "Milky Way core",
        "Is the bright core of the Milky Way clearly visible?",
        0.9,
    ),
    m("meteor", "Meteor", "Is a meteor streak visible?", 0.9),
    m("aurora", "Aurora", "Is an aurora visible?", 1.0),
];

/// The moments worth judging at this kind of shoot. Empty: nothing to judge.
pub fn vocabulary(preset: ShootPreset) -> &'static [Moment] {
    use ShootPreset::*;
    match preset {
        Motorsport => MOTORSPORT,
        MotorsportBurnouts => BURNOUTS,
        MotorsportTrack => TRACK,
        MotorsportRally => RALLY,
        MotorsportMeet => MEET,
        Event => EVENT,
        EventShows => SHOWS,
        EventParties => PARTIES,
        Mix => MIX,
        Auto => MIX,
        Portrait => PORTRAIT,
        PortraitGroup => GROUP,
        PortraitCouple => COUPLE,
        PortraitPets => PETS,
        SportField => FIELD,
        SportCourt => COURT,
        SportRodeo => RODEO,
        SportEquestrian => EQUESTRIAN,
        SportCycling => CYCLING,
        SportAction => ACTION,
        PropertyListing | PropertyReport | AutoDealer => &[],
        Wedding => WEDDING,
        Concert => CONCERT,
        WildlifeBirds => WILDLIFE,
        Landscape => LANDSCAPE,
        Astro => ASTRO,
        Aviation => AVIATION,
    }
}

/// Words the vision model used to tag vehicles before the vocabulary, kept
/// readable: `VehicleAnalysis.action` still holds them.
pub const LEGACY_ACTION_TAGS: [&str; 8] = [
    "airborne",
    "dust",
    "tyre_smoke",
    "spray",
    "flames",
    "sparks",
    "contact",
    "wheel_lift",
];

/// One tag a judge reported, with its probability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MomentReading {
    pub tag: String,
    /// P(the moment is visible). A model that only names tags has no real
    /// probability: `calibrated` is false and `p` is a fixed stand-in.
    pub p: f64,
    #[serde(default)]
    pub calibrated: bool,
}

/// What a judge made of one frame; stored on the frame as JSON.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MomentVerdict {
    /// "vlm" or "decider".
    #[serde(default)]
    pub backend: String,
    #[serde(default)]
    pub readings: Vec<MomentReading>,
    /// How strong the best moment is, 0..=1 (the decider's expected level), when judged.
    #[serde(default)]
    pub strength: Option<f64>,
    /// P(this frame is the peak of its burst), set only on the one frame a
    /// burst-peak judgement picked clearly.
    #[serde(default)]
    pub peak: Option<f64>,
}

/// Readings below this are not shown at all.
pub const MAYBE_FLOOR: f64 = 0.5;
/// The default bar a moment must clear before it can affect ranking.
pub const DEFAULT_THRESHOLD: f64 = 0.85;

impl MomentVerdict {
    pub fn from_json(text: &str) -> MomentVerdict {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Readings sure enough to show as moments and to rank on.
    pub fn confident(&self, threshold: f64) -> Vec<&MomentReading> {
        self.readings.iter().filter(|r| r.p >= threshold).collect()
    }

    /// Readings shown as "maybe": plausible, but never affecting ranking.
    pub fn maybe(&self, threshold: f64) -> Vec<&MomentReading> {
        self.readings
            .iter()
            .filter(|r| r.p >= MAYBE_FLOOR && r.p < threshold)
            .collect()
    }
}

/// The moment that lifts a frame, and by how much.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lift {
    pub tag: String,
    pub label: String,
    /// 0..=1: the tag's weight times the judged strength.
    pub amount: f64,
}

/// The strongest confident moment of the frame, or none. `vocab` carries the
/// weights; a tag outside it (an old album, another preset) lifts nothing.
pub fn lift(verdict: &MomentVerdict, vocab: &[Moment], threshold: f64) -> Option<Lift> {
    verdict
        .confident(threshold)
        .into_iter()
        .filter_map(|r| {
            let moment = vocab.iter().find(|v| v.key == r.tag)?;
            let amount = moment.weight * verdict.strength.unwrap_or(1.0).clamp(0.0, 1.0);
            Some(Lift {
                tag: moment.key.to_string(),
                label: moment.label.to_string(),
                amount,
            })
        })
        .max_by(|a, b| a.amount.total_cmp(&b.amount))
}

/// Ranking buckets of a lift: 0 (none) to 4. Coarse on purpose, so near-equal
/// moments tie and the next key (size, framing) still decides.
pub fn bucket(amount: f64) -> i64 {
    (amount.clamp(0.0, 1.0) * 4.0).round() as i64
}

/// Whether a frame's moment may count toward its rank. A moment only ever
/// breaks ties among frames the sharpness gate accepted: it never rescues a
/// frame that gate rejected (blurred, or culled by any rule), and with
/// ranking off it counts for nothing.
pub fn counts_for_rank(rank_on: bool, passes_sharpness_gate: bool, culled: bool) -> bool {
    rank_on && passes_sharpness_gate && !culled
}

/// The preset's moments when the album judges them, else none.
pub fn active(preset: ShootPreset, detect: bool) -> &'static [Moment] {
    if detect {
        vocabulary(preset)
    } else {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(readings: &[(&str, f64)], strength: Option<f64>) -> MomentVerdict {
        MomentVerdict {
            backend: "decider".into(),
            readings: readings
                .iter()
                .map(|(t, p)| MomentReading {
                    tag: (*t).into(),
                    p: *p,
                    calibrated: true,
                })
                .collect(),
            strength,
            peak: None,
        }
    }

    #[test]
    fn every_preset_has_a_short_vocabulary_or_none_on_purpose() {
        for preset in ShootPreset::ALL {
            let v = vocabulary(preset);
            assert!(v.len() <= 12, "{preset:?} has {} moments", v.len());
            let mut keys: Vec<_> = v.iter().map(|m| m.key).collect();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), v.len(), "{preset:?} repeats a tag");
            for moment in v {
                assert!(
                    !moment.label.is_empty() && moment.ask.ends_with('?'),
                    "{}",
                    moment.key
                );
                assert!(
                    (0.0..=1.0).contains(&moment.weight) && moment.weight > 0.0,
                    "{}",
                    moment.key
                );
                assert!(
                    moment
                        .key
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_'),
                    "{}",
                    moment.key
                );
            }
        }
        use ShootPreset::*;
        for none in [PropertyListing, PropertyReport, AutoDealer] {
            assert!(vocabulary(none).is_empty(), "{none:?}");
        }
        for some in ShootPreset::ALL {
            if !matches!(some, PropertyListing | PropertyReport | AutoDealer) {
                assert!(vocabulary(some).len() >= 2, "{some:?}");
            }
        }
    }

    #[test]
    fn no_vehicle_words_leak_into_presets_without_vehicles() {
        let vehicle = [
            "tyre", "tyres", "wheel", "wheels", "kerb", "vehicle", "vehicles", "car", "cars",
            "exhaust", "racing", "pit",
        ];
        for preset in ShootPreset::ALL {
            let lanes = crate::profile::Lanes::defaults(preset);
            if lanes.vehicles {
                continue;
            }
            for moment in vocabulary(preset) {
                let text = format!("{} {}", moment.key, moment.ask).to_lowercase();
                for word in text.split(|c: char| !c.is_ascii_alphabetic()) {
                    assert!(
                        !vehicle.contains(&word),
                        "{preset:?} {}: '{word}'",
                        moment.key
                    );
                }
            }
        }
        // The reverse: a car shoot does ask about its own effects.
        let track: Vec<_> = vocabulary(ShootPreset::MotorsportTrack)
            .iter()
            .map(|m| m.key)
            .collect();
        assert!(track.contains(&"tyre_smoke") && track.contains(&"wheel_lift"));
        let wedding: Vec<_> = vocabulary(ShootPreset::Wedding)
            .iter()
            .map(|m| m.key)
            .collect();
        for tag in [
            "first_kiss",
            "ring_exchange",
            "vows",
            "first_dance",
            "confetti",
            "bouquet_toss",
        ] {
            assert!(wedding.contains(&tag), "{tag}");
        }
    }

    #[test]
    fn only_clearly_confident_moments_count_and_the_rest_are_maybes() {
        let v = verdict(
            &[("first_kiss", 0.95), ("confetti", 0.7), ("vows", 0.2)],
            None,
        );
        assert_eq!(v.confident(0.85).len(), 1);
        assert_eq!(v.maybe(0.85).len(), 1);
        assert_eq!(v.maybe(0.85)[0].tag, "confetti");
        // A lower bar promotes the maybe; a higher one demotes the kiss.
        assert_eq!(v.confident(0.6).len(), 2);
        assert_eq!(v.confident(0.99).len(), 0);
    }

    #[test]
    fn the_lift_is_the_strongest_confident_tag_scaled_by_strength() {
        let vocab = vocabulary(ShootPreset::Wedding);
        let v = verdict(&[("confetti", 0.9), ("first_kiss", 0.9)], Some(1.0));
        let l = lift(&v, vocab, 0.85).unwrap();
        assert_eq!(l.tag, "first_kiss");
        assert_eq!(l.label, "First kiss");
        assert!((l.amount - 1.0).abs() < 1e-9);
        // A weak moment (the judge's own strength) lifts less.
        let weak = verdict(&[("first_kiss", 0.9)], Some(0.25));
        assert!((lift(&weak, vocab, 0.85).unwrap().amount - 0.25).abs() < 1e-9);
        // Below the bar, or outside the vocabulary: nothing.
        assert!(lift(&verdict(&[("first_kiss", 0.8)], None), vocab, 0.85).is_none());
        assert!(lift(&verdict(&[("tyre_smoke", 0.99)], None), vocab, 0.85).is_none());
        assert!(lift(&MomentVerdict::default(), vocab, 0.85).is_none());
    }

    #[test]
    fn a_moment_counts_for_rank_only_on_a_sharp_unculled_frame_with_ranking_on() {
        assert!(counts_for_rank(true, true, false));
        assert!(!counts_for_rank(false, true, false), "ranking off: no lift");
        assert!(
            !counts_for_rank(true, false, false),
            "no rescue past the sharpness gate"
        );
        assert!(
            !counts_for_rank(true, true, true),
            "no rescue of a culled frame"
        );
    }

    #[test]
    fn buckets_are_coarse_and_bounded() {
        assert_eq!(bucket(0.0), 0);
        assert_eq!(bucket(1.0), 4);
        assert_eq!(bucket(7.0), 4);
        assert_eq!(bucket(-1.0), 0);
        assert_eq!(bucket(0.6), bucket(0.62), "near-equal moments tie");
    }

    #[test]
    fn a_verdict_round_trips_and_a_broken_one_is_empty() {
        let v = verdict(&[("jump", 0.9)], Some(0.5));
        assert_eq!(MomentVerdict::from_json(&v.to_json()), v);
        assert_eq!(
            MomentVerdict::from_json("not json"),
            MomentVerdict::default()
        );
    }

    #[test]
    fn legacy_action_tags_are_still_a_known_set() {
        assert_eq!(LEGACY_ACTION_TAGS.len(), 8);
        assert!(LEGACY_ACTION_TAGS.contains(&"tyre_smoke"));
    }
}

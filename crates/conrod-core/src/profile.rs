//! What kind of shoot a scan is, and what that changes.
//!
//! Motorsport is the default and the reason Conrod exists; the others let the
//! same cull work at any event. A profile decides what the detector looks
//! for, which subject's sharpness decides the frame, whether a smeared
//! background can be forgiven as a pan, and whether the identification lane
//! (plates, numbers, the vision model) runs at all.

/// A subject whose sharpness can decide a frame, most specific first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    Eye,
    Face,
    Person,
    Vehicle,
    WholeFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanProfile {
    Motorsport,
    Portrait,
    Event,
    Mix,
}

/// What a shoot's frames group as: a burst is one moment, brackets and focus
/// stacks are one keeper set, a dealer's angle set is one unit per angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupUnit {
    Moment,
    KeepSet,
    PerAngle,
}

/// What the identification lane should try to read from a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VlmTargets {
    /// Race, bib and competition numbers.
    pub numbers: bool,
    /// Registration plates; aviation tail numbers ride the same OCR lane.
    pub plates: bool,
    pub make_model: bool,
    pub sponsors: bool,
    /// Person names: riders, VIPs, graduates.
    pub names: bool,
}

impl VlmTargets {
    pub const fn with_numbers(mut self) -> VlmTargets {
        self.numbers = true;
        self
    }

    pub const fn with_plates(mut self) -> VlmTargets {
        self.plates = true;
        self
    }

    pub const fn with_make_model(mut self) -> VlmTargets {
        self.make_model = true;
        self
    }

    pub const fn with_sponsors(mut self) -> VlmTargets {
        self.sponsors = true;
        self
    }

    pub const fn with_names(mut self) -> VlmTargets {
        self.names = true;
        self
    }

    pub const NONE: VlmTargets = VlmTargets {
        numbers: false,
        plates: false,
        make_model: false,
        sponsors: false,
        names: false,
    };
    /// Competition vehicles: numbers, class and livery.
    pub const RACE: VlmTargets = VlmTargets {
        numbers: true,
        plates: false,
        make_model: true,
        sponsors: true,
        names: false,
    };
    /// Racing with a crew to name: drivers, riders, teams.
    pub const CREW: VlmTargets = VlmTargets {
        numbers: true,
        plates: false,
        make_model: true,
        sponsors: true,
        names: true,
    };
    /// Registered vehicles: plate, make and model.
    pub const PLATES: VlmTargets = VlmTargets {
        numbers: false,
        plates: true,
        make_model: true,
        sponsors: false,
        names: false,
    };
    /// People to name.
    pub const NAMES: VlmTargets = VlmTargets {
        numbers: false,
        plates: false,
        make_model: false,
        sponsors: false,
        names: true,
    };
}

/// Everything a shoot preset changes, decided in one place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresetPolicy {
    pub profile: ScanProfile,
    pub vehicles: bool,
    pub people: bool,
    pub animals: bool,
    /// COCO 32 sports ball counts as context, never as the deciding subject.
    pub ball: bool,
    pub faces_default: bool,
    pub pan_allowed: bool,
    /// "critical", "strict", "moderate" or "lenient"; a default for later
    /// wiring.
    pub default_blur: &'static str,
    pub group: GroupUnit,
    pub vlm: VlmTargets,
    pub min_box_factor: f64,
    /// A floor for `max_subjects`; 0 keeps whatever the album asks for.
    pub max_subjects_floor: usize,
    /// Condition reports: only true duplicates and blurred frames go.
    pub deliver_most: bool,
    /// Level matters (property, interiors, landscape): measure tilt and
    /// convergence, and flag rather than cull.
    pub level: bool,
    /// Dark frames are the subject here (night sky, a stage in blackout): a
    /// low mean luma is neither a cull nor a ranking fault.
    pub dark_ok: bool,
    /// COCO classes the vehicles lane asks the detector for, when the
    /// subject is not cars, bikes and trucks (4 airplane, 1 bicycle). Empty
    /// keeps the album's own car / bike / truck switches.
    pub vehicle_classes: &'static [usize],
    /// The competitors are people (field and court sport): race numbers are
    /// read off the people, as jersey numbers, instead of off a vehicle.
    pub players: bool,
}

impl PresetPolicy {
    const fn new(profile: ScanProfile) -> PresetPolicy {
        PresetPolicy {
            profile,
            vehicles: false,
            people: true,
            animals: false,
            ball: false,
            faces_default: true,
            pan_allowed: false,
            default_blur: "moderate",
            group: GroupUnit::Moment,
            vlm: VlmTargets::NONE,
            min_box_factor: 1.0,
            max_subjects_floor: 0,
            deliver_most: false,
            level: false,
            dark_ok: false,
            vehicle_classes: &[],
            players: false,
        }
    }

    const fn players(mut self) -> Self {
        self.players = true;
        self
    }

    const fn vehicles(mut self) -> Self {
        self.vehicles = true;
        self
    }

    const fn no_people(mut self) -> Self {
        self.people = false;
        self
    }

    const fn animals(mut self) -> Self {
        self.animals = true;
        self
    }

    const fn ball(mut self) -> Self {
        self.ball = true;
        self
    }

    const fn no_faces(mut self) -> Self {
        self.faces_default = false;
        self
    }

    const fn pan(mut self) -> Self {
        self.pan_allowed = true;
        self
    }

    const fn blur(mut self, default_blur: &'static str) -> Self {
        self.default_blur = default_blur;
        self
    }

    const fn group(mut self, group: GroupUnit) -> Self {
        self.group = group;
        self
    }

    const fn vlm(mut self, vlm: VlmTargets) -> Self {
        self.vlm = vlm;
        self
    }

    const fn min_box(mut self, factor: f64) -> Self {
        self.min_box_factor = factor;
        self
    }

    const fn floor(mut self, subjects: usize) -> Self {
        self.max_subjects_floor = subjects;
        self
    }

    const fn deliver_most(mut self) -> Self {
        self.deliver_most = true;
        self
    }

    const fn level(mut self) -> Self {
        self.level = true;
        self
    }

    const fn dark_ok(mut self) -> Self {
        self.dark_ok = true;
        self
    }

    const fn classes(mut self, classes: &'static [usize]) -> Self {
        self.vehicle_classes = classes;
        self
    }
}

/// A concrete kind of shoot. The parent profile supplies the broad culling
/// behaviour; a child preset only changes the few defaults that benefit from
/// more context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShootPreset {
    Portrait,
    PortraitGroup,
    PortraitCouple,
    PortraitPets,
    Motorsport,
    MotorsportBurnouts,
    MotorsportTrack,
    MotorsportRally,
    MotorsportMeet,
    Event,
    EventShows,
    EventParties,
    Mix,
    SportField,
    SportCourt,
    SportRodeo,
    SportEquestrian,
    SportCycling,
    SportAction,
    PropertyListing,
    PropertyReport,
    AutoDealer,
    Wedding,
    Concert,
    WildlifeBirds,
    Landscape,
    Astro,
    Aviation,
}

impl ShootPreset {
    pub const ALL: [ShootPreset; 28] = [
        ShootPreset::Portrait,
        ShootPreset::PortraitGroup,
        ShootPreset::PortraitCouple,
        ShootPreset::PortraitPets,
        ShootPreset::Motorsport,
        ShootPreset::MotorsportBurnouts,
        ShootPreset::MotorsportTrack,
        ShootPreset::MotorsportRally,
        ShootPreset::MotorsportMeet,
        ShootPreset::Event,
        ShootPreset::EventShows,
        ShootPreset::EventParties,
        ShootPreset::Mix,
        ShootPreset::SportField,
        ShootPreset::SportCourt,
        ShootPreset::SportRodeo,
        ShootPreset::SportEquestrian,
        ShootPreset::SportCycling,
        ShootPreset::SportAction,
        ShootPreset::PropertyListing,
        ShootPreset::PropertyReport,
        ShootPreset::AutoDealer,
        ShootPreset::Wedding,
        ShootPreset::Concert,
        ShootPreset::WildlifeBirds,
        ShootPreset::Landscape,
        ShootPreset::Astro,
        ShootPreset::Aviation,
    ];

    pub fn parse(name: &str) -> ShootPreset {
        match name.trim().to_ascii_lowercase().as_str() {
            "portrait" => ShootPreset::Portrait,
            "portrait-group" => ShootPreset::PortraitGroup,
            "portrait-couple" => ShootPreset::PortraitCouple,
            "portrait-pets" => ShootPreset::PortraitPets,
            "motorsport" => ShootPreset::Motorsport,
            "motorsport-burnouts" => ShootPreset::MotorsportBurnouts,
            "motorsport-track" => ShootPreset::MotorsportTrack,
            "motorsport-rally" => ShootPreset::MotorsportRally,
            "motorsport-meet" => ShootPreset::MotorsportMeet,
            "event" => ShootPreset::Event,
            "event-shows" => ShootPreset::EventShows,
            "event-parties" => ShootPreset::EventParties,
            "mix" => ShootPreset::Mix,
            "sport-field" => ShootPreset::SportField,
            "sport-court" => ShootPreset::SportCourt,
            "sport-rodeo" => ShootPreset::SportRodeo,
            "sport-equestrian" => ShootPreset::SportEquestrian,
            "sport-cycling" => ShootPreset::SportCycling,
            "sport-action" => ShootPreset::SportAction,
            "property-listing" => ShootPreset::PropertyListing,
            "property-report" => ShootPreset::PropertyReport,
            "auto-dealer" => ShootPreset::AutoDealer,
            "wedding" => ShootPreset::Wedding,
            "concert" => ShootPreset::Concert,
            "wildlife-birds" => ShootPreset::WildlifeBirds,
            "landscape" => ShootPreset::Landscape,
            "astro" => ShootPreset::Astro,
            "aviation" => ShootPreset::Aviation,
            _ => ShootPreset::Motorsport,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ShootPreset::Portrait => "portrait",
            ShootPreset::PortraitGroup => "portrait-group",
            ShootPreset::PortraitCouple => "portrait-couple",
            ShootPreset::PortraitPets => "portrait-pets",
            ShootPreset::Motorsport => "motorsport",
            ShootPreset::MotorsportBurnouts => "motorsport-burnouts",
            ShootPreset::MotorsportTrack => "motorsport-track",
            ShootPreset::MotorsportRally => "motorsport-rally",
            ShootPreset::MotorsportMeet => "motorsport-meet",
            ShootPreset::Event => "event",
            ShootPreset::EventShows => "event-shows",
            ShootPreset::EventParties => "event-parties",
            ShootPreset::Mix => "mix",
            ShootPreset::SportField => "sport-field",
            ShootPreset::SportCourt => "sport-court",
            ShootPreset::SportRodeo => "sport-rodeo",
            ShootPreset::SportEquestrian => "sport-equestrian",
            ShootPreset::SportCycling => "sport-cycling",
            ShootPreset::SportAction => "sport-action",
            ShootPreset::PropertyListing => "property-listing",
            ShootPreset::PropertyReport => "property-report",
            ShootPreset::AutoDealer => "auto-dealer",
            ShootPreset::Wedding => "wedding",
            ShootPreset::Concert => "concert",
            ShootPreset::WildlifeBirds => "wildlife-birds",
            ShootPreset::Landscape => "landscape",
            ShootPreset::Astro => "astro",
            ShootPreset::Aviation => "aviation",
        }
    }

    pub fn profile(self) -> ScanProfile {
        self.policy().profile
    }

    pub fn label(self) -> &'static str {
        match self {
            ShootPreset::Portrait => "Portraits",
            ShootPreset::PortraitGroup => "Group photos",
            ShootPreset::PortraitCouple => "Couple photos",
            ShootPreset::PortraitPets => "Pets",
            ShootPreset::Motorsport => "Motorsport",
            ShootPreset::MotorsportBurnouts => "Burnouts",
            ShootPreset::MotorsportTrack => "Track days",
            ShootPreset::MotorsportRally => "Rallies",
            ShootPreset::MotorsportMeet => "Car meets",
            ShootPreset::Event => "Events",
            ShootPreset::EventShows => "Shows",
            ShootPreset::EventParties => "Parties",
            ShootPreset::Mix => "Mixed",
            ShootPreset::SportField => "Field sports",
            ShootPreset::SportCourt => "Court sports",
            ShootPreset::SportRodeo => "Rodeo",
            ShootPreset::SportEquestrian => "Equestrian",
            ShootPreset::SportCycling => "Cycling",
            ShootPreset::SportAction => "Action sports",
            ShootPreset::PropertyListing => "Property listing",
            ShootPreset::PropertyReport => "Condition report",
            ShootPreset::AutoDealer => "Dealership stock",
            ShootPreset::Wedding => "Weddings",
            ShootPreset::Concert => "Concerts",
            ShootPreset::WildlifeBirds => "Birds",
            ShootPreset::Landscape => "Landscape",
            ShootPreset::Astro => "Astro",
            ShootPreset::Aviation => "Aviation",
        }
    }

    /// Concise attention guidance for a vision-language model. It deliberately
    /// contains no sample values or scenarios for the model to imitate.
    pub fn prompt_context(self) -> &'static str {
        match self {
            ShootPreset::Portrait => "Portraits: prioritise people, faces, expression, and eye focus.",
            ShootPreset::PortraitGroup => {
                "Group photos: inspect every visible person and group-wide focus."
            }
            ShootPreset::PortraitCouple => {
                "Couple photos: prioritise both people, their expressions, and interaction."
            }
            ShootPreset::PortraitPets => {
                "Pets: prioritise the animal's face, eyes, pose, and interaction."
            }
            ShootPreset::Motorsport => {
                "Motorsport: capture vehicle identity, competition details, livery, and action."
            }
            ShootPreset::MotorsportBurnouts => {
                "Burnouts: capture vehicle identity, tyre smoke, motion, and visible livery."
            }
            ShootPreset::MotorsportTrack => {
                "Track days: capture vehicle identity, competition details, livery, and motion."
            }
            ShootPreset::MotorsportRally => {
                "Rallies: capture vehicle identity, competition details, livery, and terrain action."
            }
            ShootPreset::MotorsportMeet => {
                "Car meets: capture vehicle identity, modifications, finish, and visible details."
            }
            ShootPreset::Event => {
                "Events: capture the main people, subjects, setting, and activity."
            }
            ShootPreset::EventShows => {
                "Shows: capture performers, presentation, stage detail, and audience context."
            }
            ShootPreset::EventParties => {
                "Parties: capture people, expressions, interaction, and atmosphere."
            }
            ShootPreset::Mix => {
                "Mixed shoot: inspect all visible subjects without assuming a dominant type."
            }
            ShootPreset::SportField => {
                "Field sports: capture the ball, the contest, and the players' effort."
            }
            ShootPreset::SportCourt => {
                "Court sports: capture the ball, the shot, and the players' effort."
            }
            ShootPreset::SportRodeo => {
                "Rodeo: capture the rider and animal at the peak of the ride."
            }
            ShootPreset::SportEquestrian => {
                "Equestrian: capture horse and rider over the fence and at the peak."
            }
            ShootPreset::SportCycling => {
                "Cycling: capture the rider, the machine, and the pace of the race."
            }
            ShootPreset::SportAction => {
                "Action sports: capture the athlete, board or bike, and the peak of the trick."
            }
            ShootPreset::PropertyListing => {
                "Property listing: capture each room square, bright, and uncluttered."
            }
            ShootPreset::PropertyReport => {
                "Condition report: capture each defect wide and close, legible and dated."
            }
            ShootPreset::AutoDealer => {
                "Dealership stock: capture each vehicle's angles, condition, and details."
            }
            ShootPreset::Wedding => {
                "Weddings: capture the couple, the vows, and the guests' expressions."
            }
            ShootPreset::Concert => {
                "Concerts: capture performers, peak gestures, and stage light."
            }
            ShootPreset::WildlifeBirds => {
                "Birds: capture the eye, the wings at the apex, and the head toward the lens."
            }
            ShootPreset::Landscape => {
                "Landscape: capture a level horizon, a sharp foreground, and an unblown sky."
            }
            ShootPreset::Astro => {
                "Astro: capture sharp stars, a clean core, and the intended exposure."
            }
            ShootPreset::Aviation => {
                "Aviation: capture the aircraft's attitude, type, and tail registration."
            }
        }
    }

    /// The one table: everything a preset changes about a scan. Each row
    /// is what the shoot's subjects need and nothing else; the photographer
    /// can override any detector or reader per album (`Lanes`).
    pub fn policy(self) -> PresetPolicy {
        use ScanProfile::{Event, Mix, Motorsport, Portrait};
        let none = VlmTargets::NONE;
        match self {
            ShootPreset::Portrait => PresetPolicy::new(Portrait).animals().blur("strict"),
            ShootPreset::PortraitGroup => PresetPolicy::new(Portrait)
                .animals()
                .blur("strict")
                .min_box(0.5)
                .floor(24),
            ShootPreset::PortraitCouple => PresetPolicy::new(Portrait).animals().blur("strict"),
            ShootPreset::PortraitPets => PresetPolicy::new(Portrait).animals().blur("strict"),
            // Circuit racing: the car decides; numbers and livery identify it;
            // cars race without plates and drivers sit behind helmets.
            ShootPreset::Motorsport => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_faces()
                .pan()
                .blur("critical")
                .vlm(VlmTargets::RACE),
            // Burnout comps: numbered entries on street-registered cars; the
            // crowd is part of the picture (people stay on). Faces are the
            // photographer's choice: off until asked for.
            ShootPreset::MotorsportBurnouts => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_faces()
                .pan()
                .blur("lenient")
                .vlm(none.with_numbers().with_plates().with_make_model()),
            // Track days: road cars (plates) with taped numbers, plus pit lane,
            // paddock, drivers and crew (people stay on; faces are the
            // photographer's choice).
            ShootPreset::MotorsportTrack => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_faces()
                .pan()
                .blur("critical")
                .min_box(0.7)
                .vlm(VlmTargets::CREW.with_plates()),
            // Rally: door numbers, plated cars, crews, service park, spectators
            // (people stay on; faces are the photographer's choice).
            ShootPreset::MotorsportRally => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_faces()
                .pan()
                .blur("lenient")
                .min_box(0.7)
                .vlm(VlmTargets::CREW.with_plates()),
            // Meets and shows: plates and make/model; no race numbers; owners.
            ShootPreset::MotorsportMeet => PresetPolicy::new(Motorsport)
                .vehicles()
                .floor(16)
                .vlm(VlmTargets::PLATES),
            ShootPreset::Event => PresetPolicy::new(Event)
                .animals()
                .pan()
                .vlm(VlmTargets::NAMES),
            // Agricultural and stage shows: animals, riders, performers, crowd.
            ShootPreset::EventShows => PresetPolicy::new(Event)
                .animals()
                .pan()
                .blur("lenient")
                .min_box(0.5)
                .floor(24)
                .dark_ok()
                .vlm(VlmTargets::NAMES),
            ShootPreset::EventParties => PresetPolicy::new(Event)
                .min_box(0.5)
                .floor(24)
                .vlm(VlmTargets::NAMES),
            // Unknown mix: look for everything, read everything.
            ShootPreset::Mix => PresetPolicy::new(Mix)
                .vehicles()
                .animals()
                .pan()
                .vlm(VlmTargets::CREW.with_plates()),
            // Players and the ball; jersey numbers are read off the players.
            ShootPreset::SportField => PresetPolicy::new(Event)
                .ball()
                .no_faces()
                .players()
                .min_box(0.7)
                .floor(24)
                .vlm(none.with_numbers()),
            ShootPreset::SportCourt => PresetPolicy::new(Event)
                .ball()
                .players()
                .pan()
                .blur("strict")
                .vlm(VlmTargets::NAMES.with_numbers()),
            ShootPreset::SportRodeo => PresetPolicy::new(Event)
                .animals()
                .min_box(0.7)
                .vlm(VlmTargets::NAMES),
            ShootPreset::SportEquestrian => PresetPolicy::new(Event)
                .animals()
                .pan()
                .min_box(0.7)
                .vlm(VlmTargets::NAMES),
            // The bicycle is the vehicle here; riders hide behind helmets.
            ShootPreset::SportCycling => PresetPolicy::new(Motorsport)
                .vehicles()
                .classes(&[1])
                .no_faces()
                .pan()
                .min_box(0.7)
                .vlm(none.with_numbers()),
            ShootPreset::SportAction => PresetPolicy::new(Motorsport)
                .vehicles()
                .classes(&[1, 3])
                .no_faces()
                .pan()
                .vlm(none.with_numbers()),
            ShootPreset::PropertyListing => PresetPolicy::new(Mix)
                .no_people()
                .no_faces()
                .blur("strict")
                .group(GroupUnit::KeepSet)
                .level(),
            ShootPreset::PropertyReport => PresetPolicy::new(Mix)
                .no_people()
                .no_faces()
                .blur("strict")
                .group(GroupUnit::KeepSet)
                .deliver_most()
                .level(),
            ShootPreset::AutoDealer => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_people()
                .no_faces()
                .group(GroupUnit::PerAngle)
                .vlm(VlmTargets::PLATES),
            ShootPreset::Wedding => PresetPolicy::new(Event)
                .min_box(0.7)
                .floor(24)
                .vlm(VlmTargets::NAMES),
            ShootPreset::Concert => PresetPolicy::new(Event)
                .pan()
                .blur("lenient")
                .min_box(0.5)
                .floor(24)
                .dark_ok()
                .vlm(VlmTargets::NAMES),
            ShootPreset::WildlifeBirds => PresetPolicy::new(Mix)
                .animals()
                .no_people()
                .no_faces()
                .pan()
                .min_box(0.5),
            ShootPreset::Landscape => PresetPolicy::new(Mix)
                .no_people()
                .no_faces()
                .blur("strict")
                .group(GroupUnit::KeepSet)
                .deliver_most()
                .level(),
            ShootPreset::Astro => PresetPolicy::new(Mix)
                .no_people()
                .no_faces()
                .blur("lenient")
                .dark_ok()
                .group(GroupUnit::KeepSet),
            // Aircraft only; registrations ride the plate lane, type the
            // make/model lane, the operator the livery lane.
            ShootPreset::Aviation => PresetPolicy::new(Motorsport)
                .vehicles()
                .classes(&[4])
                .no_people()
                .no_faces()
                .pan()
                .min_box(0.5)
                .vlm(none.with_plates().with_make_model().with_sponsors()),
        }
    }

    pub fn priority(self) -> &'static [Subject] {
        self.profile().priority()
    }

    pub fn pan_compatible(self) -> bool {
        self.policy().pan_allowed
    }

    pub fn min_box_fraction(self, baseline: f64) -> f64 {
        baseline * self.policy().min_box_factor
    }

    pub fn max_subjects(self, baseline: usize) -> usize {
        baseline.max(self.policy().max_subjects_floor)
    }

    /// How a shoot's frames group; the cull does not use this yet.
    pub fn group_unit(self) -> GroupUnit {
        self.policy().group
    }

    /// What the identification lane should read; exposed for later wiring.
    pub fn vlm_targets(self) -> VlmTargets {
        self.policy().vlm
    }

    /// The preset's own blur default, a fallback for the global setting.
    pub fn default_blur_strictness(self) -> &'static str {
        self.policy().default_blur
    }

    /// Condition reports keep everything but duplicates and blurred frames.
    pub fn deliver_most(self) -> bool {
        self.policy().deliver_most
    }
}

/// One thing Conrod can look for or read, switchable per album in both
/// directions. A preset sets each one's default; an album override wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    Vehicles,
    People,
    Faces,
    Animals,
    Numbers,
    Plates,
    MakeModel,
    Sponsors,
    Names,
    /// Ask a judge for the shoot's moments and show them as tags.
    MomentsDetect,
    /// Let confident moments lift a frame's rank among its burst.
    MomentsRank,
}

impl Feature {
    pub const ALL: [Feature; 11] = [
        Feature::Vehicles,
        Feature::People,
        Feature::Faces,
        Feature::Animals,
        Feature::Numbers,
        Feature::Plates,
        Feature::MakeModel,
        Feature::Sponsors,
        Feature::Names,
        Feature::MomentsDetect,
        Feature::MomentsRank,
    ];
    /// What the detector looks for.
    pub const DETECTS: [Feature; 4] = [
        Feature::Vehicles,
        Feature::People,
        Feature::Faces,
        Feature::Animals,
    ];
    /// What the identification lane reads.
    pub const READS: [Feature; 5] = [
        Feature::Numbers,
        Feature::Plates,
        Feature::MakeModel,
        Feature::Sponsors,
        Feature::Names,
    ];

    /// The moment judge's two switches.
    pub const MOMENTS: [Feature; 2] = [Feature::MomentsDetect, Feature::MomentsRank];

    /// The key stored in the album's `overrides` and sent over the wire.
    pub fn key(self) -> &'static str {
        match self {
            Feature::Vehicles => "vehicles",
            Feature::People => "people",
            Feature::Faces => "faces",
            Feature::Animals => "animals",
            Feature::Numbers => "numbers",
            Feature::Plates => "plates",
            Feature::MakeModel => "make_model",
            Feature::Sponsors => "sponsors",
            Feature::Names => "names",
            Feature::MomentsDetect => "moments_detect",
            Feature::MomentsRank => "moments_rank",
        }
    }

    pub fn parse(key: &str) -> Option<Feature> {
        Feature::ALL.into_iter().find(|f| f.key() == key.trim())
    }

    /// Plain photographer wording, in the shoot's own terms.
    pub fn label(self, preset: ShootPreset) -> &'static str {
        use ShootPreset::{Aviation, SportAction, SportCourt, SportCycling, SportField};
        match (self, preset) {
            (Feature::Numbers, SportField | SportCourt) => "Jersey numbers",
            (Feature::Vehicles, Aviation) => "Aircraft",
            (Feature::Vehicles, SportCycling) => "Bicycles",
            (Feature::Vehicles, SportAction) => "Bikes",
            (Feature::Vehicles, _) => "Cars & bikes",
            (Feature::People, _) => "People",
            (Feature::Faces, _) => "Faces & eyes",
            (Feature::Animals, _) => "Animals",
            (Feature::Numbers, _) => "Race numbers",
            (Feature::Plates, Aviation) => "Registrations",
            (Feature::Plates, _) => "Number plates",
            (Feature::MakeModel, Aviation) => "Aircraft type",
            (Feature::MakeModel, _) => "Make & model",
            (Feature::Sponsors, Aviation) => "Operator & livery",
            (Feature::Sponsors, _) => "Sponsors & livery",
            (Feature::Names, _) => "Names (from faces)",
            (Feature::MomentsDetect, _) => "Spot moments",
            (Feature::MomentsRank, _) => "Let moments lift ranking",
        }
    }

    /// The feature this one cannot work without: readers work on vehicle
    /// crops, names work on faces.
    pub fn requires(self) -> Option<Feature> {
        match self {
            Feature::Numbers | Feature::Plates | Feature::MakeModel | Feature::Sponsors => {
                Some(Feature::Vehicles)
            }
            Feature::Names => Some(Feature::Faces),
            Feature::MomentsRank => Some(Feature::MomentsDetect),
            _ => None,
        }
    }

    /// [`Feature::requires`] for one shoot type: on a players' type the
    /// numbers are jersey numbers, read off the people.
    pub fn requires_for(self, preset: ShootPreset) -> Option<Feature> {
        match self {
            Feature::Numbers if preset.policy().players => Some(Feature::People),
            _ => self.requires(),
        }
    }
}

/// An album's own on/off choices; a missing feature follows the preset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides(std::collections::BTreeMap<Feature, bool>);

impl Overrides {
    /// Read the stored `overrides` object; unknown keys and non-boolean
    /// values are ignored rather than failing the album.
    pub fn from_map(map: &serde_json::Map<String, serde_json::Value>) -> Overrides {
        Overrides(
            map.iter()
                .filter_map(|(k, v)| Some((Feature::parse(k)?, v.as_bool()?)))
                .collect(),
        )
    }

    pub fn to_map(&self) -> serde_json::Map<String, serde_json::Value> {
        self.0
            .iter()
            .map(|(f, on)| (f.key().to_string(), serde_json::Value::Bool(*on)))
            .collect()
    }

    pub fn get(&self, feature: Feature) -> Option<bool> {
        self.0.get(&feature).copied()
    }

    /// `None` clears the override, back to the preset's default.
    pub fn set(&mut self, feature: Feature, on: Option<bool>) {
        match on {
            Some(on) => {
                self.0.insert(feature, on);
            }
            None => {
                self.0.remove(&feature);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// What a preset's policy says about a feature before any override.
fn policy_default(
    preset: ShootPreset,
    p: &PresetPolicy,
    include_people: bool,
    feature: Feature,
) -> bool {
    match feature {
        Feature::Vehicles => p.vehicles,
        Feature::People => p.people,
        Feature::Faces => include_people || p.faces_default,
        Feature::Animals => p.animals,
        Feature::Numbers => p.vlm.numbers,
        Feature::Plates => p.vlm.plates,
        Feature::MakeModel => p.vlm.make_model,
        Feature::Sponsors => p.vlm.sponsors,
        Feature::Names => p.vlm.names,
        // On wherever the type has moments worth judging.
        Feature::MomentsDetect | Feature::MomentsRank => {
            !crate::moments::vocabulary(preset).is_empty()
        }
    }
}

/// What an album actually looks for and reads: the preset's policy with
/// the album's overrides applied. Every lane in the engine asks this, never
/// the preset table directly.
#[derive(Debug, Clone, PartialEq)]
pub struct Lanes {
    pub preset: ShootPreset,
    pub vehicles: bool,
    pub people: bool,
    pub faces: bool,
    pub animals: bool,
    /// Race numbers; need `vehicles` (or, on field and court sport, `people`:
    /// jersey numbers).
    pub numbers: bool,
    pub plates: bool,
    pub make_model: bool,
    pub sponsors: bool,
    /// Name suggestions from faces; needs `faces`.
    pub names: bool,
    /// A judge is asked for the preset's moments (needs a vocabulary).
    pub moments_detect: bool,
    /// Confident moments break ties among equally sharp frames.
    pub moments_rank: bool,
    /// The probability a moment must reach to count (the album's setting).
    pub moments_threshold: f64,
    /// Frames whose main subject is a person are judged like portraits
    /// (eye, face, person) even on a vehicle-first shoot.
    pub people_judged: bool,
    /// COCO classes the vehicles lane asks for; empty = the album's own
    /// car / bike / truck switches.
    pub vehicle_classes: &'static [usize],
    /// The type's competitors are people: numbers are jersey numbers.
    players: bool,
    overrides: Overrides,
    user: Overrides,
}

impl Lanes {
    /// `include_people` is the older motorsport-only "also look at people"
    /// switch; it only changes the default for faces.
    pub fn resolve(preset: ShootPreset, include_people: bool, overrides: &Overrides) -> Lanes {
        Lanes::resolve_with(preset, include_people, overrides, &Overrides::default())
    }

    /// [`Lanes::resolve`] with the photographer's own defaults for this shoot
    /// type (`user`, from the setup wizard). The order, first present wins:
    /// the album's override, the older `include_people` switch (faces only),
    /// the photographer's default for the type, the type's own policy.
    pub fn resolve_with(
        preset: ShootPreset,
        include_people: bool,
        overrides: &Overrides,
        user: &Overrides,
    ) -> Lanes {
        let p = preset.policy();
        let raw = |f: Feature| {
            overrides
                .get(f)
                .or((f == Feature::Faces && include_people).then_some(true))
                .or_else(|| user.get(f))
                .unwrap_or_else(|| policy_default(preset, &p, include_people, f))
        };
        let vehicles = raw(Feature::Vehicles);
        let people = raw(Feature::People);
        let faces = raw(Feature::Faces);
        let moments_detect =
            raw(Feature::MomentsDetect) && !crate::moments::vocabulary(preset).is_empty();
        let explicit_people = overrides.get(Feature::Faces) == Some(true)
            || overrides.get(Feature::People) == Some(true);
        let faces_by_type = user.get(Feature::Faces).unwrap_or(p.faces_default);
        // Numbers are read off a vehicle; on a players' type, off the people.
        let numbers_subject = vehicles || (p.players && people);
        Lanes {
            preset,
            vehicles,
            people,
            faces,
            animals: raw(Feature::Animals),
            numbers: raw(Feature::Numbers) && numbers_subject,
            plates: raw(Feature::Plates) && vehicles,
            make_model: raw(Feature::MakeModel) && vehicles,
            sponsors: raw(Feature::Sponsors) && vehicles,
            names: raw(Feature::Names) && faces,
            moments_detect,
            moments_rank: raw(Feature::MomentsRank) && moments_detect,
            moments_threshold: crate::moments::DEFAULT_THRESHOLD,
            people_judged: faces
                && (include_people
                    || explicit_people
                    || (p.profile == ScanProfile::Motorsport && faces_by_type)),
            vehicle_classes: p.vehicle_classes,
            players: p.players,
            overrides: overrides.clone(),
            user: user.clone(),
        }
    }

    /// The preset's own defaults.
    pub fn defaults(preset: ShootPreset) -> Lanes {
        Lanes::resolve(preset, false, &Overrides::default())
    }

    pub fn on(&self, feature: Feature) -> bool {
        match feature {
            Feature::Vehicles => self.vehicles,
            Feature::People => self.people,
            Feature::Faces => self.faces,
            Feature::Animals => self.animals,
            Feature::Numbers => self.numbers,
            Feature::Plates => self.plates,
            Feature::MakeModel => self.make_model,
            Feature::Sponsors => self.sponsors,
            Feature::Names => self.names,
            Feature::MomentsDetect => self.moments_detect,
            Feature::MomentsRank => self.moments_rank,
        }
    }

    /// The identification lane has a vehicle to work on and something to
    /// read from it.
    pub fn identifies_vehicles(&self) -> bool {
        self.vehicles && (self.numbers || self.plates || self.make_model || self.sponsors)
    }

    /// Jersey numbers are read off the people: a players' type, numbers on,
    /// no vehicle to read instead.
    pub fn reads_players(&self) -> bool {
        self.players && self.people && self.numbers && !self.vehicles
    }

    /// The identification lane has a subject to work on: a vehicle to read,
    /// or players' jerseys.
    pub fn identifies(&self) -> bool {
        self.identifies_vehicles() || self.reads_players()
    }

    /// The vision model has something to ask about a subject.
    pub fn asks_model(&self) -> bool {
        self.identifies()
    }

    /// The moments this album asks a judge for.
    pub fn moments(&self) -> &'static [crate::moments::Moment] {
        crate::moments::active(self.preset, self.moments_detect)
    }

    /// First present wins. A switched-off detector's subject kinds leave
    /// the list, so a frame falls back to the next kind -- animals are
    /// stored as "person" boxes and keep that kind alive.
    pub fn frame_priority(&self, main: Option<Subject>) -> Vec<Subject> {
        let profile = self.preset.profile();
        let base = if profile == ScanProfile::Motorsport
            && self.people_judged
            && main == Some(Subject::Person)
        {
            ScanProfile::Portrait.priority()
        } else {
            profile.priority()
        };
        base.iter()
            .copied()
            .filter(|kind| match kind {
                Subject::Eye | Subject::Face => self.faces,
                Subject::Person => self.people || self.animals,
                Subject::Vehicle => self.vehicles,
                Subject::WholeFrame => true,
            })
            .collect()
    }

    /// Whether a feature's value comes from the album rather than the preset.
    pub fn overridden(&self, feature: Feature) -> bool {
        self.overrides.get(feature).is_some()
    }

    /// The compact "what Conrod will do" summary the UI shows when a shoot
    /// type is picked: one row per detector and reader, plus the knobs that
    /// are not switchable.
    pub fn summary(&self) -> serde_json::Value {
        use serde_json::json;
        let policy = self.preset.policy();
        let row = |f: Feature| {
            let requires = f.requires_for(self.preset);
            let blocked = requires.is_some_and(|r| !self.on(r));
            json!({
                "key": f.key(),
                "label": f.label(self.preset),
                "on": self.on(f),
                // What clearing the album's switch goes back to: the
                // photographer's default for the type, else the type's own.
                "default": self
                    .user
                    .get(f)
                    .unwrap_or_else(|| policy_default(self.preset, &policy, false, f)),
                "overridden": self.overridden(f),
                "requires": requires.map(Feature::key),
                "blocked": blocked,
            })
        };
        json!({
            "preset": self.preset.name(),
            "label": self.preset.label(),
            "profile": policy.profile.name(),
            "detects": Feature::DETECTS.map(row),
            "reads": Feature::READS.map(row),
            "moments": {
                "detect": row(Feature::MomentsDetect),
                "rank": row(Feature::MomentsRank),
                "vocabulary": crate::moments::vocabulary(self.preset)
                    .iter()
                    .map(|m| json!({"key": m.key, "label": m.label, "weight": m.weight}))
                    .collect::<Vec<_>>(),
            },
            "panForgiven": policy.pan_allowed,
            "blur": policy.default_blur,
            "levelCheck": policy.level,
            "darkOk": policy.dark_ok,
            "deliverMost": policy.deliver_most,
        })
    }
}

impl ScanProfile {
    pub const ALL: [ScanProfile; 4] = [
        ScanProfile::Motorsport,
        ScanProfile::Portrait,
        ScanProfile::Event,
        ScanProfile::Mix,
    ];

    /// The name stored in settings and on the job; unknown names are motorsport.
    pub fn parse(name: &str) -> ScanProfile {
        ShootPreset::parse(name).profile()
    }

    pub fn name(self) -> &'static str {
        match self {
            ScanProfile::Motorsport => "motorsport",
            ScanProfile::Portrait => "portrait",
            ScanProfile::Event => "event",
            ScanProfile::Mix => "mix",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ScanProfile::Motorsport => "Motorsport",
            ScanProfile::Portrait => "Portrait session",
            ScanProfile::Event => "Event",
            ScanProfile::Mix => "Mix",
        }
    }

    pub fn wants_vehicles(self) -> bool {
        !matches!(self, ScanProfile::Portrait)
    }

    pub fn wants_people(self) -> bool {
        true
    }

    /// Faces and eyes: the costly extra pass, skipped where no one looks at
    /// a driver's eyes -- unless the album includes people.
    pub fn wants_faces(self, include_people: bool) -> bool {
        include_people || !matches!(self, ScanProfile::Motorsport)
    }

    /// First present wins.
    pub fn priority(self) -> &'static [Subject] {
        use Subject::*;
        match self {
            ScanProfile::Motorsport => &[Vehicle, Person, WholeFrame],
            ScanProfile::Portrait => &[Eye, Face, Person, WholeFrame],
            ScanProfile::Event | ScanProfile::Mix => &[Eye, Face, Person, Vehicle, WholeFrame],
        }
    }

    /// The priority for one frame. A motorsport album that includes people
    /// is judged by the frame's main subject: when that is a person, the
    /// frame is rated like a portrait (eye, then face, then person).
    pub fn frame_priority(self, include_people: bool, main: Option<Subject>) -> &'static [Subject] {
        if self == ScanProfile::Motorsport && include_people && main == Some(Subject::Person) {
            ScanProfile::Portrait.priority()
        } else {
            self.priority()
        }
    }

    /// Whether a sharp subject against a smeared background is a keeper.
    /// Off for portraits: there the soft background is bokeh, and the face is
    /// judged on its own.
    pub fn pan_compatible(self) -> bool {
        !matches!(self, ScanProfile::Portrait)
    }
}

/// A frame's main subject: the kind of its largest box, person or vehicle.
/// Other kinds (faces, eyes) belong to a person and do not compete.
pub fn main_subject(boxes: impl IntoIterator<Item = (Subject, f64)>) -> Option<Subject> {
    boxes
        .into_iter()
        .filter(|(kind, _)| matches!(kind, Subject::Person | Subject::Vehicle))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(kind, _)| kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_unknowns_fall_back() {
        for p in ScanProfile::ALL {
            assert_eq!(ScanProfile::parse(p.name()), p);
        }
        assert_eq!(ScanProfile::parse("  Portrait "), ScanProfile::Portrait);
        assert_eq!(ScanProfile::parse("banquet"), ScanProfile::Motorsport);
        for preset in ShootPreset::ALL {
            assert_eq!(ShootPreset::parse(preset.name()), preset);
            assert_eq!(ScanProfile::parse(preset.name()), preset.profile());
            assert_eq!(ShootPreset::parse(&format!(" {} ", preset.name())), preset);
        }
    }

    #[test]
    fn every_profile_can_fall_back_to_the_whole_frame() {
        for p in ScanProfile::ALL {
            assert_eq!(p.priority().last(), Some(&Subject::WholeFrame));
        }
    }

    #[test]
    fn track_presets_default_to_critical_blur() {
        assert_eq!(
            ShootPreset::Motorsport.default_blur_strictness(),
            "critical"
        );
        assert_eq!(
            ShootPreset::MotorsportTrack.default_blur_strictness(),
            "critical"
        );
        // Unchanged: burnouts and rallies stay lenient.
        assert_eq!(
            ShootPreset::MotorsportBurnouts.default_blur_strictness(),
            "lenient"
        );
        assert_eq!(
            ShootPreset::MotorsportRally.default_blur_strictness(),
            "lenient"
        );
        // Unchanged: MotorsportMeet sets no blur of its own, so it keeps moderate.
        assert_eq!(
            ShootPreset::MotorsportMeet.default_blur_strictness(),
            "moderate"
        );
    }

    use ShootPreset::*;

    fn lanes(preset: ShootPreset) -> Lanes {
        Lanes::defaults(preset)
    }

    fn with(preset: ShootPreset, feature: Feature, on: bool) -> Lanes {
        let mut o = Overrides::default();
        o.set(feature, Some(on));
        Lanes::resolve(preset, false, &o)
    }

    /// Every preset is in exactly one of `on` / `off`.
    fn partition(on: &[ShootPreset], pick: impl Fn(&Lanes) -> bool) {
        for preset in ShootPreset::ALL {
            let want = on.contains(&preset);
            assert_eq!(pick(&lanes(preset)), want, "{preset:?}");
        }
    }

    #[test]
    fn race_numbers_are_read_where_competitors_wear_them() {
        // Competition shoots, players' jerseys, plus a mix that assumes
        // nothing. Meets, dealers, aircraft, portraits, weddings, shows... get none.
        partition(
            &[
                Motorsport,
                MotorsportBurnouts,
                MotorsportTrack,
                MotorsportRally,
                SportCycling,
                SportAction,
                SportField,
                SportCourt,
                Mix,
            ],
            |l| l.numbers,
        );
    }

    #[test]
    fn field_and_court_sport_read_jersey_numbers_off_the_people() {
        for preset in [SportField, SportCourt] {
            let l = lanes(preset);
            assert!(l.numbers && l.people && !l.vehicles, "{preset:?}");
            assert!(l.reads_players() && l.identifies() && l.asks_model());
            // Not a vehicle lane: nothing about plates, make or livery.
            assert!(!l.identifies_vehicles());
            assert!(!l.plates && !l.make_model && !l.sponsors);
            let s = l.summary();
            assert_eq!(s["reads"][0]["label"], "Jersey numbers");
            assert_eq!(s["reads"][0]["requires"], "people");
            assert_eq!(s["reads"][0]["blocked"], false);
        }
        // No people, no jerseys to read.
        let l = with(SportField, Feature::People, false);
        assert!(!l.numbers && !l.reads_players());
        assert_eq!(l.summary()["reads"][0]["blocked"], true);
        // Numbers off: the lane has nothing to do.
        assert!(!with(SportCourt, Feature::Numbers, false).identifies());
        // Everywhere else the lane is a vehicle lane only.
        for preset in ShootPreset::ALL {
            if !matches!(preset, SportField | SportCourt) {
                assert!(!lanes(preset).reads_players(), "{preset:?}");
            }
        }
        // A type that is not about players cannot read them: portraits need a vehicle.
        assert!(!with(Portrait, Feature::Numbers, true).reads_players());
        // Court sport still names people from faces, and reads their numbers.
        assert!(lanes(SportCourt).names && !lanes(SportField).names);
    }

    #[test]
    fn faces_at_motorsport_are_the_photographers_choice_and_people_stay_on() {
        // Faces are off by default at every motorsport type with a crowd or a
        // paddock; people are on (the crowd, the pit lane, the service park).
        for preset in [
            Motorsport,
            MotorsportTrack,
            MotorsportRally,
            MotorsportBurnouts,
        ] {
            let l = lanes(preset);
            assert!(l.people && !l.faces && !l.people_judged, "{preset:?}");
            assert!(!l.names, "names ride on faces: {preset:?}");
        }
        // Car meets are owners with their cars: faces stay on.
        assert!(lanes(MotorsportMeet).faces);
        // One switch brings faces, names and the portrait-style judging back.
        for preset in [MotorsportTrack, MotorsportRally] {
            let l = with(preset, Feature::Faces, true);
            assert!(l.faces && l.names && l.people_judged, "{preset:?}");
        }
        let l = with(MotorsportBurnouts, Feature::Faces, true);
        assert!(l.faces && l.people_judged && !l.names);
    }

    #[test]
    fn moments_are_on_for_every_type_that_has_a_vocabulary() {
        for preset in ShootPreset::ALL {
            let l = lanes(preset);
            let has = !crate::moments::vocabulary(preset).is_empty();
            assert_eq!(l.moments_detect, has, "{preset:?}");
            assert_eq!(l.moments_rank, has, "{preset:?}");
            assert_eq!(!l.moments().is_empty(), has, "{preset:?}");
        }
    }

    #[test]
    fn plates_are_read_where_cars_are_registered() {
        partition(
            &[
                MotorsportBurnouts,
                MotorsportTrack,
                MotorsportRally,
                MotorsportMeet,
                AutoDealer,
                Aviation,
                Mix,
            ],
            |l| l.plates,
        );
    }

    #[test]
    fn make_and_model_are_read_where_the_car_is_the_story() {
        partition(
            &[
                Motorsport,
                MotorsportBurnouts,
                MotorsportTrack,
                MotorsportRally,
                MotorsportMeet,
                AutoDealer,
                Aviation,
                Mix,
            ],
            |l| l.make_model,
        );
        // Not livery text at a meet or a dealership.
        assert!(!lanes(MotorsportMeet).sponsors && !lanes(AutoDealer).sponsors);
        assert!(lanes(Motorsport).sponsors && lanes(Aviation).sponsors);
    }

    #[test]
    fn vehicles_are_on_by_default_only_where_they_are_the_subject() {
        partition(
            &[
                Motorsport,
                MotorsportBurnouts,
                MotorsportTrack,
                MotorsportRally,
                MotorsportMeet,
                Mix,
                SportCycling,
                SportAction,
                AutoDealer,
                Aviation,
            ],
            |l| l.vehicles,
        );
        // Shows, portraits, weddings, concerts: off by default.
        for off in [EventShows, Portrait, Wedding, Concert, Event, EventParties] {
            assert!(!lanes(off).vehicles, "{off:?}");
        }
    }

    #[test]
    fn animals_are_looked_for_where_animals_are_in_the_picture() {
        partition(
            &[
                Portrait,
                PortraitGroup,
                PortraitCouple,
                PortraitPets,
                Event,
                EventShows,
                Mix,
                SportRodeo,
                SportEquestrian,
                WildlifeBirds,
            ],
            |l| l.animals,
        );
    }

    /// A rodeo and an equestrian shoot read the animal and the rider on it: no
    /// vehicle lane of their own, no reader that needs one, and the judge works
    /// the moments. The switches still answer both ways per album.
    #[test]
    fn a_rodeo_and_an_equestrian_read_the_rider_and_the_animal() {
        for preset in [SportRodeo, SportEquestrian] {
            let l = lanes(preset);
            assert!(!l.vehicles, "{preset:?} has a vehicle lane by default");
            assert!(l.animals && l.people, "{preset:?}");
            assert!(l.faces && l.names, "{preset:?} suggests names from faces");
            assert!(!l.identifies_vehicles(), "{preset:?}");
            assert!(!l.identifies(), "{preset:?} reads nothing off a vehicle");
            assert!(l.moments_detect && l.moments_rank, "{preset:?}");
            // The album can switch vehicles on, the reader included.
            let o = with(preset, Feature::Vehicles, true);
            assert!(o.vehicles, "{preset:?}");
            assert!(
                !o.identifies_vehicles(),
                "{preset:?} still reads nothing off it"
            );
        }
    }

    #[test]
    fn every_detector_and_reader_overrides_both_ways_on_every_preset() {
        for preset in ShootPreset::ALL {
            for feature in Feature::ALL {
                // A type with no moments has nothing to switch on.
                if Feature::MOMENTS.contains(&feature)
                    && crate::moments::vocabulary(preset).is_empty()
                {
                    continue;
                }
                for on in [true, false] {
                    let mut o = Overrides::default();
                    o.set(feature, Some(on));
                    // A reader needs its detector; give it one.
                    if let Some(needed) = feature.requires() {
                        o.set(needed, Some(true));
                    }
                    let l = Lanes::resolve(preset, false, &o);
                    assert_eq!(l.on(feature), on, "{preset:?} {feature:?}");
                    assert!(l.overridden(feature));
                    // Clearing goes back to the preset's own default.
                    o.set(feature, None);
                    if let Some(needed) = feature.requires() {
                        o.set(needed, None);
                    }
                    assert_eq!(Lanes::resolve(preset, false, &o), lanes(preset));
                }
            }
        }
    }

    #[test]
    fn readers_need_vehicles_and_names_need_faces() {
        let l = with(MotorsportTrack, Feature::Vehicles, false);
        assert!(!l.numbers && !l.plates && !l.make_model && !l.sponsors);
        assert!(!l.identifies_vehicles());
        // Names ride the face pass.
        let l = with(Wedding, Feature::Faces, false);
        assert!(!l.names && l.people);
        // A reader switched on where it was off needs a vehicle to read.
        let l = with(Portrait, Feature::Numbers, true);
        assert!(!l.numbers);
        let mut o = Overrides::default();
        o.set(Feature::Vehicles, Some(true));
        o.set(Feature::Numbers, Some(true));
        assert!(Lanes::resolve(Portrait, false, &o).identifies_vehicles());
    }

    #[test]
    fn the_identification_lane_runs_for_a_preset_only_when_it_reads_vehicles() {
        for preset in ShootPreset::ALL {
            let l = lanes(preset);
            assert_eq!(
                l.identifies_vehicles(),
                l.vehicles && (l.numbers || l.plates || l.make_model || l.sponsors),
                "{preset:?}"
            );
        }
        assert!(!lanes(Wedding).identifies_vehicles());
        assert!(!lanes(Landscape).identifies_vehicles());
        assert!(lanes(MotorsportMeet).identifies_vehicles());
    }

    #[test]
    fn include_people_only_moves_the_faces_default() {
        let none = Overrides::default();
        assert!(!Lanes::resolve(Motorsport, false, &none).faces);
        assert!(Lanes::resolve(Motorsport, true, &none).faces);
        assert!(Lanes::resolve(Motorsport, true, &none).people_judged);
        assert!(!Lanes::resolve(Motorsport, false, &none).people_judged);
        // An explicit faces-off wins over the old switch.
        let mut o = Overrides::default();
        o.set(Feature::Faces, Some(false));
        assert!(!Lanes::resolve(Motorsport, true, &o).faces);
        assert!(!Lanes::resolve(Motorsport, true, &o).people_judged);
        // Places that never had faces off are unchanged.
        assert!(lanes(Portrait).faces && lanes(MotorsportMeet).faces);
    }

    #[test]
    fn track_days_judge_person_frames_as_portraits_once_faces_are_on() {
        // Faces are the photographer's choice; until they ask, a person is
        // judged as a person and the car still decides.
        let l = lanes(MotorsportTrack);
        assert!(l.vehicles && l.people && !l.faces && !l.people_judged);
        assert_eq!(
            l.frame_priority(Some(Subject::Person)),
            vec![Subject::Vehicle, Subject::Person, Subject::WholeFrame]
        );
        let l = with(MotorsportTrack, Feature::Faces, true);
        assert!(l.vehicles && l.people && l.faces && l.people_judged);
        // The car still decides when it is the biggest thing in the frame.
        assert_eq!(
            l.frame_priority(Some(Subject::Vehicle)),
            vec![Subject::Vehicle, Subject::Person, Subject::WholeFrame]
        );
        assert_eq!(
            l.frame_priority(Some(Subject::Person)),
            vec![
                Subject::Eye,
                Subject::Face,
                Subject::Person,
                Subject::WholeFrame
            ]
        );
    }

    #[test]
    fn a_switched_off_detector_leaves_the_frame_priority() {
        // Faces off on a portrait shoot: person, then the whole frame.
        let l = with(Portrait, Feature::Faces, false);
        assert_eq!(
            l.frame_priority(None),
            vec![Subject::Person, Subject::WholeFrame]
        );
        // No people and no animals: only vehicles (or the whole frame).
        let mut o = Overrides::default();
        o.set(Feature::People, Some(false));
        o.set(Feature::Animals, Some(false));
        o.set(Feature::Faces, Some(false));
        let l = Lanes::resolve(Event, false, &o);
        assert_eq!(l.frame_priority(None), vec![Subject::WholeFrame]);
        // Vehicles off on a car shoot: people decide.
        let l = with(Motorsport, Feature::Vehicles, false);
        assert_eq!(
            l.frame_priority(None),
            vec![Subject::Person, Subject::WholeFrame]
        );
        // Animals are stored as person boxes and keep that kind alive.
        let l = lanes(WildlifeBirds);
        assert!(!l.people && l.animals);
        assert!(l.frame_priority(None).contains(&Subject::Person));
        // The whole frame is always the last resort.
        for preset in ShootPreset::ALL {
            for feature in Feature::DETECTS {
                let l = with(preset, feature, false);
                assert_eq!(
                    l.frame_priority(None).last(),
                    Some(&Subject::WholeFrame),
                    "{preset:?} {feature:?}"
                );
            }
        }
    }

    #[test]
    fn the_vehicles_lane_asks_only_for_its_own_classes() {
        assert_eq!(lanes(Aviation).vehicle_classes, &[4]);
        assert_eq!(lanes(SportCycling).vehicle_classes, &[1]);
        assert_eq!(lanes(SportAction).vehicle_classes, &[1, 3]);
        for preset in ShootPreset::ALL {
            if !matches!(preset, Aviation | SportCycling | SportAction) {
                assert!(lanes(preset).vehicle_classes.is_empty(), "{preset:?}");
            }
        }
    }

    #[test]
    fn stored_overrides_ignore_junk() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"vehicles": true, "faces": "yes", "nonsense": false, "numbers": false}"#,
        )
        .unwrap();
        let o = Overrides::from_map(&map);
        assert_eq!(o.get(Feature::Vehicles), Some(true));
        assert_eq!(o.get(Feature::Faces), None);
        assert_eq!(o.get(Feature::Numbers), Some(false));
        assert_eq!(Overrides::from_map(&o.to_map()), o);
        for feature in Feature::ALL {
            assert_eq!(Feature::parse(feature.key()), Some(feature));
        }
    }

    #[test]
    fn the_summary_lists_every_detector_and_reader() {
        let s = with(EventShows, Feature::Vehicles, true).summary();
        let keys = |k: &str| -> Vec<String> {
            s[k].as_array()
                .unwrap()
                .iter()
                .map(|r| r["key"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(keys("detects"), ["vehicles", "people", "faces", "animals"]);
        assert_eq!(
            keys("reads"),
            ["numbers", "plates", "make_model", "sponsors", "names"]
        );
        let vehicles = &s["detects"][0];
        assert_eq!(vehicles["on"], true);
        assert_eq!(vehicles["default"], false);
        assert_eq!(vehicles["overridden"], true);
        assert_eq!(s["label"], "Shows");
        assert_eq!(s["blur"], "lenient");
        assert_eq!(s["darkOk"], true);
        // Aviation talks about aircraft, not cars.
        let a = lanes(Aviation).summary();
        assert_eq!(a["detects"][0]["label"], "Aircraft");
        assert_eq!(a["reads"][1]["label"], "Registrations");
    }

    /// The frontend's copy of every preset's default summary (browser mock,
    /// facet tabs, first paint). The app itself asks the engine.
    fn policies_module() -> String {
        let mut all = serde_json::Map::new();
        for preset in ShootPreset::ALL {
            all.insert(preset.name().to_string(), lanes(preset).summary());
        }
        format!(
            "// Generated from the Rust policy table (conrod-core profile.rs). Do not edit:\n// UPDATE_PRESET_POLICIES=1 cargo test -p conrod-core presets_match_the_frontend_copy\nexport default {};\n",
            serde_json::to_string_pretty(&all).unwrap()
        )
    }

    #[test]
    fn presets_match_the_frontend_copy() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../frontend/src/lib/presetPolicies.mjs");
        let want = policies_module();
        if std::env::var_os("UPDATE_PRESET_POLICIES").is_some() {
            std::fs::write(&path, &want).unwrap();
        }
        let have = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            have == want,
            "frontend/src/lib/presetPolicies.mjs is stale: run with UPDATE_PRESET_POLICIES=1"
        );
    }

    #[test]
    fn defaults_never_read_without_something_to_read_from() {
        for preset in ShootPreset::ALL {
            let p = preset.policy();
            if !p.vehicles {
                // Jersey numbers are read off the people, not a vehicle.
                assert!(
                    !((p.vlm.numbers && !p.players)
                        || p.vlm.plates
                        || p.vlm.make_model
                        || p.vlm.sponsors),
                    "{preset:?} reads vehicles it does not look for"
                );
                if p.players {
                    assert!(p.people, "{preset:?} reads jerseys with no people");
                }
            }
            // Names ride the face pass: a type that names people but starts
            // with faces off keeps the names idle until faces are switched on.
            if !p.faces_default && p.vlm.names {
                assert!(
                    !lanes(preset).names,
                    "{preset:?} names people it cannot see"
                );
            }
        }
    }

    #[test]
    fn every_policy_is_internally_consistent() {
        for preset in ShootPreset::ALL {
            let p = preset.policy();
            assert_eq!(p.profile, preset.profile(), "{preset:?}");
            assert!(
                p.min_box_factor > 0.0 && p.min_box_factor <= 1.0,
                "{preset:?}"
            );
            assert!(
                matches!(
                    p.default_blur,
                    "critical" | "strict" | "moderate" | "lenient"
                ),
                "{preset:?}"
            );
            assert!(!preset.label().is_empty(), "{preset:?}");
            assert!(!preset.prompt_context().is_empty(), "{preset:?}");
            // A preset with no people never needs faces, and every animal
            // preset keeps faces only where people are looked for.
            if !p.people && p.faces_default {
                panic!("{preset:?} finds faces without people");
            }
            // The whole frame is always the fallback of a profile's priority.
            assert_eq!(preset.priority().last(), Some(&Subject::WholeFrame));
            assert_eq!(preset.max_subjects(0), p.max_subjects_floor, "{preset:?}");
        }
    }

    #[test]
    fn only_motorsport_with_people_follows_the_main_subject() {
        use ScanProfile::*;
        for p in ScanProfile::ALL {
            for main in [None, Some(Subject::Vehicle), Some(Subject::Person)] {
                assert_eq!(p.frame_priority(false, main), p.priority(), "{p:?}");
                if p != Motorsport {
                    assert_eq!(p.frame_priority(true, main), p.priority(), "{p:?}");
                }
            }
        }
        assert_eq!(
            Motorsport.frame_priority(true, Some(Subject::Vehicle))[0],
            Subject::Vehicle
        );
        assert_eq!(Motorsport.frame_priority(true, None)[0], Subject::Vehicle);
        assert_eq!(
            Motorsport.frame_priority(true, Some(Subject::Person)),
            Portrait.priority()
        );
    }

    #[test]
    fn the_largest_person_or_vehicle_is_the_main_subject() {
        use Subject::*;
        assert_eq!(
            main_subject([(Vehicle, 900.0), (Person, 400.0)]),
            Some(Vehicle)
        );
        assert_eq!(
            main_subject([(Vehicle, 300.0), (Person, 400.0)]),
            Some(Person)
        );
        // A face's box never makes it the main subject on its own.
        assert_eq!(
            main_subject([(Face, 5000.0), (Vehicle, 10.0)]),
            Some(Vehicle)
        );
        assert_eq!(main_subject([(Eye, 1.0)]), None);
        // Person larger: a present eye beats the face and the person.
        let present = [Face, Eye, Person, Vehicle];
        let first = first_judged(&present, main_subject([(Vehicle, 1.0), (Person, 2.0)]));
        assert_eq!(first, Some(Eye));
        let first = first_judged(&[Face, Person, Vehicle], Some(Person));
        assert_eq!(first, Some(Face));
        let first = first_judged(&present, Some(Vehicle));
        assert_eq!(first, Some(Vehicle));
    }

    fn first_judged(present: &[Subject], main: Option<Subject>) -> Option<Subject> {
        ScanProfile::Motorsport
            .frame_priority(true, main)
            .iter()
            .copied()
            .find(|k| present.contains(k))
    }
}

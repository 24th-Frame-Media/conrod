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
    /// Bib or jersey numbers and the people wearing them.
    pub const BIB: VlmTargets = VlmTargets {
        numbers: true,
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
    /// "strict", "moderate" or "lenient"; a default for later wiring.
    pub default_blur: &'static str,
    pub group: GroupUnit,
    pub vlm: VlmTargets,
    pub min_box_factor: f64,
    /// A floor for `max_subjects`; 0 keeps whatever the album asks for.
    pub max_subjects_floor: usize,
    /// Condition reports: only true duplicates and blurred frames go.
    pub deliver_most: bool,
    /// Extra COCO classes asked of the detector, e.g. 4 (airplane).
    pub extra_vehicles: &'static [usize],
}

impl PresetPolicy {
    const fn new(profile: ScanProfile) -> PresetPolicy {
        PresetPolicy {
            profile,
            vehicles: false,
            people: true,
            animals: true,
            ball: false,
            faces_default: true,
            pan_allowed: false,
            default_blur: "moderate",
            group: GroupUnit::Moment,
            vlm: VlmTargets::NONE,
            min_box_factor: 1.0,
            max_subjects_floor: 0,
            deliver_most: false,
            extra_vehicles: &[],
        }
    }

    const fn vehicles(mut self) -> Self {
        self.vehicles = true;
        self
    }

    const fn no_people(mut self) -> Self {
        self.people = false;
        self
    }

    const fn no_animals(mut self) -> Self {
        self.animals = false;
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

    const fn extra_vehicles(mut self, classes: &'static [usize]) -> Self {
        self.extra_vehicles = classes;
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

    /// The one table: everything a preset changes about a scan.
    pub fn policy(self) -> PresetPolicy {
        use ScanProfile::{Event, Mix, Motorsport, Portrait};
        match self {
            ShootPreset::Portrait => PresetPolicy::new(Portrait).blur("strict"),
            ShootPreset::PortraitGroup => PresetPolicy::new(Portrait)
                .blur("strict")
                .min_box(0.5)
                .floor(24),
            ShootPreset::PortraitCouple => PresetPolicy::new(Portrait).blur("strict"),
            ShootPreset::PortraitPets => PresetPolicy::new(Portrait).blur("strict"),
            ShootPreset::Motorsport => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_faces()
                .pan()
                .vlm(VlmTargets::RACE),
            ShootPreset::MotorsportBurnouts => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_faces()
                .pan()
                .blur("lenient")
                .vlm(VlmTargets::RACE),
            ShootPreset::MotorsportTrack => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_faces()
                .pan()
                .min_box(0.7)
                .vlm(VlmTargets::CREW),
            ShootPreset::MotorsportRally => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_faces()
                .pan()
                .blur("lenient")
                .min_box(0.7)
                .vlm(VlmTargets::CREW),
            ShootPreset::MotorsportMeet => PresetPolicy::new(Motorsport)
                .vehicles()
                .floor(16)
                .vlm(VlmTargets::PLATES),
            ShootPreset::Event => PresetPolicy::new(Event)
                .vehicles()
                .pan()
                .vlm(VlmTargets::NAMES),
            ShootPreset::EventShows => PresetPolicy::new(Event)
                .vehicles()
                .pan()
                .blur("lenient")
                .min_box(0.5)
                .floor(24)
                .vlm(VlmTargets::NAMES),
            ShootPreset::EventParties => PresetPolicy::new(Event)
                .min_box(0.5)
                .floor(24)
                .vlm(VlmTargets::NAMES),
            ShootPreset::Mix => PresetPolicy::new(Mix).vehicles().pan(),
            ShootPreset::SportField => PresetPolicy::new(Event)
                .ball()
                .no_faces()
                .no_animals()
                .min_box(0.7)
                .floor(24)
                .vlm(VlmTargets::BIB),
            ShootPreset::SportCourt => PresetPolicy::new(Event)
                .ball()
                .no_animals()
                .pan()
                .blur("strict")
                .vlm(VlmTargets::BIB),
            ShootPreset::SportRodeo => PresetPolicy::new(Event)
                .min_box(0.7)
                .vlm(VlmTargets::NAMES),
            ShootPreset::SportEquestrian => PresetPolicy::new(Event)
                .pan()
                .min_box(0.7)
                .vlm(VlmTargets::NAMES),
            ShootPreset::SportCycling => PresetPolicy::new(Motorsport)
                .no_faces()
                .pan()
                .min_box(0.7)
                .vlm(VlmTargets::BIB),
            ShootPreset::SportAction => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_faces()
                .pan()
                .vlm(VlmTargets::NAMES),
            ShootPreset::PropertyListing => PresetPolicy::new(Mix)
                .no_people()
                .no_animals()
                .no_faces()
                .blur("strict")
                .group(GroupUnit::KeepSet),
            ShootPreset::PropertyReport => PresetPolicy::new(Mix)
                .no_people()
                .no_animals()
                .no_faces()
                .blur("strict")
                .group(GroupUnit::KeepSet)
                .deliver_most(),
            ShootPreset::AutoDealer => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_people()
                .no_animals()
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
                .vlm(VlmTargets::NAMES),
            ShootPreset::WildlifeBirds => PresetPolicy::new(Mix)
                .no_people()
                .no_faces()
                .pan()
                .min_box(0.5),
            ShootPreset::Landscape => PresetPolicy::new(Mix)
                .no_people()
                .no_animals()
                .no_faces()
                .blur("strict")
                .group(GroupUnit::KeepSet),
            ShootPreset::Astro => PresetPolicy::new(Mix)
                .no_people()
                .no_animals()
                .no_faces()
                .blur("lenient")
                .group(GroupUnit::KeepSet),
            ShootPreset::Aviation => PresetPolicy::new(Motorsport)
                .vehicles()
                .no_people()
                .no_animals()
                .no_faces()
                .pan()
                .min_box(0.5)
                .vlm(VlmTargets::PLATES)
                .extra_vehicles(&[4]),
        }
    }

    pub fn wants_vehicles(self) -> bool {
        self.policy().vehicles
    }

    pub fn wants_people(self) -> bool {
        self.policy().people
    }

    /// `include_people` is the album's choice to look at people too; only the
    /// presets whose policy skips faces change with it.
    pub fn wants_faces(self, include_people: bool) -> bool {
        include_people || self.policy().faces_default
    }

    /// Animals are looked for wherever the policy says a wildlife frame can
    /// turn up; frame priority still lets a vehicle or a person decide.
    pub fn wants_animals(self) -> bool {
        self.policy().animals
    }

    /// Whether the sports ball belongs in the context classes.
    pub fn wants_ball(self) -> bool {
        self.policy().ball
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

    /// Extra COCO vehicle classes, e.g. 4 (airplane) for aviation.
    pub fn extra_vehicle_classes(self) -> &'static [usize] {
        self.policy().extra_vehicles
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

    /// Plates, numbers and the vision model -- the slow lane.
    pub fn identifies_vehicles(self) -> bool {
        matches!(self, ScanProfile::Motorsport | ScanProfile::Mix)
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

    /// The thirteen presets from before the policy table.
    const ORIGINAL: [ShootPreset; 13] = [
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
    ];

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
    fn include_people_turns_faces_on_only_where_they_were_off() {
        use ScanProfile::*;
        for (p, off, on) in [
            (Motorsport, false, true),
            (Portrait, true, true),
            (Event, true, true),
            (Mix, true, true),
        ] {
            assert_eq!(p.wants_faces(false), off, "{p:?}");
            assert_eq!(p.wants_faces(true), on, "{p:?}");
            assert!(p.wants_people(), "{p:?}");
        }
        assert!(!ShootPreset::MotorsportTrack.wants_faces(false));
        assert!(ShootPreset::MotorsportTrack.wants_faces(true));
        assert!(ShootPreset::MotorsportMeet.wants_faces(false));
        for preset in ORIGINAL {
            if preset.profile() != Motorsport {
                assert_eq!(preset.wants_faces(false), preset.wants_faces(true));
            }
        }
    }

    /// The values the thirteen original presets had before the policy table.
    /// (preset, profile, vehicles, people, faces off, animals, pan,
    ///  min_box factor, max_subjects with a baseline of 8)
    #[test]
    fn the_original_presets_keep_their_old_values() {
        use ShootPreset::*;
        type OldValues = (ShootPreset, ScanProfile, bool, bool, bool, bool, bool, f64, usize);
        let old: [OldValues; 13] = [
            (Portrait, ScanProfile::Portrait, false, true, false, true, false, 1.0, 8),
            (PortraitGroup, ScanProfile::Portrait, false, true, false, true, false, 0.5, 24),
            (PortraitCouple, ScanProfile::Portrait, false, true, false, true, false, 1.0, 8),
            (PortraitPets, ScanProfile::Portrait, false, true, false, true, false, 1.0, 8),
            (Motorsport, ScanProfile::Motorsport, true, true, true, true, true, 1.0, 8),
            (MotorsportBurnouts, ScanProfile::Motorsport, true, true, true, true, true, 1.0, 8),
            (MotorsportTrack, ScanProfile::Motorsport, true, true, true, true, true, 0.7, 8),
            (MotorsportRally, ScanProfile::Motorsport, true, true, true, true, true, 0.7, 8),
            (MotorsportMeet, ScanProfile::Motorsport, true, true, false, true, false, 1.0, 16),
            (Event, ScanProfile::Event, true, true, false, true, true, 1.0, 8),
            (EventShows, ScanProfile::Event, true, true, false, true, true, 0.5, 24),
            (EventParties, ScanProfile::Event, false, true, false, true, false, 0.5, 24),
            (Mix, ScanProfile::Mix, true, true, false, true, true, 1.0, 8),
        ];
        for (preset, profile, vehicles, people, faces_off, animals, pan, factor, max) in old {
            assert_eq!(preset.profile(), profile, "{preset:?}");
            assert_eq!(preset.wants_vehicles(), vehicles, "{preset:?}");
            assert_eq!(preset.wants_people(), people, "{preset:?}");
            assert_eq!(!preset.wants_faces(false), faces_off, "{preset:?}");
            assert!(preset.wants_faces(true), "{preset:?}");
            assert_eq!(preset.wants_animals(), animals, "{preset:?}");
            assert_eq!(preset.pan_compatible(), pan, "{preset:?}");
            assert!((preset.min_box_fraction(1.0) - factor).abs() < 1e-9, "{preset:?}");
            assert_eq!(preset.max_subjects(8), max, "{preset:?}");
        }
    }

    #[test]
    fn every_preset_with_a_subject_looks_for_animals_where_it_matters() {
        for preset in ORIGINAL {
            assert!(preset.wants_animals(), "{preset:?}");
        }
        for preset in [
            ShootPreset::PortraitPets,
            ShootPreset::SportRodeo,
            ShootPreset::SportEquestrian,
            ShootPreset::WildlifeBirds,
        ] {
            assert!(preset.wants_animals(), "{preset:?}");
        }
        for preset in [
            ShootPreset::PropertyListing,
            ShootPreset::PropertyReport,
            ShootPreset::Landscape,
            ShootPreset::Astro,
            ShootPreset::Aviation,
        ] {
            assert!(!preset.wants_animals(), "{preset:?}");
        }
    }

    #[test]
    fn the_policy_table_matches_the_matrix_for_the_new_presets() {
        use GroupUnit::{KeepSet, Moment, PerAngle};
        // sport-field: people and ball, faces off by default, moderate, moments,
        // numbers and names.
        let field = ShootPreset::SportField.policy();
        assert!(field.people && field.ball && !field.vehicles);
        assert!(!field.faces_default);
        assert_eq!(field.default_blur, "moderate");
        assert_eq!(field.group, Moment);
        assert_eq!(field.profile, ScanProfile::Event);
        assert!(field.vlm.numbers && field.vlm.names && !field.vlm.plates);
        assert!(!field.pan_allowed);
        // sport-court rides the same parent with faces left on.
        assert!(ShootPreset::SportCourt.wants_faces(false));
        assert_eq!(ShootPreset::SportCourt.default_blur_strictness(), "strict");
        // Rodeo: riders decide, pans are not excused, numbers are not read.
        let rodeo = ShootPreset::SportRodeo.policy();
        assert!(rodeo.people && rodeo.animals && !rodeo.pan_allowed);
        assert!(!rodeo.vlm.numbers);
        assert!(ShootPreset::SportEquestrian.pan_compatible());
        // Cycling and action sports allow pans; cyclists wear bib numbers.
        assert!(ShootPreset::SportCycling.pan_compatible());
        assert!(ShootPreset::SportCycling.policy().vlm.numbers);
        assert!(ShootPreset::SportAction.pan_compatible());
        // Property: no subjects, brackets kept, and the report delivers most.
        for preset in [ShootPreset::PropertyListing, ShootPreset::PropertyReport] {
            let p = preset.policy();
            assert!(!p.people && !p.vehicles && !p.animals && !p.faces_default, "{preset:?}");
            assert_eq!(p.group, KeepSet, "{preset:?}");
            assert_eq!(p.default_blur, "strict", "{preset:?}");
            assert!(!p.pan_allowed, "{preset:?}");
        }
        assert!(!ShootPreset::PropertyListing.deliver_most());
        assert!(ShootPreset::PropertyReport.deliver_most());
        assert_eq!(ShootPreset::AutoDealer.group_unit(), PerAngle);
        assert!(ShootPreset::AutoDealer.policy().vlm.plates);
        assert!(ShootPreset::AutoDealer.policy().vlm.make_model);
        assert!(!ShootPreset::AutoDealer.policy().vlm.numbers);
        assert_eq!(ShootPreset::Wedding.profile(), ScanProfile::Event);
        assert!(!ShootPreset::Wedding.pan_compatible());
        assert_eq!(ShootPreset::Concert.default_blur_strictness(), "lenient");
        assert!(ShootPreset::WildlifeBirds.pan_compatible());
        assert!(ShootPreset::WildlifeBirds.wants_animals());
        assert_eq!(ShootPreset::Landscape.group_unit(), KeepSet);
        assert_eq!(ShootPreset::Astro.default_blur_strictness(), "lenient");
        assert_eq!(ShootPreset::Landscape.default_blur_strictness(), "strict");
        assert_eq!(ShootPreset::Landscape.min_box_fraction(0.08), 0.08);
        // Aviation asks for the airplane class and reads tail numbers as plates.
        assert_eq!(ShootPreset::Aviation.extra_vehicle_classes(), &[4]);
        assert!(ShootPreset::Aviation.policy().vlm.plates);
        assert!(ShootPreset::Aviation.pan_compatible());
        assert!(ShootPreset::Aviation.wants_vehicles());
        // Only aviation extends the detector's class list.
        for preset in ShootPreset::ALL {
            if preset != ShootPreset::Aviation {
                assert!(preset.extra_vehicle_classes().is_empty(), "{preset:?}");
            }
        }
    }

    #[test]
    fn every_policy_is_internally_consistent() {
        for preset in ShootPreset::ALL {
            let p = preset.policy();
            assert_eq!(p.profile, preset.profile(), "{preset:?}");
            assert!(p.min_box_factor > 0.0 && p.min_box_factor <= 1.0, "{preset:?}");
            assert!(matches!(p.default_blur, "strict" | "moderate" | "lenient"), "{preset:?}");
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

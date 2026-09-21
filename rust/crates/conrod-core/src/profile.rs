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
}

impl ShootPreset {
    pub const ALL: [ShootPreset; 13] = [
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

    pub fn parse(name: &str) -> ShootPreset {
        match name.trim().to_ascii_lowercase().as_str() {
            "portrait" => ShootPreset::Portrait,
            "portrait-group" => ShootPreset::PortraitGroup,
            "portrait-couple" => ShootPreset::PortraitCouple,
            "portrait-pets" => ShootPreset::PortraitPets,
            "motorsport-burnouts" => ShootPreset::MotorsportBurnouts,
            "motorsport-track" => ShootPreset::MotorsportTrack,
            "motorsport-rally" => ShootPreset::MotorsportRally,
            "motorsport-meet" => ShootPreset::MotorsportMeet,
            "event" => ShootPreset::Event,
            "event-shows" => ShootPreset::EventShows,
            "event-parties" => ShootPreset::EventParties,
            "mix" => ShootPreset::Mix,
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
        }
    }

    pub fn profile(self) -> ScanProfile {
        match self {
            ShootPreset::Portrait
            | ShootPreset::PortraitGroup
            | ShootPreset::PortraitCouple
            | ShootPreset::PortraitPets => ScanProfile::Portrait,
            ShootPreset::Motorsport
            | ShootPreset::MotorsportBurnouts
            | ShootPreset::MotorsportTrack
            | ShootPreset::MotorsportRally
            | ShootPreset::MotorsportMeet => ScanProfile::Motorsport,
            ShootPreset::Event | ShootPreset::EventShows | ShootPreset::EventParties => {
                ScanProfile::Event
            }
            ShootPreset::Mix => ScanProfile::Mix,
        }
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
        }
    }

    pub fn wants_vehicles(self) -> bool {
        !matches!(
            self,
            ShootPreset::Portrait
                | ShootPreset::PortraitGroup
                | ShootPreset::PortraitCouple
                | ShootPreset::PortraitPets
                | ShootPreset::EventParties
        )
    }

    pub fn wants_people(self) -> bool {
        true
    }

    pub fn wants_faces(self) -> bool {
        !matches!(
            self,
            ShootPreset::Motorsport
                | ShootPreset::MotorsportBurnouts
                | ShootPreset::MotorsportTrack
                | ShootPreset::MotorsportRally
        )
    }

    pub fn wants_pets(self) -> bool {
        matches!(self, ShootPreset::PortraitPets)
    }

    pub fn priority(self) -> &'static [Subject] {
        self.profile().priority()
    }

    pub fn pan_compatible(self) -> bool {
        matches!(
            self,
            ShootPreset::Motorsport
                | ShootPreset::MotorsportBurnouts
                | ShootPreset::MotorsportTrack
                | ShootPreset::MotorsportRally
                | ShootPreset::Event
                | ShootPreset::EventShows
                | ShootPreset::Mix
        )
    }

    pub fn min_box_fraction(self, baseline: f64) -> f64 {
        let factor = match self {
            ShootPreset::PortraitGroup | ShootPreset::EventShows | ShootPreset::EventParties => 0.5,
            ShootPreset::MotorsportRally | ShootPreset::MotorsportTrack => 0.7,
            _ => 1.0,
        };
        baseline * factor
    }

    pub fn max_subjects(self, baseline: usize) -> usize {
        match self {
            ShootPreset::PortraitGroup | ShootPreset::EventShows | ShootPreset::EventParties => {
                baseline.max(24)
            }
            ShootPreset::MotorsportMeet => baseline.max(16),
            _ => baseline,
        }
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
    /// a driver's eyes.
    pub fn wants_faces(self) -> bool {
        !matches!(self, ScanProfile::Motorsport)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_unknowns_fall_back() {
        for p in ScanProfile::ALL {
            assert_eq!(ScanProfile::parse(p.name()), p);
        }
        assert_eq!(ScanProfile::parse("  Portrait "), ScanProfile::Portrait);
        assert_eq!(ScanProfile::parse("wedding"), ScanProfile::Motorsport);
        for preset in ShootPreset::ALL {
            assert_eq!(ShootPreset::parse(preset.name()), preset);
            assert_eq!(ScanProfile::parse(preset.name()), preset.profile());
        }
    }

    #[test]
    fn every_profile_can_fall_back_to_the_whole_frame() {
        for p in ScanProfile::ALL {
            assert_eq!(p.priority().last(), Some(&Subject::WholeFrame));
        }
    }
}

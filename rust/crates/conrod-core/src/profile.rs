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

impl ScanProfile {
    pub const ALL: [ScanProfile; 4] = [
        ScanProfile::Motorsport,
        ScanProfile::Portrait,
        ScanProfile::Event,
        ScanProfile::Mix,
    ];

    /// The name stored in settings and on the job; unknown names are motorsport.
    pub fn parse(name: &str) -> ScanProfile {
        match name.trim().to_ascii_lowercase().as_str() {
            "portrait" => ScanProfile::Portrait,
            "event" => ScanProfile::Event,
            "mix" => ScanProfile::Mix,
            _ => ScanProfile::Motorsport,
        }
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
    }

    #[test]
    fn every_profile_can_fall_back_to_the_whole_frame() {
        for p in ScanProfile::ALL {
            assert_eq!(p.priority().last(), Some(&Subject::WholeFrame));
        }
    }
}

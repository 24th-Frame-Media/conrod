//! Sport subjects: the ball a player is playing, and the rider on an animal.
//!
//! COCO has no "bull", no "rodeo" and no "goal": a bucking bull arrives as a
//! cow, a horse or a dog, and the class list alone cannot say whether a frame
//! is the ride or the loading chute. What tells them apart is the arrangement
//! of the boxes -- a person sitting on the animal, the animal rearing, the
//! rider thrown forward -- so the rules here are arithmetic on
//! (class, confidence, box) triples, not another model.
//!
//! `conrod-engine`'s `sporttest` measures these rules against a real rodeo
//! shoot; every constant below is stated there in rates before it is trusted.

/// COCO ids this module reads. `conrod-vision::detect` has the same ones.
pub const PERSON: usize = 0;
pub const HORSE: usize = 17;
pub const COW: usize = 19;
pub const SPORTS_BALL: usize = 32;
/// Every COCO animal, in `detect::ANIMALS`' order: bird, cat, dog, horse,
/// sheep, cow, elephant, bear, zebra, giraffe.
pub const ANIMALS: [usize; 10] = [14, 15, 16, 17, 18, 19, 20, 21, 22, 23];

/// A ball this many person-heights from a player is still in play: a ball at
/// arm's length or above the head of the player nearest it.
pub const BALL_REACH: f64 = 1.5;
/// How much of the rider's lower half must land on the animal's back before
/// the pair counts as a rider: a leg over the withers, not a handler beside
/// the head.
pub const RIDER_CONTACT: f64 = 0.05;
/// Where a rider's feet are: above this share of the animal's box, measured
/// from its top, the person is standing on the ground beside it, not riding.
pub const RIDER_FEET: f64 = 0.80;
/// A rider is a real share of the animal, not a speck of a spectator behind a
/// close-up: their box is at least this much of the animal's height. The
/// first pass on the rodeo shoot picked such specks as riders and scored them
/// like a throw (see `sporttest`), which is why this exists.
pub const RIDER_SHARE: f64 = 0.20;
/// A rider sits over the spine: further than this many animal widths from the
/// animal's centre is somebody standing to the side of it.
pub const RIDER_REACH: f64 = 0.50;

/// The detector's box as callers here carry it: COCO class, confidence,
/// `[x1, y1, x2, y2]` in frame pixels.
pub type Det = (usize, f32, [f64; 4]);

fn width(b: &[f64; 4]) -> f64 {
    (b[2] - b[0]).max(0.0)
}

fn height(b: &[f64; 4]) -> f64 {
    (b[3] - b[1]).max(0.0)
}

fn area(b: &[f64; 4]) -> f64 {
    width(b) * height(b)
}

fn centre(b: &[f64; 4]) -> (f64, f64) {
    ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0)
}

fn intersection(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    (a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0)
}

fn union(a: &[f64; 4], b: &[f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// Distance between two boxes' edges, zero when they touch or overlap.
fn gap(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let dx = (a[0].max(b[0]) - a[2].min(b[2])).max(0.0);
    let dy = (a[1].max(b[1]) - a[3].min(b[3])).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// The person nearest the ball, and how far the ball is from that person's
/// box: the gap between the two boxes in person-heights, so 0.0 is a ball
/// being held or bounced at the feet and 1.5 is a ball a body-length away.
///
/// Boxes of the wrong class are ignored, so the whole detection list can be
/// passed. The tallest-and-widest reading is not needed: the nearest pair
/// wins, which for a team sport is the player actually playing the ball.
pub fn ball_context(person_boxes: &[Det], ball_boxes: &[Det]) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (i, (class, _, person)) in person_boxes.iter().enumerate() {
        if *class != PERSON {
            continue;
        }
        let tall = height(person).max(width(person));
        if tall <= 0.0 {
            continue;
        }
        for (ball_class, _, ball) in ball_boxes {
            if *ball_class != SPORTS_BALL {
                continue;
            }
            let distance = gap(person, ball) / tall;
            if best.is_none_or(|(_, d)| distance < d) {
                best = Some((i, distance));
            }
        }
    }
    best
}

/// Is a ball within `BALL_REACH` person-heights of a player?
pub fn ball_in_play(person_boxes: &[Det], ball_boxes: &[Det]) -> bool {
    ball_context(person_boxes, ball_boxes).is_some_and(|(_, d)| d <= BALL_REACH)
}

/// A person riding an animal: the pair, and the shapes that say whether it is
/// a ride or a handler standing about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rider {
    /// Index into the person boxes given to [`rider_on_animal`].
    pub person: usize,
    /// Index into the animal boxes given to it.
    pub animal: usize,
    /// How much of the person's lower half lies on the animal's upper half,
    /// as a share of that lower half: the legs over the back.
    pub overlap: f64,
    /// The person's centre is above the animal's centre, as a sit is.
    pub rider_above: bool,
    /// The animal's box as height/width: 1.0 is square, above that it is
    /// taller than wide (rearing, jumping, nose down), below it a standing
    /// animal seen from the side.
    pub animal_aspect: f64,
    /// The rider's centre against the animal's, in animal widths, signed
    /// left/right: 0.0 seated over the spine, larger thrown forward or back.
    pub rider_offset: f64,
}

/// The best person-on-animal pair, or `None`.
///
/// A rider's legs are in the lower half of the person's box and on the upper
/// half of the animal's, so those two halves must overlap. On top of that the
/// pair has to look like a person on a back rather than a person near an
/// animal, which is what the rodeo shoot taught (see `sporttest`): the rider's
/// feet above `RIDER_FEET` of the animal's box (not on the ground), their box
/// at least `RIDER_SHARE` of the animal's height (not a speck behind a
/// close-up), and their centre within `RIDER_REACH` animal widths of the
/// animal's (over the spine, not beside it). Candidates are then taken in the
/// order (centre above the animal's centre, more leg on the back). Classes
/// outside COCO's animals are ignored.
pub fn rider_on_animal(person_boxes: &[Det], animal_boxes: &[Det]) -> Option<Rider> {
    let mut best: Option<Rider> = None;
    for (i, (class, _, person)) in person_boxes.iter().enumerate() {
        if *class != PERSON {
            continue;
        }
        let (pw, ph) = (width(person), height(person));
        if pw <= 0.0 || ph <= 0.0 {
            continue;
        }
        let (px, py) = centre(person);
        let legs = [person[0], person[1] + ph / 2.0, person[2], person[3]];
        let leg_area = area(&legs).max(1e-9);
        for (j, (animal_class, _, animal)) in animal_boxes.iter().enumerate() {
            if !ANIMALS.contains(animal_class) {
                continue;
            }
            let (aw, ah) = (width(animal), height(animal));
            if aw <= 0.0 || ah <= 0.0 {
                continue;
            }
            let back = [animal[0], animal[1], animal[2], animal[1] + ah / 2.0];
            let overlap = intersection(&legs, &back) / leg_area;
            if overlap < RIDER_CONTACT {
                continue;
            }
            let (ax, ay) = centre(animal);
            let rider_offset = (px - ax) / aw;
            let feet = (person[3] - animal[1]) / ah;
            if feet > RIDER_FEET || ph / ah < RIDER_SHARE || rider_offset.abs() > RIDER_REACH {
                continue;
            }
            let candidate = Rider {
                person: i,
                animal: j,
                overlap,
                rider_above: py < ay,
                animal_aspect: ah / aw,
                rider_offset,
            };
            let better = match best {
                None => true,
                Some(b) => match (candidate.rider_above, b.rider_above) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => candidate.overlap > b.overlap,
                },
            };
            if better {
                best = Some(candidate);
            }
        }
    }
    best
}

/// How much this looks like the peak of the ride, from 0 (animal level and
/// still, rider sitting upright) to 1 (animal rearing, rider thrown and high).
///
/// A heuristic, kept deliberately cheap and linear so its failures are
/// readable: four shapes of the two boxes, weighted, halved when the rider is
/// not above the animal at all.
///
/// `sporttest` checks the order it gives on a real rodeo shoot. What it found:
/// every frame it ranks highest is a riding animal with its front feet off the
/// ground, and the standing ones come last -- but a bull bucking with its body
/// level has the same boxes as a bull standing still, so a level buck ranks
/// with the quiet frames. Read the score as "how much the animal is rearing",
/// not "how good the ride was", and it does its job inside a burst.
pub fn buck_score(rider: &Rider, animal_box: [f64; 4], rider_box: [f64; 4]) -> f64 {
    let (aw, ah) = (width(&animal_box), height(&animal_box));
    if aw <= 0.0 || ah <= 0.0 {
        return 0.0;
    }
    // Rearing: a bull standing side-on is wider than tall (about 0.6-0.7),
    // one that has its front feet in the air comes out square or taller.
    let rear = clamp01((rider.animal_aspect - 0.60) / 0.45);
    // Height in the animal's box: seated on the back is its top third, and a
    // rider lifting off the back is above its whole box.
    let (_, ry) = centre(&rider_box);
    let seat = clamp01((0.45 - (ry - animal_box[1]) / ah) / 0.30);
    // Thrown forward or back rather than sitting over the spine.
    let pitch = clamp01((rider.rider_offset.abs() - 0.05) / 0.30);
    // Legs actually on the animal: a box the detector split reads as no grip.
    let grip = clamp01(rider.overlap);
    let score = 0.40 * rear + 0.30 * seat + 0.20 * pitch + 0.10 * grip;
    if rider.rider_above {
        score
    } else {
        score * 0.5
    }
}

/// What the frame is about, in a sport with animals in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RodeoSubject {
    /// A person on an animal: the pair's union box, so a rating measured on it
    /// sees the animal's head and the rider's hat rather than the animal alone.
    Rider {
        person: usize,
        animal: usize,
        bbox: [f64; 4],
    },
    Animal {
        index: usize,
        bbox: [f64; 4],
    },
    Person {
        index: usize,
        bbox: [f64; 4],
    },
}

impl RodeoSubject {
    pub fn bbox(&self) -> [f64; 4] {
        match *self {
            RodeoSubject::Rider { bbox, .. }
            | RodeoSubject::Animal { bbox, .. }
            | RodeoSubject::Person { bbox, .. } => bbox,
        }
    }

    /// For a caption or a CSV column.
    pub fn kind(&self) -> &'static str {
        match self {
            RodeoSubject::Rider { .. } => "rider",
            RodeoSubject::Animal { .. } => "animal",
            RodeoSubject::Person { .. } => "person",
        }
    }
}

/// The largest box of the wanted classes, by area.
fn largest(boxes: &[Det], keep: impl Fn(usize) -> bool) -> Option<(usize, [f64; 4])> {
    boxes
        .iter()
        .enumerate()
        .filter(|(_, (class, _, b))| keep(*class) && area(b) > 0.0)
        .max_by(|a, b| area(&a.1 .2).total_cmp(&area(&b.1 .2)))
        .map(|(i, (_, _, b))| (i, *b))
}

/// The frame's decisive subject: the rider and the animal together when
/// someone is riding, else the largest animal, else the largest person.
pub fn rodeo_subject(person_boxes: &[Det], animal_boxes: &[Det]) -> Option<RodeoSubject> {
    if let Some(rider) = rider_on_animal(person_boxes, animal_boxes) {
        return Some(RodeoSubject::Rider {
            person: rider.person,
            animal: rider.animal,
            bbox: union(&person_boxes[rider.person].2, &animal_boxes[rider.animal].2),
        });
    }
    if let Some((index, bbox)) = largest(animal_boxes, |c| ANIMALS.contains(&c)) {
        return Some(RodeoSubject::Animal { index, bbox });
    }
    largest(person_boxes, |c| c == PERSON).map(|(index, bbox)| RodeoSubject::Person { index, bbox })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A person standing here, 300 px tall.
    fn person(x: f64, y: f64) -> Det {
        (PERSON, 0.9, [x, y, x + 100.0, y + 300.0])
    }

    /// A ball 20 px across.
    fn ball(x: f64, y: f64) -> Det {
        (SPORTS_BALL, 0.7, [x, y, x + 20.0, y + 20.0])
    }

    /// A bull seen side-on, 400 x 500: taller than wide, so already rearing.
    fn bull(x: f64, y: f64) -> Det {
        (COW, 0.8, [x, y, x + 400.0, y + 500.0])
    }

    #[test]
    fn the_nearest_player_owns_the_ball() {
        let people = [person(100.0, 100.0), person(900.0, 100.0)];
        let balls = [ball(880.0, 380.0)];
        // The second player's right edge is 1000, the ball's left edge 880:
        // overlapping, so the gap is zero and the ball is in play.
        let (who, distance) = ball_context(&people, &balls).expect("a player near the ball");
        assert_eq!(who, 1);
        assert_eq!(distance, 0.0);
        assert!(ball_in_play(&people, &balls));

        let far = [ball(210.0, 380.0)];
        let (who, distance) = ball_context(&people, &far).expect("a player near the ball");
        assert_eq!(who, 0);
        // 10 px clear of the first player's right edge, over a 300 px body.
        assert!((distance - 10.0 / 300.0).abs() < 1e-9, "{distance}");
        assert!(ball_in_play(&people, &far));

        let nowhere = [ball(1900.0, 380.0)];
        let (_, distance) = ball_context(&people, &nowhere).expect("a player somewhere");
        assert!(distance > BALL_REACH, "{distance}");
        assert!(!ball_in_play(&people, &nowhere));
    }

    #[test]
    fn without_a_ball_or_a_player_there_is_no_context() {
        assert_eq!(ball_context(&[], &[ball(10.0, 10.0)]), None);
        assert_eq!(ball_context(&[person(0.0, 0.0)], &[]), None);
        assert!(!ball_in_play(&[], &[]));
        // A car is not a player and a horse is not a ball.
        let cars = [(2, 0.9, [0.0, 0.0, 500.0, 500.0])];
        let horses = [(HORSE, 0.9, [10.0, 10.0, 60.0, 60.0])];
        assert!(!ball_in_play(&cars, &horses));
    }

    #[test]
    fn a_person_on_a_bull_is_a_rider() {
        let people = [person(620.0, 250.0)];
        let herd = [bull(500.0, 400.0)];
        let rider = rider_on_animal(&people, &herd).expect("a rider");
        assert_eq!((rider.person, rider.animal), (0, 0));
        assert!(rider.rider_above);
        assert!(rider.overlap > 0.5, "{}", rider.overlap);
        assert!((rider.animal_aspect - 500.0 / 400.0).abs() < 1e-9);
        // The rider's centre (670) sits 30 px left of the animal's (700), over
        // a 400 px wide animal.
        assert!((rider.rider_offset + 30.0 / 400.0).abs() < 1e-9);
        // Rearing, seated high, level: most of the score from position.
        let score = buck_score(&rider, herd[0].2, people[0].2);
        assert!((0.7..=0.85).contains(&score), "{score}");
    }

    #[test]
    fn a_handler_beside_the_animal_is_not_a_rider() {
        // Same ground line, no leg over the back: the boxes touch at most.
        let people = [person(950.0, 500.0)];
        let herd = [bull(500.0, 400.0)];
        assert_eq!(rider_on_animal(&people, &herd), None);
        // Nor is a person nowhere near the animal.
        let elsewhere = [bull(3000.0, 400.0)];
        assert_eq!(rider_on_animal(&people, &elsewhere), None);
        // And a car's box is not an animal.
        let cars = [(2, 0.9, [500.0, 400.0, 900.0, 900.0])];
        assert_eq!(rider_on_animal(&people, &cars), None);
    }

    #[test]
    fn a_person_standing_on_the_ground_is_not_a_rider() {
        let herd = [(COW, 0.8, [500.0, 500.0, 700.0, 650.0])];
        // Legs on the animal's upper half, but the box runs on down past its
        // feet to the ground: a handler behind a bull in the dust.
        let handler = [(PERSON, 0.9, [480.0, 430.0, 560.0, 640.0])];
        assert_eq!(rider_on_animal(&handler, &herd), None);

        // A speck of a spectator behind a close-up animal: on its back as far
        // as the boxes go, but far too small to be riding it.
        let speck = [(PERSON, 0.5, [520.0, 530.0, 555.0, 550.0])];
        assert_eq!(rider_on_animal(&speck, &herd), None);

        // And somebody level with the animal but half a width to the side.
        let beside = [(PERSON, 0.9, [660.0, 440.0, 760.0, 640.0])];
        assert_eq!(rider_on_animal(&beside, &herd), None);
    }

    #[test]
    fn a_standing_animal_and_rider_score_low() {
        // 600 wide, 380 tall: level and side-on; a small rider sitting over the
        // middle of its back.
        let still = [(COW, 0.8, [500.0, 400.0, 1100.0, 780.0])];
        let seated = [(PERSON, 0.9, [740.0, 430.0, 860.0, 630.0])];
        let quiet = buck_score(
            &rider_on_animal(&seated, &still).expect("a rider"),
            still[0].2,
            seated[0].2,
        );
        // Both front feet down and 400 x 500: rearing, rider high over the
        // shoulders.
        let herd = [(COW, 0.8, [500.0, 400.0, 900.0, 900.0])];
        let lifting = [(PERSON, 0.9, [620.0, 300.0, 740.0, 500.0])];
        let action = buck_score(
            &rider_on_animal(&lifting, &herd).expect("a rider"),
            herd[0].2,
            lifting[0].2,
        );
        assert!(action > quiet, "{action} vs {quiet}");
        assert!(quiet < 0.5, "{quiet}");
    }

    #[test]
    fn a_thrown_rider_scores_higher_than_a_centred_one() {
        let herd = [bull(500.0, 400.0)];
        let level = [person(620.0, 250.0)];
        let thrown = [person(830.0, 210.0)];
        let a = buck_score(
            &rider_on_animal(&level, &herd).expect("a rider"),
            herd[0].2,
            level[0].2,
        );
        let b = buck_score(
            &rider_on_animal(&thrown, &herd).expect("a rider"),
            herd[0].2,
            thrown[0].2,
        );
        assert!(b > a, "{b} vs {a}");
    }

    #[test]
    fn the_rider_is_the_subject_then_the_largest_animal_then_the_largest_person() {
        let people = [person(620.0, 250.0), person(1200.0, 100.0)];
        let herd = [
            bull(500.0, 400.0),
            (HORSE, 0.9, [1400.0, 300.0, 2400.0, 1200.0]),
        ];
        let ridden = rodeo_subject(&people, &herd).expect("a subject");
        assert_eq!(ridden.kind(), "rider");
        assert_eq!(ridden.bbox(), [500.0, 250.0, 900.0, 900.0]);

        // Nobody riding: the biggest animal, here the horse.
        let no_rider = rodeo_subject(&[person(1200.0, 100.0)], &herd).expect("a subject");
        assert!(matches!(no_rider, RodeoSubject::Animal { index: 1, .. }));

        // No animals at all: the biggest person.
        let people_only = rodeo_subject(&[person(0.0, 0.0), person(900.0, 0.0)], &[]);
        assert!(matches!(
            people_only,
            Some(RodeoSubject::Person { index: 1, .. })
        ));
        assert_eq!(rodeo_subject(&[], &[]), None);
    }
}

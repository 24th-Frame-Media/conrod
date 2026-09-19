//! Fixing a make that contradicts the model name beside it.
//!
//! Port of `conrod/marques.py`. The vision model reads nameplates better than
//! badges ("Yamaha Ninja H2"), so a nameplate sold by exactly one marque wins.

/// Nameplate -> the only marque that sells it.
const NAMEPLATES: &[(&str, &str)] = &[
    ("falcon", "Ford"),
    ("fairmont", "Ford"),
    ("territory", "Ford"),
    ("commodore", "Holden"),
    ("monaro", "Holden"),
    ("maloo", "Holden"),
    ("torana", "Holden"),
    ("kingswood", "Holden"),
    ("calais", "Holden"),
    ("statesman", "Holden"),
    ("sandman", "Holden"),
    ("focus", "Ford"),
    ("fiesta", "Ford"),
    ("mustang", "Ford"),
    ("ranger", "Ford"),
    ("impreza", "Subaru"),
    ("liberty", "Subaru"),
    ("forester", "Subaru"),
    ("brz", "Subaru"),
    ("wrx", "Subaru"),
    ("lancer", "Mitsubishi"),
    ("evolution", "Mitsubishi"),
    ("pajero", "Mitsubishi"),
    ("skyline", "Nissan"),
    ("silvia", "Nissan"),
    ("patrol", "Nissan"),
    ("supra", "Toyota"),
    ("corolla", "Toyota"),
    ("hilux", "Toyota"),
    ("landcruiser", "Toyota"),
    ("celica", "Toyota"),
    ("aurion", "Toyota"),
    ("civic", "Honda"),
    ("integra", "Honda"),
    ("nsx", "Honda"),
    ("accord", "Honda"),
    ("golf", "Volkswagen"),
    ("polo", "Volkswagen"),
    ("passat", "Volkswagen"),
    ("rx-7", "Mazda"),
    ("rx-8", "Mazda"),
    ("mx-5", "Mazda"),
    ("cosmo", "Mazda"),
    ("cooper", "MINI"),
    ("clubman", "MINI"),
    ("ninja", "Kawasaki"),
    ("zx-10r", "Kawasaki"),
    ("zx-6r", "Kawasaki"),
    ("z1000", "Kawasaki"),
    ("h2", "Kawasaki"),
    ("yzf", "Yamaha"),
    ("r1", "Yamaha"),
    ("r6", "Yamaha"),
    ("mt-09", "Yamaha"),
    ("hayabusa", "Suzuki"),
    ("gsx-r", "Suzuki"),
    ("gsxr", "Suzuki"),
    ("panigale", "Ducati"),
    ("monster", "Ducati"),
    ("multistrada", "Ducati"),
    ("fireblade", "Honda"),
    ("cbr", "Honda"),
];

/// Words as `[a-z0-9][a-z0-9-]*` finds them in the lowercased model name.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        let starts = c.is_ascii_lowercase() || c.is_ascii_digit();
        if starts || (c == '-' && !current.is_empty()) {
            current.push(c);
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// The make the model name implies, if it plainly contradicts the badge.
pub fn correct_make(make: Option<&str>, model: Option<&str>) -> Option<String> {
    let Some(model) = model.filter(|m| !m.is_empty()) else {
        return make.map(str::to_string);
    };
    for token in words(&model.to_lowercase()) {
        let Some(&(_, owner)) = NAMEPLATES.iter().find(|(name, _)| *name == token) else {
            continue;
        };
        return match make {
            Some(make)
                if !make.is_empty() && make.trim().to_lowercase() == owner.to_lowercase() =>
            {
                Some(make.to_string())
            }
            _ => Some(owner.to_string()),
        };
    }
    make.map(str::to_string)
}

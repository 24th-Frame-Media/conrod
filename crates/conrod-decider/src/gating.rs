//! Which quantization a machine can run, from the video memory it has.

/// What gets installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quant {
    /// The official Q8_0 GGUF (near-lossless).
    Int8,
    /// Re-quantized from the Q8_0 on the machine (smaller, a little less exact).
    Int4,
}

pub const GIB: u64 = 1 << 30;

impl Quant {
    pub fn label(self) -> &'static str {
        match self {
            Quant::Int8 => "int8",
            Quant::Int4 => "int4",
        }
    }
    /// Video memory it needs free: weights, vision projector and compute buffers for a
    /// typical request (measured, see `docs/research/decider-runtime.md`).
    pub fn vram_needed(self) -> u64 {
        match self {
            Quant::Int8 => 6 * GIB + GIB / 2,
            Quant::Int4 => 4 * GIB + GIB / 2,
        }
    }
}

/// The quantization to use, or `None` when the GPU cannot hold even the smallest.
/// `forced` is the user's override; a forced one that does not fit is still refused.
pub fn choose(free_bytes: u64, forced: Option<Quant>) -> Option<Quant> {
    let fits = |q: Quant| free_bytes >= q.vram_needed();
    match forced {
        Some(q) => fits(q).then_some(q),
        None if fits(Quant::Int8) => Some(Quant::Int8),
        None if fits(Quant::Int4) => Some(Quant::Int4),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_biggest_quantization_that_fits_wins() {
        assert_eq!(choose(12 * GIB, None), Some(Quant::Int8));
        assert_eq!(choose(5 * GIB, None), Some(Quant::Int4));
        assert_eq!(choose(7 * GIB, None), Some(Quant::Int8));
        assert_eq!(choose(3 * GIB, None), None);
    }

    #[test]
    fn a_forced_choice_must_still_fit() {
        assert_eq!(choose(12 * GIB, Some(Quant::Int4)), Some(Quant::Int4));
        assert_eq!(choose(5 * GIB, Some(Quant::Int8)), None);
    }
}

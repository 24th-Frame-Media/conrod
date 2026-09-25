//! Ridge regression, in the two flavours the app uses: standardised
//! hand-built features onto a one-to-five sharpness rating, and unit
//! embeddings onto the stars a photographer gives. Both are a linear model
//! stored as a list of numbers, so a model file trained once is read here
//! unchanged regardless of which trainer produced it.

use serde::{Deserialize, Serialize};

/// Bump when the sharpness feature vector changes meaning or length. A model
/// stored under another version describes a different vector and is ignored.
pub const FEATURE_VERSION: u32 = 1;
/// Below this the sharpness fit is noise.
pub const SHARP_MIN_LABELS: usize = 60;
/// Ridge penalty on standardised sharpness features.
pub const SHARP_PENALTY: f64 = 2.0;
/// Below this the taste fit is noise (it has 384 inputs).
pub const TASTE_ENOUGH_RATINGS: usize = 200;
/// Ridge penalty for the taste model.
pub const TASTE_PENALTY: f64 = 0.3;

/// Solve `a x = b` by Gaussian elimination with partial pivoting, the same
/// method LAPACK's `gesv` uses under `numpy.linalg.solve`. `None` if singular.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col] == 0.0 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let factor = a[row][col] / a[col][col];
            let (upper, lower) = a.split_at_mut(row);
            for (target, above) in lower[0][col..].iter_mut().zip(&upper[col][col..]) {
                *target -= factor * above;
            }
            let above = b[col];
            b[row] -= factor * above;
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let tail: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - tail) / a[row][row];
    }
    Some(x)
}

/// Solve the ridge normal equations for `design` (which already carries a
/// trailing column of ones) with the intercept left unpenalised.
fn ridge(design: &[Vec<f64>], target: &[f64], penalty: f64) -> Option<Vec<f64>> {
    let width = design.first()?.len();
    let mut gram = vec![vec![0.0; width]; width];
    let mut moment = vec![0.0; width];
    for (row, &y) in design.iter().zip(target) {
        for i in 0..width {
            moment[i] += row[i] * y;
            for j in 0..width {
                gram[i][j] += row[i] * row[j];
            }
        }
    }
    for (i, diagonal) in gram.iter_mut().enumerate().take(width - 1) {
        diagonal[i] += penalty;
    }
    solve(gram, moment)
}

fn distinct(target: &[f64]) -> bool {
    target.iter().any(|&y| y != target[0])
}

// --- sharpness --------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharpModel {
    pub version: u32,
    pub trained_on: usize,
    pub mean: Vec<f64>,
    pub spread: Vec<f64>,
    pub weights: Vec<f64>,
    pub intercept: f64,
}

/// Fit sharpness ratings to feature vectors. `None` with too few ratings, with
/// ragged vectors, or when every rating is the same.
pub fn fit_sharp(vectors: &[Vec<f64>], stars: &[f64]) -> Option<SharpModel> {
    let dims = vectors.first()?.len();
    if vectors.len() < SHARP_MIN_LABELS
        || vectors.len() != stars.len()
        || dims == 0
        || vectors.iter().any(|v| v.len() != dims)
        || !distinct(stars)
    {
        return None;
    }
    let n = vectors.len() as f64;
    let mean: Vec<f64> = (0..dims)
        .map(|d| vectors.iter().map(|v| v[d]).sum::<f64>() / n)
        .collect();
    let spread: Vec<f64> = (0..dims)
        .map(|d| {
            let variance = vectors
                .iter()
                .map(|v| (v[d] - mean[d]).powi(2))
                .sum::<f64>()
                / n;
            variance.sqrt().max(1e-6)
        })
        .collect();
    let design: Vec<Vec<f64>> = vectors
        .iter()
        .map(|v| {
            let mut row: Vec<f64> = (0..dims).map(|d| (v[d] - mean[d]) / spread[d]).collect();
            row.push(1.0);
            row
        })
        .collect();
    let mut weights = ridge(&design, stars, SHARP_PENALTY)?;
    let intercept = weights.pop()?;
    Some(SharpModel {
        version: FEATURE_VERSION,
        trained_on: stars.len(),
        mean,
        spread,
        weights,
        intercept,
    })
}

/// The rating this photographer would give, on the one-to-five scale but not
/// rounded or clamped. `None` for a model of another version or a vector of the
/// wrong length.
pub fn predict_sharp(model: &SharpModel, vector: &[f64]) -> Option<f64> {
    if model.version != FEATURE_VERSION || vector.is_empty() || model.mean.len() != vector.len() {
        return None;
    }
    let dot: f64 = vector
        .iter()
        .zip(&model.mean)
        .zip(&model.spread)
        .zip(&model.weights)
        .map(|(((v, m), s), w)| (v - m) / s * w)
        .sum();
    Some(dot + model.intercept)
}

// --- taste ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TasteModel {
    /// One weight per embedding dimension, then the intercept.
    pub weights: Vec<f64>,
    pub trained_on: usize,
}

/// Fit the stars a photographer gives to unit embeddings. `None` below
/// [`TASTE_ENOUGH_RATINGS`] or when every rating is the same.
pub fn fit_taste(vectors: &[Vec<f64>], stars: &[f64]) -> Option<TasteModel> {
    let dims = vectors.first()?.len();
    if vectors.len() < TASTE_ENOUGH_RATINGS
        || vectors.len() != stars.len()
        || dims == 0
        || vectors.iter().any(|v| v.len() != dims)
        || !distinct(stars)
    {
        return None;
    }
    let design: Vec<Vec<f64>> = vectors
        .iter()
        .map(|v| {
            let mut row = v.clone();
            row.push(1.0);
            row
        })
        .collect();
    let weights = ridge(&design, stars, TASTE_PENALTY)?;
    Some(TasteModel {
        weights,
        trained_on: stars.len(),
    })
}

/// The star this photographer would probably give this crop, one to five.
pub fn predict_taste(model: &TasteModel, vector: &[f64]) -> Option<i32> {
    if model.weights.len() != vector.len() + 1 {
        return None;
    }
    let (intercept, weights) = model.weights.split_last()?;
    let value: f64 = vector.iter().zip(weights).map(|(v, w)| v * w).sum::<f64>() + intercept;
    // Round-half-to-even, matching the convention the model was trained
    // against.
    Some(value.round_ties_even().clamp(1.0, 5.0) as i32)
}

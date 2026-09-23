//! PaddleOCR inference through oar-ocr, with Conrod's number/text rules.
use crate::imageops::Rgb;
use conrod_core::settings::Settings;
use oar_ocr::{
    core::config::OrtSessionConfig,
    domain::tasks::{TextDetectionConfig, TextRecognitionConfig},
    oarocr::{OAROCRBuilder, OAROCR},
    processors::LimitType,
};
use std::{collections::HashSet, path::Path};

pub struct Ocr {
    engine: OAROCR,
}
#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub confidence: f64,
    pub area: f64,
}

impl Ocr {
    pub fn load(models: &Path) -> Result<Self, String> {
        // RapidOCR 1.3+ (what the Python build bundles) ships PP-OCRv4; v3 is
        // its older default and still accepted.
        let pick = |kind: &str| {
            ["v4", "v3"]
                .map(|v| models.join(format!("ch_PP-OCR{v}_{kind}_infer.onnx")))
                .into_iter()
                .find(|p| p.is_file())
                .unwrap_or_else(|| models.join(format!("ch_PP-OCRv4_{kind}_infer.onnx")))
        };
        let (det, rec) = (pick("det"), pick("rec"));
        let engine = OAROCRBuilder::new(det, rec, "")
            .character_dict_content(include_str!("ocr_charset.txt"))
            .ort_session(OrtSessionConfig::new().with_intra_threads(1))
            .text_detection_config(TextDetectionConfig {
                score_threshold: 0.3,
                box_threshold: 0.5,
                unclip_ratio: 1.6,
                // RapidOCR's own defaults: grow small crops until their short
                // side is 736, never past 2000, and drop reads under 0.5.
                limit_side_len: Some(736),
                limit_type: Some(LimitType::Min),
                max_side_len: Some(2000),
                ..Default::default()
            })
            .text_recognition_config(TextRecognitionConfig {
                score_threshold: 0.5,
            })
            .region_batch_size(6)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { engine })
    }
    pub fn read(&self, image: &Rgb) -> Result<Vec<Token>, String> {
        let rgb =
            image::RgbImage::from_raw(image.width as u32, image.height as u32, image.data.clone())
                .ok_or("Invalid OCR image")?;
        let results = self.engine.predict(vec![rgb]).map_err(|e| e.to_string())?;
        Ok(results
            .into_iter()
            .flat_map(|r| r.text_regions)
            .filter_map(|r| {
                let (text, confidence) = r.text_with_confidence()?;
                Some(Token {
                    text: text.to_string(),
                    confidence: f64::from(confidence),
                    area: f64::from(r.bounding_box.area()) / 1_000_000.0,
                })
            })
            .collect())
    }
}

pub fn read_number(tokens: &[Token], settings: &Settings) -> Option<(String, f64)> {
    tokens
        .iter()
        .filter_map(|t| {
            let token = t
                .text
                .trim()
                .trim_matches(|c: char| !c.is_ascii_alphanumeric());
            let digits = token.chars().filter(char::is_ascii_digit).count();
            let candidate: String = if digits > 0 && digits + 1 >= token.chars().count() {
                token
                    .chars()
                    .map(|c| match c {
                        'O' | 'o' => '0',
                        'I' | 'l' => '1',
                        'S' => '5',
                        'B' => '8',
                        'Z' => '2',
                        'G' => '6',
                        c => c,
                    })
                    .collect()
            } else {
                token.into()
            };
            let n = candidate.len() as i64;
            if n < settings.number_min_len
                || n > settings.number_max_len
                || candidate.starts_with("00")
                || !candidate.chars().all(|c| c.is_ascii_digit())
            {
                return None;
            }
            Some((candidate, t.confidence * (1.0 + t.area)))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(n, c)| (n, c.min(1.0)))
}

pub fn visible_text(tokens: &[Token], settings: &Settings, exclude: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = exclude.iter().map(|s| s.to_uppercase()).collect();
    tokens
        .iter()
        .filter_map(|t| {
            let text = t
                .text
                .trim()
                .trim_matches(|c: char| !c.is_ascii_alphanumeric());
            let key: String = text
                .to_uppercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect();
            if t.confidence < settings.text_min_confidence
                || (text.chars().count() as i64) < settings.text_min_length
                || key.is_empty()
                || !seen.insert(key)
            {
                return None;
            }
            Some(text.to_string())
        })
        .take(settings.max_text_items.max(0) as usize)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lookalikes_need_a_numeric_token() {
        let s = Settings::default();
        let token = |text: &str| Token {
            text: text.into(),
            confidence: 0.9,
            area: 0.01,
        };
        assert_eq!(read_number(&[token("BOSS")], &s), None);
        assert_eq!(read_number(&[token("(2O)")], &s).unwrap().0, "20");
        assert_eq!(read_number(&[token("007")], &s), None);
        assert_eq!(read_number(&[token("07")], &s).unwrap().0, "07");
    }
}

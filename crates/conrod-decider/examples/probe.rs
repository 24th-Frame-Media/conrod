//! Smoke test and benchmark against real files:
//! `probe <runtime-dir> <model.gguf> <mmproj.gguf> [image|-] [cpu|gpu] [max_pixels]`
//! `probe quantize <runtime-dir> <in-q8.gguf> <out-q4.gguf>`
use conrod_decider::*;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a[1] == "quantize" {
        let t = Instant::now();
        Decider::quantize_int4(a[2].as_ref(), a[3].as_ref(), a[4].as_ref()).unwrap();
        println!("quantized in {:?}", t.elapsed());
        return;
    }
    let paths = DeciderPaths::new(&a[1], &a[2], &a[3]);
    for d in Decider::devices(&paths.runtime).unwrap() {
        println!(
            "device {} {} [{}] gpu={} free={:.2}GiB total={:.2}GiB",
            d.index,
            d.name,
            d.description,
            d.gpu,
            d.free as f64 / 1073741824.0,
            d.total as f64 / 1073741824.0
        );
    }
    let device = match a.get(5).map(|s| s.as_str()) {
        Some("cpu") => Device::Cpu,
        _ => Device::Auto,
    };
    let t = Instant::now();
    let decider = Decider::load(
        &paths,
        &LoadOptions {
            device,
            max_pixels: a.get(6).and_then(|s| s.parse().ok()).unwrap_or(MAX_PIXELS),
            ..LoadOptions::default()
        },
    )
    .unwrap();
    println!(
        "loaded in {:?} on {:?}, temperature {}",
        t.elapsed(),
        decider.device.as_ref().map(|d| &d.name),
        decider.temperature
    );
    let mut request = Request::new(
        "My integration keeps failing with a 401 since we rotated our API key. Please help.",
    )
    .ask(
        "team",
        Question::choice(
            "Which team should handle this request?",
            [
                ("billing", Some("Charges and refunds".to_string())),
                ("support", Some("Technical integration errors".to_string())),
                (
                    "sales",
                    Some("Questions about buying a product".to_string()),
                ),
            ],
        ),
    )
    .ask(
        "urgent",
        Question::noul("Is the customer blocked right now?"),
    )
    .ask(
        "frustration",
        Question::score(
            "How frustrated is the customer?",
            &["calm", "mildly annoyed", "frustrated", "angry"],
        ),
    );
    if let Some(img) = a.get(4).filter(|s| s.as_str() != "-") {
        request = Request::new("")
            .ask("sharp", Question::noul("Is this photo sharp?"))
            .ask("people", Question::noul("Are there people in the photo?"));
        request.images.push(img.clone());
    }
    let gib = |b: u64| b as f64 / 1073741824.0;
    if let Some((free, total)) = decider.device_memory() {
        println!("after load: free {:.2} / {:.2} GiB", gib(free), gib(total));
    }
    for i in 0..6 {
        let (r, t) = decider.decide_timed(&request).unwrap();
        println!(
            "run {i}: {} tokens, encode {:?}, decode {:?}",
            t.tokens, t.encode, t.decode
        );
        if let Some((free, total)) = decider.device_memory() {
            println!("  free {:.2} / {:.2} GiB", gib(free), gib(total));
        }
        if i == 0 {
            println!("{}", serde_json::to_string_pretty(&r).unwrap());
        }
    }
}

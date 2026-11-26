#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod frame;
mod i18n;
mod metrics;
mod oled;
mod ui;

fn main() {
    if std::env::args().any(|arg| arg == "--diagnose") {
        let sources = metrics::sources();
        let mut s = metrics::Sampler::new();
        std::thread::sleep(std::time::Duration::from_secs(2));
        for (src, value) in sources.iter().zip(s.sample(&sources)) {
            println!(
                "{}: {}",
                src.name,
                value.map_or_else(|| "N/A".into(), |v| format!("{v}%"))
            );
        }
        match oled::Display::open() {
            Ok(d) => println!(
                "M701: {} · battery {} (no image uploaded)",
                d.link,
                d.battery()
                    .map_or_else(|| "N/A".into(), |v| format!("{v}%"))
            ),
            Err(e) => println!("M701: {e}"),
        };
        return;
    }
    unsafe { ui::run() }
}

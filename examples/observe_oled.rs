//! Passive, USB M701 only. Count known OLED ACK headers by second; never send a report.
use hidapi::HidApi;
use std::{
    collections::BTreeMap,
    error::Error,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn stamp() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        % 86_400_000;
    format!(
        "{:02}:{:02}:{:02}.{:03}Z",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

// Watch the vendor OLED control collection only. Keep timestamps and command
// headers, not pixel payloads. No mode changes and no device writes.
fn timeline(dev: &hidapi::HidDevice, seconds: u64) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let mut last_header = None::<Instant>;
    let mut pending = None::<(Instant, u32, Option<u16>, u32)>;
    let mut buffer = [0; 65];
    let mut last_reply = Instant::now();
    let mut idle_reported = false;
    println!(
        "{} start: USB M701, passive OLED trace, {seconds}s",
        stamp()
    );
    while start.elapsed() < Duration::from_secs(seconds) {
        let n = dev
            .read_timeout(&mut buffer, 100)
            .map_err(|e| format!("{} control read failed: {e}", stamp()))?;
        let now = Instant::now();
        if n < 2 {
            if !idle_reported && last_reply.elapsed() >= Duration::from_secs(2) {
                println!("{} 2s without OLED acknowledgement", stamp());
                idle_reported = true;
            }
            continue;
        }
        let key = (buffer[0], buffer[1]);
        // Profile changes can replace the displayed OLED preset without a
        // 61:00. Record the command family, not report payloads.
        if key == (0x51, 0x00) {
            println!("{} profile selection ACK (51:00)", stamp());
            continue;
        }
        if !(0x61..=0x69).contains(&key.0) && key != (0xff, 0xaa) {
            continue;
        }
        last_reply = now;
        idle_reported = false;
        match key {
            (0x67, 0) => {
                if let Some((_, count, _, _)) = pending.take() {
                    println!(
                        "{} new header before end of observed frame ({count} chunks)",
                        stamp()
                    );
                }
                let gap = last_header
                    .replace(now)
                    .map(|t| now.duration_since(t).as_millis());
                println!("{} media header; interval_ms={gap:?}", stamp());
                pending = Some((now, 0, None, 0));
            }
            (0x67, 1) if n >= 4 => {
                if let Some((begin, count, last, anomalies)) = pending.as_mut() {
                    let index = u16::from_le_bytes([buffer[2], buffer[3]]);
                    if last.is_some_and(|prev| prev.checked_sub(1) != Some(index)) {
                        *anomalies += 1;
                    }
                    *last = Some(index);
                    *count += 1;
                    if buffer[2..4] == [0, 0] {
                        println!(
                            "{} last chunk; chunks={count} index_jumps={anomalies} transfer_ms={}",
                            stamp(),
                            now.duration_since(*begin).as_millis()
                        );
                        pending = None;
                    }
                }
            }
            (0x61, 0) => println!("{} preset selection ACK (61:00)", stamp()),
            (0xff, 0xaa) => println!("{} firmware rejection", stamp()),
            // Bulk data for unrelated image/banner uploads need only their mode headers.
            (0x61, 2) | (0x62, 1) | (0x67, 2) => {}
            (cmd, op) => println!("{} OLED ACK {cmd:02x}:{op:02x}", stamp()),
        }
    }
    println!("{} finished", stamp());
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let api = HidApi::new()?;
    let mut controls = api.device_list().filter(|d| {
        d.vendor_id() == 0x0b05 && d.product_id() == 0x1a83 && d.usage_page() == 0xff00
    });
    let info = controls
        .next()
        .ok_or("USB M701 OLED control interface not found")?;
    if controls.next().is_some() {
        return Err("Multiple M701 interfaces".into());
    }
    let dev = info.open_device(&api)?;
    if let Some(arg) = std::env::args().nth(1) {
        if arg != "--timeline" {
            return Err("Usage: observe_oled [--timeline 1..900]".into());
        }
        let seconds: u64 = std::env::args()
            .nth(2)
            .ok_or("Missing duration in seconds")?
            .parse()?;
        if !(1..=900).contains(&seconds) {
            return Err("Duration must be 1..900 seconds".into());
        }
        return timeline(&dev, seconds);
    }
    let start = Instant::now();
    let mut totals = BTreeMap::<(u64, u8, u8), usize>::new();
    let mut other = 0usize;
    let mut buffer = [0u8; 65];
    while start.elapsed() < Duration::from_secs(12) {
        let n = dev.read_timeout(&mut buffer, 100)?;
        if n < 2 {
            continue;
        }
        match buffer[0] {
            0x61..=0x69 => {
                *totals
                    .entry((start.elapsed().as_secs(), buffer[0], buffer[1]))
                    .or_default() += 1
            }
            _ => other += 1,
        }
    }
    println!("second  command  count");
    for ((sec, cmd, op), count) in totals {
        println!("{sec:6}  {cmd:02x}:{op:02x}   {count:5}");
    }
    println!("Other control reports (not decoded): {other}");
    Ok(())
}

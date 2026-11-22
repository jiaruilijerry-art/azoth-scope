use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
pub const W: usize = 208;
pub const H: usize = 64;
pub const BANNER_H: usize = 46;
#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Surface {
    #[default]
    Image,
    Banner,
    NativeBars,
}
impl Surface {
    pub fn height(self) -> usize {
        match self {
            Self::Image | Self::NativeBars => H,
            Self::Banner => BANNER_H,
        }
    }
}
pub const MAX_MINUTES: u8 = 30;
pub const MAX_SECONDS: u8 = 30;
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Timing {
    pub minutes: u8,
    pub seconds: u8,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            minutes: 2,
            seconds: 2,
        }
    }
}
impl Timing {
    pub fn normalize(&mut self) {
        self.minutes = self.minutes.clamp(1, MAX_MINUTES);
        self.seconds = self.seconds.clamp(1, MAX_SECONDS);
    }
    pub fn interval(self) -> Duration {
        Duration::from_secs(self.seconds.into())
    }
    pub fn window(self, end: Instant) -> Window {
        Window {
            end,
            span_ms: u64::from(self.minutes) * 60_000,
        }
    }
}
pub type History = VecDeque<(Instant, Option<u8>)>;
#[derive(Clone, Copy)]
pub struct Window {
    end: Instant,
    span_ms: u64,
}
impl Window {
    fn visible(self, h: &History) -> impl Iterator<Item = (u64, Option<u8>)> + '_ {
        h.iter()
            .rev()
            .map(move |(t, v)| {
                (
                    self.end.saturating_duration_since(*t).as_millis() as u64,
                    *v,
                )
            })
            .take_while(move |(age, _)| *age < self.span_ms)
    }
}
pub type Frame = Vec<u8>;
pub type Bands = [u8; 26];
pub const TITLE_H: usize = 32;
#[derive(Clone, PartialEq, Eq)]
pub struct Legend {
    pub first: String,
    pub second: Option<String>,
}
impl Legend {
    pub fn pixels(&self) -> Frame {
        let mut f = vec![0; W * TITLE_H];
        let draw = |f: &mut [u8], name: &str, cx: i32| {
            let width = name.len() as i32 * 8 - 2;
            text(f, name, cx - width / 2, (TITLE_H as i32 - 10) / 2, 2, 255);
        };
        if let Some(b) = &self.second {
            draw(&mut f, &self.first, 48);
            draw(&mut f, b, 160);
        } else {
            draw(&mut f, &self.first, 104);
        }
        f
    }
}
#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GraphStyle {
    #[default]
    Bar,
    Line,
    JoinedLine,
}
impl GraphStyle {
    pub fn mode_header(self) -> [u8; 12] {
        let id = match self {
            Self::Bar => 1,
            Self::Line | Self::JoinedLine => 3,
        };
        [0x67, 0, 0, 0, 1, id, 0, 0, 208, 0, 64, 0]
    }
}

// The keyboard has 26 fixed spectrum columns. Anchor the latest sample at the right
// of each plot; a short spike must stay visible when several samples share a bar.
pub fn history_bands(first: &History, second: Option<&History>, window: Window) -> Bands {
    let fill = |out: &mut [u8], history: &History| {
        for (age, v) in window.visible(history) {
            if let Some(v) = v {
                let i = out.len() - 1 - (age * out.len() as u64 / window.span_ms) as usize;
                out[i] = out[i].max(((v.min(100) as u16 * 255 + 50) / 100) as u8);
            }
        }
    };
    let mut bars = [0; 26];
    if let Some(h) = second {
        fill(&mut bars[..12], first);
        fill(&mut bars[14..], h);
    } else {
        fill(&mut bars, first);
    }
    bars
}
pub fn preview_bands(
    bars: &Bands,
    style: GraphStyle,
    dual: bool,
    legend: Option<&Legend>,
) -> Frame {
    let mut f = vec![0; W * H];
    let plot_h = if let Some(l) = legend {
        f[..W * TITLE_H].copy_from_slice(&l.pixels());
        H - TITLE_H
    } else {
        H
    };
    let mut prev = None;
    for (n, &v) in bars.iter().enumerate() {
        if dual && style != GraphStyle::JoinedLine && (12..14).contains(&n) {
            prev = None;
            continue;
        }
        if style != GraphStyle::Bar {
            let p = (
                (n * 8 + 3) as i32,
                (H - 1 - (v as usize * (plot_h - 1) + 127) / 255) as i32,
            );
            line(&mut f, prev.unwrap_or(p), p, 255);
            prev = Some(p);
            continue;
        }
        let height = (v as usize * plot_h).div_ceil(255);
        for y in H - height..H {
            f[y * W + n * 8..y * W + n * 8 + 6].fill(255);
        }
    }
    f
}

pub fn push(h: &mut History, now: Instant, v: Option<u8>) {
    while h.front().is_some_and(|(t, _)| {
        now.saturating_duration_since(*t) >= Duration::from_secs(u64::from(MAX_MINUTES) * 60)
    }) {
        h.pop_front();
    }
    h.push_back((now, v));
}

/// Two independent paths. A zero in the spectrum stream is still a point,
/// so leaving two bins at zero cannot make a real break in firmware Line mode.
pub fn split_lines(
    left: &History,
    right: &History,
    legend: Option<&Legend>,
    window: Window,
) -> Frame {
    let mut f = vec![0; W * H];
    if let Some(l) = legend {
        f[..W * TITLE_H].copy_from_slice(&l.pixels());
    }
    let bottom = H as i32 - 1 - 4;
    let top = if legend.is_some() {
        TITLE_H as i32 - 4
    } else {
        0
    };
    for (h, x0) in [(left, 4), (right, 112)] {
        let mut previous = None;
        for (age, sample) in window.visible(h) {
            let Some(value) = sample else {
                previous = None;
                continue;
            };
            let x = x0 + 91 - (age * 91 / window.span_ms) as i32;
            let y = bottom - ((bottom - top) * i32::from(value.min(100)) + 50) / 100;
            line(&mut f, previous.unwrap_or((x, y)), (x, y), 255);
            previous = Some((x, y));
        }
    }
    f
}
fn dot(f: &mut [u8], x: i32, y: i32, v: u8) {
    if (0..W as i32).contains(&x) && (0..(f.len() / W) as i32).contains(&y) {
        f[y as usize * W + x as usize] = v;
    }
}
fn line(f: &mut [u8], (mut x, mut y): (i32, i32), (x1, y1): (i32, i32), v: u8) {
    let dx = (x1 - x).abs();
    let dy = -(y1 - y).abs();
    let sx = (x1 - x).signum();
    let sy = (y1 - y).signum();
    let mut err = dx + dy;
    loop {
        dot(f, x, y, v);
        if (x, y) == (x1, y1) {
            break;
        }
        let e = 2 * err;
        if e >= dy {
            err += dy;
            x += sx;
        }
        if e <= dx {
            err += dx;
            y += sy;
        }
    }
}
fn glyph(c: char) -> [u8; 5] {
    match c {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 7, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 2, 2, 2],
        '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 7],
        'C' => [7, 4, 4, 4, 7],
        'P' => [7, 5, 7, 4, 4],
        'U' => [5, 5, 5, 5, 7],
        'G' => [7, 4, 5, 5, 7],
        '%' => [5, 1, 2, 4, 5],
        '-' => [0, 0, 7, 0, 0],
        _ => [0; 5],
    }
}
fn text(f: &mut [u8], s: &str, x: i32, y: i32, scale: i32, v: u8) {
    for (n, c) in s.chars().enumerate() {
        for (j, b) in glyph(c).iter().enumerate() {
            for i in 0..3 {
                if b & (4 >> i) != 0 {
                    for a in 0..scale {
                        for b in 0..scale {
                            dot(
                                f,
                                x + (n as i32 * 4 + i) * scale + a,
                                y + j as i32 * scale + b,
                                v,
                            );
                        }
                    }
                }
            }
        }
    }
}
pub fn render(rows: &[(&str, &History)], surface: Surface, window: Window) -> Frame {
    let mut f = vec![0; W * surface.height()];
    let height = surface.height() as i32 / rows.len().clamp(1, 2) as i32;
    for (n, (name, h)) in rows.iter().take(2).enumerate() {
        let top = n as i32 * height;
        let bottom = top + height - 3;
        let y0 = top + 2;
        if n > 0 {
            line(&mut f, (0, top), (207, top), 55);
        }
        text(&mut f, name, 3, top + (height / 2 - 12).max(1), 2, 170);
        let v = h
            .back()
            .and_then(|(_, v)| *v)
            .map(|x| format!("{x}%"))
            .unwrap_or_else(|| "--%".into());
        text(
            &mut f,
            &v,
            3,
            top + (height / 2 + 2).min(height - 11),
            2,
            255,
        );
        for x in (39..W as i32).step_by(4) {
            for y in [y0, (y0 + bottom) / 2, bottom] {
                dot(&mut f, x, y, 51);
            }
        }
        let mut prev = None;
        for (age, v) in window.visible(h) {
            let Some(v) = v else {
                prev = None;
                continue;
            };
            let p = (
                207 - (age * 168 / window.span_ms) as i32,
                bottom - (bottom - y0) * v.min(100) as i32 / 100,
            );
            line(&mut f, prev.unwrap_or(p), p, 255);
            prev = Some(p);
        }
    }
    f
}
#[cfg(test)]
mod tests {
    use super::*;
    fn samples(values: &[Option<u8>], end: Instant) -> History {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                (
                    end - Duration::from_secs(2 * (values.len() - i - 1) as u64),
                    *v,
                )
            })
            .collect()
    }
    #[test]
    fn split_line_never_draws_in_gutter_or_lower_margin() {
        let end = Instant::now();
        let left = samples(&[Some(0); 60], end);
        let right = samples(&[Some(100); 60], end);
        for names in [
            None,
            Some(Legend {
                first: "CPU".into(),
                second: Some("GPU0".into()),
            }),
        ] {
            let f = split_lines(&left, &right, names.as_ref(), Timing::default().window(end));
            assert!(f
                .as_chunks::<W>()
                .0
                .iter()
                .all(|row| row[96..112].iter().all(|&v| v == 0)));
            assert!(f[60 * W..].iter().all(|&v| v == 0));
            assert_eq!(f[59 * W + 95], 255);
            assert_eq!(f[if names.is_some() { 28 * W + 203 } else { 203 }], 255);
        }
    }
    #[test]
    fn two_histories_keep_peaks_and_do_not_bleed_across_gap() {
        let end = Instant::now();
        let mut cpu = samples(&[Some(0); 60], end);
        cpu[0].1 = Some(80);
        cpu[56].1 = Some(100);
        let gpu = samples(&[None, Some(50)], end);
        let b = history_bands(&cpu, Some(&gpu), Timing::default().window(end));
        assert_eq!((b[0], b[11], b[25]), (204, 255, 128));
        assert!(b[12..25].iter().all(|&x| x == 0));
    }
    #[test]
    fn missing_sample_breaks_line() {
        let end = Instant::now();
        let h = samples(&[Some(100), None, Some(0)], end);
        let f = render(
            &[("CPU", &h)],
            Surface::Image,
            Timing::default().window(end),
        );
        assert_eq!(f[61 * W + 207], 255);
        assert_eq!(f[2 * W + 202], 255);
        assert_ne!(f[31 * W + 205], 255);
    }
    #[test]
    fn zoom_uses_timestamps_even_when_sampling_interval_changed() {
        let end = Instant::now();
        let mut h = History::new();
        for (age, v) in [(121, 100), (60, 80), (9, 50), (1, 25), (0, 0)] {
            push(&mut h, end - Duration::from_secs(age), Some(v));
        }
        let wide = history_bands(
            &h,
            None,
            Timing {
                minutes: 2,
                seconds: 1,
            }
            .window(end),
        );
        let narrow = history_bands(
            &h,
            None,
            Timing {
                minutes: 1,
                seconds: 10,
            }
            .window(end),
        );
        assert_eq!(wide[12], 204);
        assert_eq!(narrow[22], 128);
        assert_eq!(narrow[25], 64);
        assert_eq!(narrow.iter().max(), Some(&128));
        assert_eq!(h.len(), 5);
    }
}

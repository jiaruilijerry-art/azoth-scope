use crate::i18n::{Language, OutputStatus};
use crate::{
    frame::{self, Frame, History},
    metrics::{self, Source},
    oled,
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Prefs {
    pub language: Language,
    pub one: bool,
    pub first: String,
    pub second: String,
    pub enabled: bool,
    pub accepted: bool,
    pub surface: frame::Surface,
    pub graph_style: frame::GraphStyle,
    pub hide_names: bool,
    pub timing: frame::Timing,
}
impl Prefs {
    fn file() -> Result<PathBuf, String> {
        Ok(
            PathBuf::from(env::var_os("LOCALAPPDATA").ok_or("找不到 LOCALAPPDATA")?)
                .join("AzothScope/settings.json"),
        )
    }
    fn load() -> Self {
        Self::file()
            .ok()
            .and_then(|p| fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        let p = Self::file()?;
        fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        let tmp = p.with_extension("tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(tmp, p).map_err(|e| e.to_string())
    }
}
pub struct State {
    pub sources: Vec<Source>,
    pub pref: Prefs,
    pub history: Vec<History>,
    sample_time: Instant,
    pub picture: Frame,
    pub bands: frame::Bands,
    pub legend: Option<frame::Legend>,
    pub battery: Option<u8>,
    pub generation: u64,
    pub status: OutputStatus,
    pub stop: bool,
}
pub type Shared = Arc<(Mutex<State>, Condvar)>;
impl State {
    fn split_line(&self) -> bool {
        self.pref.surface == frame::Surface::NativeBars
            && self.pref.graph_style == frame::GraphStyle::Line
            && !self.pref.one
            && self.pref.first != self.pref.second
    }
    pub fn indices(&self) -> (usize, usize) {
        let find = |id: &str| self.sources.iter().position(|s| s.id == id);
        (
            find(&self.pref.first).unwrap_or(0),
            find(&self.pref.second).unwrap_or(0),
        )
    }
    pub fn draw(&mut self) {
        let window = self.pref.timing.window(self.sample_time);
        let (a, b) = self.indices();
        let mut rows = vec![(self.sources[a].short.as_str(), &self.history[a])];
        if !self.pref.one && a != b {
            rows.push((&self.sources[b].short, &self.history[b]));
        }
        if self.pref.surface == frame::Surface::NativeBars {
            self.bands = frame::history_bands(rows[0].1, rows.get(1).map(|r| r.1), window);
            self.legend = (!self.pref.hide_names).then(|| frame::Legend {
                first: rows[0].0.into(),
                second: rows.get(1).map(|r| r.0.into()),
                battery: self.battery,
            });
            self.picture = if self.split_line() {
                frame::split_lines(rows[0].1, rows[1].1, self.legend.as_ref(), window)
            } else {
                frame::preview_bands(
                    &self.bands,
                    self.pref.graph_style,
                    rows.len() == 2,
                    self.legend.as_ref(),
                )
            };
        } else {
            self.picture = frame::render(&rows, self.pref.surface, window);
            frame::battery_badge(&mut self.picture, self.battery);
        }
        self.generation += 1;
    }
}
pub fn start() -> (Shared, Vec<JoinHandle<()>>) {
    let sources = metrics::sources();
    let mut pref = Prefs::load();
    pref.timing.normalize();
    pref.enabled &= pref.accepted;
    let gpu = sources
        .iter()
        .skip(1)
        .max_by_key(|g| g.vram)
        .unwrap_or(&sources[0]);
    if !sources.iter().any(|s| s.id == pref.first) {
        pref.first = sources[0].id.clone()
    }
    if !sources.iter().any(|s| s.id == pref.second) || pref.first == pref.second {
        pref.second = if gpu.id != pref.first {
            gpu.id.clone()
        } else {
            sources
                .iter()
                .find(|s| s.id != pref.first)
                .unwrap_or(gpu)
                .id
                .clone()
        };
    }
    if sources.len() == 1 {
        pref.one = true;
    }
    let history = vec![History::new(); sources.len()];
    let mut state = State {
        sources,
        pref,
        history,
        sample_time: Instant::now(),
        picture: vec![],
        bands: [0; 26],
        legend: None,
        battery: None,
        generation: 0,
        status: OutputStatus::Waiting,
        stop: false,
    };
    state.draw();
    let shared = Arc::new((Mutex::new(state), Condvar::new()));
    let a = shared.clone();
    let sampling = thread::spawn(move || {
        let sources = a.0.lock().unwrap().sources.clone();
        let mut sampler = metrics::Sampler::new();
        let mut longest_wait = Duration::ZERO;
        let mut last = Instant::now();
        loop {
            let s = a.0.lock().unwrap();
            let tick = s.pref.timing.interval();
            longest_wait = longest_wait.max(tick);
            let (s, _) =
                a.1.wait_timeout_while(s, tick.saturating_sub(last.elapsed()), |s| {
                    !s.stop && s.pref.timing.interval() == tick
                })
                .unwrap();
            if s.stop {
                break;
            }
            if s.pref.timing.interval() != tick {
                continue;
            }
            drop(s);
            let elapsed = last.elapsed();
            last = Instant::now();
            let vals = sampler.sample(&sources);
            let mut s = a.0.lock().unwrap();
            // A missed deadline after sleep is a real time gap. Changing to a
            // shorter interval, however, does not invalidate the previous interval.
            let gap = elapsed >= longest_wait * 2;
            longest_wait = Duration::ZERO;
            s.sample_time = last;
            for (h, v) in s.history.iter_mut().zip(vals) {
                frame::push(h, last, if gap { None } else { v });
            }
            s.draw();
            a.1.notify_all();
        }
    });
    let a = shared.clone();
    let output = thread::spawn(move || {
        let mut display: Option<oled::Display> = None;
        let mut poll_at = Instant::now();
        let mut seen = 0;
        let mut next = Instant::now();
        loop {
            let mut s = a.0.lock().unwrap();
            if s.stop {
                break;
            }
            if !s.pref.enabled {
                display = None;
                seen = 0;
                s.status = OutputStatus::Paused;
                if Instant::now() < poll_at {
                    let _s =
                        a.1.wait_timeout_while(
                            s,
                            poll_at.saturating_duration_since(Instant::now()),
                            |s| !s.stop && !s.pref.enabled,
                        )
                        .unwrap();
                    continue;
                }
            }
            if s.pref.enabled && display.is_none() {
                if next > Instant::now() {
                    let _s =
                        a.1.wait_timeout(s, next.saturating_duration_since(Instant::now()))
                            .unwrap();
                    continue;
                }
                drop(s);
                match oled::Display::open() {
                    Ok(dev) => {
                        display = Some(dev);
                        poll_at = Instant::now();
                    }
                    Err(e) => {
                        let mut s = a.0.lock().unwrap();
                        if s.battery.take().is_some() {
                            s.draw();
                        }
                        s.status = OutputStatus::Retry(e);
                        next = Instant::now() + Duration::from_secs(5);
                    }
                }
                continue;
            }
            if Instant::now() >= poll_at && (!s.pref.enabled || display.is_some()) {
                drop(s);
                // Serialize queries with image transfers. In preview-only mode,
                // release the control handle as soon as the battery read finishes.
                let value = match &display {
                    Some(d) => d.battery(),
                    None => oled::Display::open().ok().and_then(|d| d.battery()),
                };
                poll_at = Instant::now() + Duration::from_secs(30);
                let mut s = a.0.lock().unwrap();
                if s.battery != value {
                    s.battery = value;
                    s.draw();
                }
                continue;
            }
            let split = s.split_line();
            let native = s.pref.surface == frame::Surface::NativeBars && !split;
            if (!native && s.generation == seen) || next > Instant::now() {
                let wait = if !native && s.generation == seen {
                    s.pref.timing.interval()
                } else {
                    next.saturating_duration_since(Instant::now())
                };
                let _s = a.1.wait_timeout(s, wait).unwrap();
                continue;
            }
            let f = (!native).then(|| s.picture.clone());
            let bands = s.bands;
            let surface = s.pref.surface;
            let style = s.pref.graph_style;
            let legend = s.legend.clone();
            seen = s.generation;
            drop(s);
            let result = (|| {
                let dev = display.as_mut().unwrap();
                if let Some(f) = f {
                    if split {
                        dev.show_split(&f)?;
                    } else {
                        dev.show(&f, surface)?;
                    }
                } else {
                    dev.show_bands(&bands, style, legend.as_ref())?;
                }
                Ok::<_, String>(dev.link)
            })();
            let mut s = a.0.lock().unwrap();
            match result {
                Ok(link) => {
                    s.status = if native {
                        OutputStatus::Stream(
                            link,
                            match style {
                                frame::GraphStyle::Bar => "Bar",
                                frame::GraphStyle::Line | frame::GraphStyle::JoinedLine => "Line",
                            },
                        )
                    } else if split {
                        OutputStatus::Split(link)
                    } else {
                        OutputStatus::Frame(link)
                    };
                    next = Instant::now()
                        + Duration::from_millis(if native || split { 100 } else { 1250 });
                }
                Err(e) => {
                    display = None;
                    if s.battery.take().is_some() {
                        s.draw();
                    }
                    s.status = OutputStatus::Retry(e);
                    next = Instant::now() + Duration::from_secs(5);
                }
            }
        }
    });
    (shared, vec![sampling, output])
}

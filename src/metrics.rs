use std::{collections::HashMap, mem, ptr};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::{Performance::*, Threading::GetSystemTimes},
};

#[derive(Clone, Debug)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub short: String,
    pub vram: usize,
    luid: String,
}

pub fn sources() -> Vec<Source> {
    let mut out = vec![Source {
        id: "cpu".into(),
        name: "CPU · 总占用率".into(),
        short: "CPU".into(),
        vram: 0,
        luid: String::new(),
    }];
    unsafe {
        if let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() {
            let mut occurrences = HashMap::<String, usize>::new();
            for n in 0..64 {
                let Ok(adapter) = factory.EnumAdapters1(n) else {
                    break;
                };
                let Ok(d) = adapter.GetDesc1() else { continue };
                if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                    continue;
                }
                let name = String::from_utf16_lossy(
                    &d.Description[..d
                        .Description
                        .iter()
                        .position(|x| *x == 0)
                        .unwrap_or(d.Description.len())],
                );
                let pci = format!("{:04x}:{:04x}:{:08x}", d.VendorId, d.DeviceId, d.SubSysId);
                let ordinal = occurrences.entry(pci.clone()).or_default();
                let index = out.len() - 1;
                out.push(Source {
                    id: format!("{pci}:{ordinal}"),
                    name: format!("GPU {index} · {name}"),
                    short: format!("GPU{index}"),
                    vram: d.DedicatedVideoMemory,
                    luid: format!(
                        "0x{:08x}_0x{:08x}",
                        d.AdapterLuid.HighPart as u32, d.AdapterLuid.LowPart
                    ),
                });
                *ordinal += 1;
            }
        }
    }
    out
}

fn times() -> Option<(u64, u64)> {
    unsafe {
        let (mut i, mut k, mut u): (FILETIME, FILETIME, FILETIME) = mem::zeroed();
        if GetSystemTimes(&mut i, &mut k, &mut u) == 0 {
            return None;
        }
        let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        Some((ticks(i), ticks(k) + ticks(u)))
    }
}

pub struct Sampler {
    prev: Option<(u64, u64)>,
    query: isize,
    engine: isize,
}
impl Sampler {
    pub fn new() -> Self {
        unsafe {
            let mut s = Self {
                prev: times(),
                query: 0,
                engine: 0,
            };
            if PdhOpenQueryW(ptr::null(), 0, &mut s.query) == 0 {
                let path: Vec<_> = r"\GPU Engine(*)\Utilization Percentage"
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                if PdhAddEnglishCounterW(s.query, path.as_ptr(), 0, &mut s.engine) == 0 {
                    PdhCollectQueryData(s.query);
                }
            }
            s
        }
    }
    pub fn sample(&mut self, sources: &[Source]) -> Vec<Option<u8>> {
        let now = times();
        let cpu = self.prev.zip(now).and_then(|((i, t), (ni, nt))| {
            let total = nt.checked_sub(t)?;
            let idle = ni.checked_sub(i)?;
            (total > 0)
                .then(|| ((total.saturating_sub(idle) as f64 / total as f64) * 100.).round() as u8)
        });
        self.prev = now;
        let mut gpu = HashMap::new();
        if self.engine != 0 {
            unsafe {
                if PdhCollectQueryData(self.query) == 0 {
                    gpu = engine_usage(self.engine);
                }
            }
        }
        sources
            .iter()
            .map(|s| {
                if s.id == "cpu" {
                    cpu
                } else {
                    gpu.get(&s.luid).copied()
                }
            })
            .collect()
    }
}
impl Drop for Sampler {
    fn drop(&mut self) {
        if self.query != 0 {
            unsafe {
                PdhCloseQuery(self.query);
            }
        }
    }
}

// Windows exposes per-process engine times. Sum processes on the SAME engine,
// then take the busiest engine on each adapter (not a sum of all engines).
fn aggregate(rows: impl Iterator<Item = (String, f64)>) -> HashMap<String, u8> {
    let mut engines = HashMap::new();
    for (name, value) in rows {
        let Some((_, rest)) = name.split_once("_luid_") else {
            continue;
        };
        let Some((id, rest)) = rest.split_once("_phys_") else {
            continue;
        };
        let Some((engine, _)) = rest.split_once("_engtype_") else {
            continue;
        };
        if !value.is_finite() || value < 0. {
            continue;
        }
        *engines
            .entry((id.to_ascii_lowercase(), engine.to_owned()))
            .or_insert(0.) += value;
    }
    let mut out = HashMap::new();
    for ((id, _), sum) in engines {
        let v = sum.round().min(100.) as u8;
        let x = out.entry(id).or_insert(0u8);
        *x = (*x).max(v);
    }
    out
}

unsafe fn engine_usage(handle: isize) -> HashMap<String, u8> {
    let (mut bytes, mut count) = (0, 0);
    if PdhGetFormattedCounterArrayW(
        handle,
        PDH_FMT_DOUBLE,
        &mut bytes,
        &mut count,
        ptr::null_mut(),
    ) != PDH_MORE_DATA
        || bytes == 0
        || bytes > 32 * 1024 * 1024
    {
        return HashMap::new();
    }
    let mut buf = vec![0usize; (bytes as usize).div_ceil(mem::size_of::<usize>())];
    let p = buf.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    if PdhGetFormattedCounterArrayW(handle, PDH_FMT_DOUBLE, &mut bytes, &mut count, p) != 0 {
        return HashMap::new();
    }
    aggregate(
        std::slice::from_raw_parts(p, count as usize)
            .iter()
            .filter_map(|r| {
                if r.FmtValue.CStatus > 1 || r.szName.is_null() {
                    return None;
                }
                let name = windows::core::PCWSTR(r.szName).to_string().ok()?;
                Some((name, r.FmtValue.Anonymous.doubleValue))
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_engines_and_adapters() {
        let rows = [
            (1, "a", 0, 25.),
            (2, "a", 0, 30.),
            (2, "a", 1, 45.),
            (1, "b", 0, 80.),
        ];
        let g =
            aggregate(rows.into_iter().map(|(pid, id, e, x)| {
                (format!("pid_{pid}_luid_{id}_phys_0_eng_{e}_engtype_3d"), x)
            }));
        assert_eq!((g["a"], g["b"]), (55, 80));
    }
}

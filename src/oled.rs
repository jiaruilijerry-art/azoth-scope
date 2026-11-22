use crate::frame::{Surface, W};
use hidapi::{DeviceInfo, HidApi, HidDevice};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

const ASUS: u16 = 0x0b05;
const CABLE: u16 = 0x1a83;
const RADIO: u16 = 0x1a85;
const OMNI: u16 = 0x1ace;
type Res<T> = Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn report_ids(dev: &HidDevice) -> Res<BTreeSet<u8>> {
    let mut buf = [0; 4096];
    let n = dev.get_report_descriptor(&mut buf).map_err(err)?;
    let mut i = 0;
    let mut ids = BTreeSet::new();
    while i < n {
        let b = buf[i];
        i += 1;
        if b == 0xfe {
            if i + 2 > n {
                return Err("HID 描述符不完整".into());
            }
            i += 2 + buf[i] as usize;
        } else {
            let len = [0, 1, 2, 4][(b & 3) as usize];
            if i + len > n {
                return Err("HID 描述符不完整".into());
            }
            if b == 0x85 {
                ids.insert(buf[i]);
            }
            i += len;
        }
    }
    if ids.is_empty() {
        ids.insert(0);
    }
    Ok(ids)
}
pub struct Display {
    dev: HidDevice,
    id: u8,
    pub link: &'static str,
    last: Option<Instant>,
    previous: Vec<u8>,
    streaming: Option<(crate::frame::GraphStyle, Option<crate::frame::Legend>)>,
    split_active: bool,
}
impl Display {
    pub fn open() -> Res<Self> {
        let api = HidApi::new().map_err(err)?;
        let find = |pid, page| -> Vec<&DeviceInfo> {
            api.device_list()
                .filter(|d| {
                    d.vendor_id() == ASUS && d.product_id() == pid && d.usage_page() == page
                })
                .collect()
        };
        for (pid, link) in [(CABLE, "USB"), (RADIO, "2.4 GHz")] {
            let list = find(pid, 0xff00);
            if list.len() > 1 {
                return Err("发现多个 M701 控制端点，请只连接一把键盘".into());
            }
            if let Some(info) = list.first() {
                let dev = info.open_device(&api).map_err(err)?;
                let ids = report_ids(&dev)?;
                if ids.len() != 1 {
                    return Err("M701 控制报告不唯一".into());
                }
                return Ok(Self {
                    dev,
                    id: *ids.first().unwrap(),
                    link,
                    last: None,
                    previous: vec![],
                    streaming: None,
                    split_active: false,
                });
            }
        }
        let hubs = find(OMNI, 0xff02);
        if hubs.len() > 1 {
            return Err("请只连接一个 OMNI 接收器".into());
        }
        if let Some(h) = hubs.first() {
            let hub = h.open_device(&api).map_err(err)?;
            let mut q = [0; 64];
            q[..3].copy_from_slice(&[1, 0xa0, 0]);
            if hub.write(&q).map_err(err)? != q.len() {
                return Err("接收器查询不完整".into());
            }
            let mut b = [0; 65];
            let end = Instant::now() + Duration::from_secs(2);
            while Instant::now() < end {
                let n = hub.read_timeout(&mut b, 100).map_err(err)?;
                if n < 13 || b[..3] != [1, 0xa0, 0] {
                    continue;
                }
                for slot in b[5..13].as_chunks::<4>().0 {
                    if u16::from_le_bytes([slot[0], slot[1]]) != RADIO || slot[2] == 0 {
                        continue;
                    }
                    for info in find(OMNI, 0xff00) {
                        if info.serial_number() != h.serial_number() {
                            continue;
                        }
                        let dev = info.open_device(&api).map_err(err)?;
                        if report_ids(&dev)?.contains(&slot[2]) {
                            return Ok(Self {
                                dev,
                                id: slot[2],
                                link: "2.4 GHz OMNI",
                                last: None,
                                previous: vec![],
                                streaming: None,
                                split_active: false,
                            });
                        }
                    }
                }
                break;
            }
        }
        Err("找不到 M701 OLED 控制通道，请检查 USB / 2.4 GHz 连接".into())
    }
    fn packet(&self, cmd: &[u8]) -> Res<()> {
        let mut p = [0; 65];
        p[0] = self.id;
        p[1..=cmd.len()].copy_from_slice(cmd);
        let size = if self.id == 0 { 65 } else { 64 };
        if self.dev.write(&p[..size]).map_err(err)? != size {
            return Err("HID 报告写入不完整".into());
        }
        // On our USB M701 the 67:00 mode command is acknowledged, but no
        // per-column-frame 67:02 echo appears on the control collection.
        // Listen briefly for a rejection without restarting the mode on silence.
        let streamed = cmd.starts_with(&[0x67, 2]);
        let end = Instant::now() + Duration::from_millis(if streamed { 10 } else { 1200 });
        let mut buf = [0; 65];
        while Instant::now() < end {
            let n = self
                .dev
                .read_timeout(&mut buf, if streamed { 10 } else { 80 })
                .map_err(err)?;
            let mut reply = &buf[..n];
            if self.id != 0 {
                if reply.first() != Some(&self.id) {
                    continue;
                }
                reply = &reply[1..];
            }
            if reply.starts_with(&[0xff, 0xaa]) {
                return Err(format!("固件拒绝 OLED 指令 {:02X}:{:02X}", cmd[0], cmd[1]));
            }
            let k = if matches!(&cmd[..2], [0x61, 2] | [0x62, 1]) {
                4
            } else {
                2
            };
            if reply.starts_with(&cmd[..k]) {
                return Ok(());
            }
        }
        if streamed {
            Ok(())
        } else {
            Err(format!(
                "键盘未确认 OLED 指令 {:02X}:{:02X}",
                cmd[0], cmd[1]
            ))
        }
    }
    pub fn show(&mut self, f: &[u8], surface: Surface) -> Res<()> {
        if self.streaming.take().is_some() || self.split_active {
            self.split_active = false;
            self.previous.clear();
        }
        if f.len() != W * surface.height() {
            return Err("画面尺寸错误".into());
        }
        if self.previous == f {
            return Ok(());
        }
        if self
            .last
            .is_some_and(|t| t.elapsed() < Duration::from_millis(1250))
        {
            return Ok(());
        }
        let step = if self.id == 0 { 60 } else { 59 };
        upload(f, surface, step, |p| self.packet(p))?;
        self.last = Some(Instant::now());
        self.previous = f.to_owned();
        Ok(())
    }
    pub fn show_bands(
        &mut self,
        bands: &crate::frame::Bands,
        style: crate::frame::GraphStyle,
        legend: Option<&crate::frame::Legend>,
    ) -> Res<()> {
        self.split_active = false;
        if !self
            .streaming
            .as_ref()
            .is_some_and(|(s, l)| *s == style && l.as_ref() == legend)
        {
            let mut header = style.mode_header();
            if let Some(l) = legend {
                let bytes = pack(&l.pixels(), Surface::Banner);
                let step = if self.id == 0 { 60 } else { 59 };
                let count = bytes.len().div_ceil(step);
                header[4] = 2; // native spectrum + media info bitmap
                header[6..8].copy_from_slice(&(count as u16).to_le_bytes());
                header[10..12].copy_from_slice(&(crate::frame::TITLE_H as u16).to_le_bytes());
                self.packet(&header)?;
                self.info_pixels(&bytes, step)?;
            } else {
                self.packet(&header)?;
            }
            self.streaming = Some((style, legend.cloned()));
        }
        let mut data = [0u8; 30];
        data[..4].copy_from_slice(&[0x67, 2, 0, 0]);
        data[4..].copy_from_slice(bands);
        self.packet(&data)
    }

    fn info_pixels(&self, bytes: &[u8], step: usize) -> Res<()> {
        let count = bytes.len().div_ceil(step);
        for (i, data) in bytes.chunks(step).enumerate() {
            let mut p = vec![0x67, 1];
            p.extend(((count - 1 - i) as u16).to_le_bytes());
            p.extend(data);
            self.packet(&p)?;
        }
        Ok(())
    }

    pub fn show_split(&mut self, pixels: &[u8]) -> Res<()> {
        use crate::frame::{GraphStyle, H};
        if pixels.len() != W * H {
            return Err("双折线尺寸错误".into());
        }
        if self.split_active && self.previous == pixels {
            return Ok(());
        }
        let bytes = pack(pixels, Surface::Banner);
        let step = if self.id == 0 { 60 } else { 59 };
        {
            // M701 requires a fresh media-info header before each changed bitmap.
            let mut header = GraphStyle::Line.mode_header();
            header[4] = 0; // media information only
            header[6..8].copy_from_slice(&(bytes.len().div_ceil(step) as u16).to_le_bytes());
            self.packet(&header)?;
        }
        self.info_pixels(&bytes, step)?;
        self.split_active = true;
        self.streaming = None;
        self.previous = pixels.to_vec();
        Ok(())
    }
}
fn upload(
    f: &[u8],
    surface: Surface,
    step: usize,
    mut send: impl FnMut(&[u8]) -> Res<()>,
) -> Res<()> {
    let bytes = pack(f, surface);
    let count = bytes.len().div_ceil(step);
    let (cmd, data_op) = match surface {
        Surface::NativeBars => return Err("原生柱图不接受位图".into()),
        Surface::Image => {
            let mut start = vec![0x61, 1, 0, 0];
            start.extend((count as u32).to_le_bytes());
            send(&start)?;
            (0x61, 2)
        }
        Surface::Banner => {
            let mut start = vec![0x62, 0, 0, 0, 0, 0];
            start.extend((W as u16).to_le_bytes());
            start.extend((surface.height() as u16).to_le_bytes());
            start.extend((count as u16).to_le_bytes());
            send(&start)?;
            (0x62, 1)
        }
    };
    for (i, data) in bytes.chunks(step).enumerate() {
        let mut packet = vec![cmd, data_op];
        packet.extend(((count - 1 - i) as u16).to_le_bytes());
        packet.extend(data);
        send(&packet)?;
    }
    send(&[cmd, data_op + 1, 0, 0])
}
fn pack(f: &[u8], surface: Surface) -> Vec<u8> {
    let mut b = Vec::with_capacity(f.len() / 2 + 4);
    if surface == Surface::Image {
        b.extend([1, 0, 0xe8, 3]);
    }
    b.extend(
        f.as_chunks::<2>()
            .0
            .iter()
            .map(|p| p[0] >> 4 | (p[1] & 0xf0)),
    );
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_frame_format() {
        let mut f = vec![0; W * Surface::Image.height()];
        f[0] = 255;
        f[3] = 255;
        let b = pack(&f, Surface::Image);
        assert_eq!(b.len(), 6660);
        assert_eq!(b[..6], [1, 0, 232, 3, 15, 240]);
    }
    #[test]
    fn banner_uses_only_text_reports_and_preserves_both_rows_on_both_transports() {
        use crate::frame::{render, History};
        let end = Instant::now();
        let f = render(
            &[
                ("CPU", &History::from([(end, Some(100))])),
                ("G1", &History::from([(end, Some(0))])),
            ],
            Surface::Banner,
            crate::frame::Timing::default().window(end),
        );
        assert_eq!(f.len(), 208 * 46);
        assert_eq!((f[2 * 208 + 207], f[43 * 208 + 207]), (255, 255));
        for (step, count) in [(60, 80), (59, 82)] {
            let mut packets = vec![];
            upload(&f, Surface::Banner, step, |p| {
                packets.push(p.to_vec());
                Ok(())
            })
            .unwrap();
            assert_eq!(packets[0], [0x62, 0, 0, 0, 0, 0, 208, 0, 46, 0, count, 0]);
            assert_eq!(packets.len(), count as usize + 2);
            let mut pixels = vec![];
            for (i, p) in packets[1..packets.len() - 1].iter().enumerate() {
                assert_eq!(p[..4], [0x62, 1, count - 1 - i as u8, 0]);
                pixels.extend_from_slice(&p[4..]);
            }
            assert_eq!(pixels.len(), 4784);
            assert!(pixels
                .iter()
                .zip(f.as_chunks::<2>().0)
                .all(|(&b, p)| b & 15 == p[0] >> 4 && b >> 4 == p[1] >> 4));
            assert_eq!(packets.last().unwrap().as_slice(), [0x62, 2, 0, 0]);
        }
    }
}

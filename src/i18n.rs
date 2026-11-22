use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "en")]
    English,
}

impl Language {
    pub fn pick<'a>(self, chinese: &'a str, english: &'a str) -> &'a str {
        match self {
            Self::Chinese => chinese,
            Self::English => english,
        }
    }

    pub fn detail(self, error: &str) -> String {
        if self == Self::Chinese {
            return error.into();
        }
        let translated = match error {
            "找不到 LOCALAPPDATA" => "LOCALAPPDATA is unavailable",
            "HID 描述符不完整" => "Incomplete HID descriptor",
            "发现多个 M701 控制端点，请只连接一把键盘" => {
                "Multiple M701 interfaces found. Connect only one keyboard"
            }
            "M701 控制报告不唯一" => "Ambiguous M701 control report",
            "请只连接一个 OMNI 接收器" => "Connect only one OMNI receiver",
            "接收器查询不完整" => "Incomplete receiver query",
            "找不到 M701 OLED 控制通道，请检查 USB / 2.4 GHz 连接" => {
                "M701 OLED not found. Check the USB or 2.4 GHz connection"
            }
            "HID 报告写入不完整" => "Incomplete HID write",
            "画面尺寸错误" => "Invalid frame size",
            "双折线尺寸错误" => "Invalid split-line frame size",
            "原生柱图不接受位图" => "The native spectrum channel cannot upload a bitmap",
            other => {
                for (prefix, en) in [
                    ("固件拒绝 OLED 指令 ", "Firmware rejected OLED command"),
                    (
                        "键盘未确认 OLED 指令 ",
                        "No acknowledgement for OLED command",
                    ),
                ] {
                    if let Some(code) = other.strip_prefix(prefix) {
                        return format!("{en} {code}");
                    }
                }
                other
            }
        };
        translated.into()
    }
}

pub enum OutputStatus {
    Waiting,
    Paused,
    Stream(&'static str, &'static str),
    Split(&'static str),
    Frame(&'static str),
    Retry(String),
}
impl OutputStatus {
    pub fn text(&self, l: Language) -> String {
        match self {
            Self::Waiting => l.pick("等待采样", "Waiting for samples").into(),
            Self::Paused => l
                .pick("仅预览 · 已停止写屏", "Preview only · OLED output paused")
                .into(),
            Self::Stream(link, style) => format!(
                "{link} · {style} {}",
                l.pick(
                    "样式已确认，趋势数据已发送",
                    "mode confirmed; trend data sent"
                )
            ),
            Self::Split(link) => format!(
                "{link} · {}",
                l.pick("Line 双曲线数据已确认", "Split-line frame acknowledged")
            ),
            Self::Frame(link) => format!(
                "{link} · {}",
                l.pick("键盘已确认画面", "Frame acknowledged")
            ),
            Self::Retry(e) => format!(
                "{} · {}",
                l.detail(e),
                l.pick("5 秒后重试", "Retrying in 5 s")
            ),
        }
    }
}

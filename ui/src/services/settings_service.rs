//! Per-machine/per-terminal app settings — a plain `key=value` text
//! file, not a server-side concept (see `design/settings-panel.slint`
//! header comment). Covers the connection target, this computer's
//! own display name, the doctor dashboard's microphone input gain,
//! and whether signing out should ask for confirmation first.

use std::fs;
use std::io;
use std::path::PathBuf;

const DEFAULT_SERVER_ADDRESS: &str = "localhost:9086";
const DEFAULT_COMPUTER_NAME: &str = "PC-1";
const DEFAULT_MIC_GAIN: f32 = 1.0;
const DEFAULT_CONFIRM_BEFORE_SIGN_OUT: bool = false;

/// One terminal's saved preferences. Cheap to clone so both the
/// dashboard controller and its settings-panel handler can each hold
/// their own copy without fighting over ownership.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub server_address: String,
    pub computer_name: String,
    pub mic_gain: f32,
    pub confirm_before_sign_out: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server_address: DEFAULT_SERVER_ADDRESS.to_string(),
            computer_name: DEFAULT_COMPUTER_NAME.to_string(),
            mic_gain: DEFAULT_MIC_GAIN,
            confirm_before_sign_out: DEFAULT_CONFIRM_BEFORE_SIGN_OUT,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let Ok(contents) = fs::read_to_string(Self::path()) else {
            return Self::default();
        };

        let mut settings = Self::default();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "server_address" if !value.is_empty() => settings.server_address = value.to_string(),
                "computer_name" if !value.is_empty() => settings.computer_name = value.to_string(),
                "mic_gain" => {
                    if let Ok(v) = value.parse::<f32>() {
                        settings.mic_gain = v;
                    }
                }
                "confirm_before_sign_out" => {
                    if let Ok(v) = value.parse::<bool>() {
                        settings.confirm_before_sign_out = v;
                    }
                }
                _ => {}
            }
        }
        settings
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = format!(
            "server_address={}\ncomputer_name={}\nmic_gain={}\nconfirm_before_sign_out={}\n",
            self.server_address, self.computer_name, self.mic_gain, self.confirm_before_sign_out,
        );
        fs::write(path, contents)
    }

    fn path() -> PathBuf {
        let base = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join("Thyme").join("settings.txt")
    }
}
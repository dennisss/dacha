use common::errors::*;


#[derive(Args)]
pub enum HardwareModel {
    #[arg(name = "pi4")]
    Pi4,

    #[arg(name = "pi5")]
    Pi5,

    #[arg(name = "cm4")]
    Cm4,

    #[arg(name = "cm5-regular")]
    Cm5Regular,

    #[arg(name = "cm5-lite")]
    Cm5Lite
}

impl HardwareModel {
    pub fn config_txt_filter(&self) -> &'static str {
        match self {
            Self::Pi4 => "pi4",
            Self::Pi5 => "pi5",
            Self::Cm4 => "cm4",
            Self::Cm5Regular | Self::Cm5Lite => "cm5",
        }
    }

    pub fn device_tree(&self) -> &'static str {
        match self {
            Self::Pi4 => "bcm2711-rpi-4-b.dtb",
            Self::Pi5 => "bcm2712-d-rpi-5-b.dtb",
            Self::Cm4 => "bcm2711-rpi-cm4.dtb",
            Self::Cm5Regular => "bcm2712-rpi-cm5-cm5io.dtb",
            Self::Cm5Lite => "bcm2712-rpi-cm5l-cm5io.dtb",
        }
    }

    pub fn kernel(&self) -> &'static str {
        match self {
            Self::Pi4 | Self::Cm4 => "kernel8.img",
            Self::Pi5 | Self::Cm5Regular | Self::Cm5Lite => "kernel_2712.img",
        }
    }
}


pub struct ConfigTxtFile {
    lines: Vec<ConfigTxtLine>
}

enum ConfigTxtLine {
    Filter(String),
    KeyValue(String, String)
}

impl ConfigTxtFile {
    pub fn parse(data: &str) -> Result<Self> {
        let mut lines = vec![];

        for mut line in data.lines() {
            line = line.trim();
            if line.starts_with("#") || line.is_empty() {
                continue;
            }

            if let Some(rest) = line.strip_prefix("[") {
                if let Some(rest) = rest.strip_suffix("]") {
                    lines.push(ConfigTxtLine::Filter(rest.to_string()));
                    continue;
                }
            }

            let (key, value) = line.split_once("=")
                .ok_or_else(|| format_err!("Invalid config.txt key/value line: {}", line))?;

            lines.push(ConfigTxtLine::KeyValue(key.to_string(), value.to_string()));
        }

        Ok(Self {
            lines
        })
    }

    pub fn extend(&mut self, other: ConfigTxtFile) {
        self.lines.extend(other.lines);
    }

    // TODO: Support removing the disable-wifi options if wireless is being setup.

    // TODO: Implement deduping (though some things like dtoverlays can't be deduped).

    pub fn filter_to_hardware(&mut self, model: &HardwareModel) {
        let mut current_section = "all".to_string();
    
        self.lines.retain(|line| {
            if let ConfigTxtLine::Filter(filter) = line {
                current_section = filter.clone();
                return false;
            } 

            current_section == "all" || current_section == model.config_txt_filter()
        });

        self.lines.insert(0, ConfigTxtLine::KeyValue("kernel".to_string(), model.kernel().to_string()));
        self.lines.insert(1, ConfigTxtLine::KeyValue("device_tree".to_string(), model.device_tree().to_string()));
    }

    pub fn to_string(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                ConfigTxtLine::Filter(v) => out.push_str(&format!("[{}]\n", v)),
                ConfigTxtLine::KeyValue(k, v) => out.push_str(&format!("{}={}\n", k, v))
            }
        }

        out
    }
}
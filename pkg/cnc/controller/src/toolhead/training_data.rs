use std::fmt::Debug;

use common::errors::*;
use file::LocalPath;
use math_compute::io::CSVDataReader;


#[derive(Clone, Default)]
pub struct ToolheadTrainingData {
    pub rows: Vec<ToolheadTrainingDataRow>
}

impl ToolheadTrainingData {
    pub async fn read_csv(path: &LocalPath) -> Result<Self> {
        let mut reader = CSVDataReader::create(path).await?;
        let mut out = Self::default();
        while let Some(row) = reader.read().await? {

            out.rows.push(ToolheadTrainingDataRow {
                time: row.f32_field("time")?,
                heater: row.f32_field("heater")?,
                heater_temp: row.optional_f32_field("heater_temp")?,
                fan: row.f32_field("fan")?,
                nozzle_temp: row.optional_f32_field("nozzle_temp")?,
                heater_current: row.f32_field("heater_current")?,
                heater_voltage: row.f32_field("heater_voltage")?,
                psu_current: row.f32_field("psu_current")?,
                psu_voltage: row.f32_field("psu_voltage")?
            });

        }

        Ok(out)
    }

    pub fn csv_to(&self, buf: &mut String) {
        buf.clear();
        buf.push_str(ToolheadTrainingDataRow::csv_header());

        for row in &self.rows {
            buf.push_str(&row.to_csv_row());
        }
    }

}

#[derive(Clone)]
pub struct ToolheadTrainingDataRow {
    pub time: f32,
    pub heater: f32,
    pub heater_temp: Option<f32>,
    pub fan: f32,
    pub nozzle_temp: Option<f32>,
    pub heater_current: f32,
    pub heater_voltage: f32,
    pub psu_current: f32,
    pub psu_voltage: f32
}

impl ToolheadTrainingDataRow {
    pub fn csv_header() -> &'static str {
        "time,heater,heater_temp,fan,nozzle_temp,heater_current,heater_voltage,psu_current,psu_voltage\n"
    }

    pub fn to_csv_row(&self) -> String {
        format!(
            "{:.2},{:.2},{},{:.2},{},{:?},{:?},{:?},{:?}\n",
            self.time,
            self.heater,
            match self.heater_temp {
                Some(v) => format!("{:.2}", v),
                None => String::new()
            },
            self.fan,
            match self.nozzle_temp {
                Some(v) => format!("{:.2}", v),
                None => String::new()
            },
            self.heater_current,
            self.heater_voltage,
            self.psu_current,
            self.psu_voltage
        )
    }
}

impl Debug for ToolheadTrainingDataRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,
            "[Time: {:.2}] [H: {:.2}] [HT: {:.2}] [Fan: {:.2}] [N: {:.2?}] [Pow: {:.2}V / {:.2}A] [PSU: {:.2}V / {:.2}A]",
            self.time,
            self.heater,
            self.heater_temp.unwrap_or(-1.0),
            self.fan,
            self.nozzle_temp.unwrap_or(-1.0),
            self.heater_voltage,
            self.heater_current,
            self.psu_voltage,
            self.psu_current
        )
    }
}

#[derive(Default, Clone)]
pub struct BedTrainingData {
    pub rows: Vec<BedTrainingDataRow>,
}

impl BedTrainingData {

    

    pub fn csv_to(&self, buf: &mut String) {
        buf.clear();
        buf.push_str(BedTrainingDataRow::csv_header());

        for row in &self.rows {
            buf.push_str(&row.to_csv_row());
        }
    }

}

// NOTE: At each 'time', the heater/fan values start to be switched to that input value at that time.
#[derive(Clone)]
pub struct BedTrainingDataRow {
    pub time: f32,
    pub heater: f32,
    pub fan: f32,
    pub bed: Option<f32>,
    pub sheet: Option<f32>
}

impl BedTrainingDataRow {
    pub fn csv_header() -> &'static str {
        "time,heater,fan,bed,sheet\n"
    }

    pub fn to_csv_row(&self) -> String {
        format!(
            "{:.2},{:.2},{:.2},{},{}\n",
            self.time,
            self.heater,
            self.fan,
            match self.bed {
                Some(v) => format!("{:.2}", v),
                None => String::new()
            },
            match self.sheet {
                Some(v) => format!("{:.2}", v),
                None => String::new()
            }
        )

    }

}

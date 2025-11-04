use crate::thermistor::*;

const TEMP_TO_KRESISTANCE: &'static [(f32, f32)] = &[
    (-50.0, 8887.0),
    (-30.0, 2156.0),
    (-10.0, 623.2),
    (0.0, 354.6),
    (10.0, 208.8),
    (25.0, 100.0),
    (40.0, 50.9),
    (50.0, 33.45),
    (60.0, 22.48),
    (80.0, 10.8),
    (85.0, 9.094),
    (100.0, 5.569),
    (120.0, 3.058),
    (140.0, 1.77),
    (160.0, 1.074),
    (180.0, 0.6793),
    (200.0, 0.4452),
    (220.0, 0.3016),
    (240.0, 0.2104),
    (260.0, 0.1507),
    (280.0, 0.1105),
    (300.0, 0.08278)
];

/// 104NT-4-R025H42G
/// https://www.semitec-global.com/uploads/2022/01/P18-NT-Thermistor.pdf
#[derive(Default)]
pub struct Thermistor104NT {}

impl Thermistor for Thermistor104NT {
    fn temperature_to_resistance(&self, t: f32) -> Option<f32> {
        let i = match common::algorithms::upper_bound_by(TEMP_TO_KRESISTANCE, (), |el, _| el.0 <= t) {
            Some(v) => v,
            None => return None
        };

        if i == TEMP_TO_KRESISTANCE.len() - 1 {
            return None;
        }

        let (t0, r0) = TEMP_TO_KRESISTANCE[i];
        let (t1, r1) = TEMP_TO_KRESISTANCE[i + 1];

        let x = (t - t1) / (t0 - t1);
        let r = r1 + x * (r0 - r1);
        Some(r * 1000.0)
    }

    fn resistance_to_temperature(&self, mut r: f32) -> Option<f32> {
        r /= 1000.0;

        let i = match common::algorithms::upper_bound_by(TEMP_TO_KRESISTANCE, (), |el, _| el.1 >= r) {
            Some(v) => v,
            None => return None
        };

        if i == TEMP_TO_KRESISTANCE.len() - 1 {
            return None;
        }

        let (t0, r0) = TEMP_TO_KRESISTANCE[i];
        let (t1, r1) = TEMP_TO_KRESISTANCE[i + 1];

        let x = (r - r1) / (r0 - r1);
        let t = t1 + x * (t0 - t1);
        Some(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntc_test() {
        assert_eq!(Thermistor104NT::resistance_to_temperature(9000.0 * 1000.0), None);
        assert_eq!(Thermistor104NT::resistance_to_temperature(100_000.0), Some(25.0));
        assert_eq!(Thermistor104NT::resistance_to_temperature(0.3016 * 1000.0), Some(220.0));
        assert_eq!(Thermistor104NT::resistance_to_temperature(27.0 * 1000.0), Some(55.879673));
        assert_eq!(Thermistor104NT::resistance_to_temperature(96.64001), Some(290.0));
        assert_eq!(Thermistor104NT::resistance_to_temperature(0.01), None);

        assert_eq!(Thermistor104NT::temperature_to_resistance(-60.0), None);
        assert_eq!(Thermistor104NT::temperature_to_resistance(25.0), Some(100_000.0));
        assert_eq!(Thermistor104NT::temperature_to_resistance(220.0), Some(0.3016 * 1000.0));
        assert_eq!(Thermistor104NT::temperature_to_resistance(55.879673), Some(26999.998));
        assert_eq!(Thermistor104NT::temperature_to_resistance(290.0), Some(96.64001));
        assert_eq!(Thermistor104NT::temperature_to_resistance(301.0), None);
    }
}
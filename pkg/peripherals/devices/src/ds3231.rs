use common::errors::*;
use peripherals::i2c::I2CDevice;

/*
Speed 100kHz to 400kHz I2c

TODO: Explicitly configure the speed.
*/

// 7-bit
const DEVICE_ADDRESS: u8 = 0b1101000;

const CURRENT_TIME_OFFSET: u8 = 0;
const CURRENT_TIME_SIZE: usize = 7;

const TEMPERATURE_OFFSET: u8 = 0x11;

const DAYS_PER_MONTH: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

pub struct DS3231 {
    i2c: I2CDevice,
}

impl DS3231 {
    pub fn open(i2c: I2CDevice) -> Self {
        Self { i2c }
    }

    // TODO: First clear status bit and then start the oscillator?

    /// NOTE: Until the write_time() function is called at least once for this
    /// device, the time returned by this function is undefined.
    pub fn read_time(&mut self) -> Result<DS3231Time> {
        self.i2c.write(DEVICE_ADDRESS, &[CURRENT_TIME_OFFSET])?;

        let mut data = [0u8; CURRENT_TIME_SIZE];
        self.i2c.read(DEVICE_ADDRESS, &mut data)?;

        Ok(DS3231Time { data })
    }

    /// Sets the time stored on the chip.
    ///
    /// The device also resets the internal fractional second counter on the ack
    /// edge immediately after the seconds byte is written. In other words,
    /// after the time is written, the device will next increment the time in ~1
    /// second.
    pub fn write_time(&mut self, time: &DS3231Time) -> Result<()> {
        let mut full_data = [0u8; CURRENT_TIME_SIZE + 1];
        full_data[0] = CURRENT_TIME_OFFSET;
        full_data[1..].copy_from_slice(&time.data);

        self.i2c.write(DEVICE_ADDRESS, &full_data)
    }

    // Will return 0 during startup and will be updated every 64 seconds.
    // Returned with a resolution of 0.25 degrees celsius.
    pub fn read_temperature(&mut self) -> Result<f32> {
        self.i2c.write(DEVICE_ADDRESS, &[TEMPERATURE_OFFSET])?;

        let mut data = [0u8; 2];
        self.i2c.read(DEVICE_ADDRESS, &mut data)?;

        // Temperature in 0.25 degree increments. May be negative.
        let num = i16::from_be_bytes(data) >> 6;

        let temp = num as f32 * 0.25;

        Ok(temp)
    }
}

// NOTE: The DS3231 allows providing an arbitrarily shift from the day of the
// week register within the [1, 7] range, but for simplicity, we only support
// Sunday always being assigned to 1.
enum_def!(Day u8 =>
    Sunday = 1,
    Monday = 2,
    Tuesday = 3,
    Wednesday = 4,
    Thursday = 5,
    Friday = 6,
    Saturday = 7
);

/// Raw time data that was pulled off the DS3231 chip at the time of reading.
pub struct DS3231Time {
    data: [u8; CURRENT_TIME_SIZE],
}

impl DS3231Time {
    fn decode_2digit_bcd(value: u8) -> u8 {
        (value & (0b1111)) + (10 * (value >> 4))
    }

    fn encode_2digit_bcd(value: u8) -> u8 {
        assert!(value < 100);
        (value % 10) | ((value / 10) << 4)
    }

    /// Gets the number of seconds since the last full minute.
    /// Returns a value in the range [0, 59].
    pub fn seconds(&self) -> u8 {
        Self::decode_2digit_bcd(self.data[0])
    }

    /// Returns a value in the range [0, 59].
    pub fn minutes(&self) -> u8 {
        Self::decode_2digit_bcd(self.data[1])
    }

    /// Returns a value in the range [0, 23].
    pub fn hours_24(&self) -> u8 {
        let v = self.data[2];

        let in_12h_mode = (1 << 6) & v != 0;

        if in_12h_mode {
            let pm = (1 << 5) & v != 0;
            Self::decode_2digit_bcd(v & 0b11111) - 1 + if pm { 12 } else { 0 }
        } else {
            Self::decode_2digit_bcd(v & 0b111111)
        }
    }

    pub fn day(&self) -> Day {
        // self.data[3] should always be in the range [1, 7].
        Day::from_value(self.data[3]).unwrap_or(Day::Sunday)
    }

    /// Returns a value in the range [1, 31].
    pub fn date(&self) -> u8 {
        Self::decode_2digit_bcd(self.data[4])
    }

    /// Returns a value in the range [1, 12].
    pub fn month(&self) -> u8 {
        Self::decode_2digit_bcd(self.data[5] & 0b11111)
    }

    /// Returns a value in the range [1900, 2099].
    pub fn year(&self) -> u32 {
        let century_set = self.data[5] & (1 << 7) != 0;

        // Lower 2 digits of the year. Range [0, 99].
        let year_2digit = Self::decode_2digit_bcd(self.data[6]) as u32;

        if century_set {
            2000 + year_2digit
        } else {
            1900 + year_2digit
        }
    }

    /// Gets the number of seconds since the unix epoch (excluding leap
    /// seconds).
    ///
    /// This is usually known as the 'International Atomic Time' standard
    /// (https://en.wikipedia.org/wiki/International_Atomic_Time).
    ///
    /// NOTE: To get unix time, you need to compensate for leap seconds.
    ///
    /// Times before the unix epoch (1970) will be clipped and returned as 0.
    pub fn to_atomic_seconds(&self) -> u32 {
        let days = {
            let year = self.year();
            if year < 1970 {
                return 0;
            }

            // Number of days added due to fully elapsed leap years.
            // 1968 is the last leap year <= 1970.
            //
            // NOTE: All years in the range [1970, 2099] that we are concerned about that
            // are divisible by 100 and/or 400 are leap years (only year 2000 is
            // relevant and it is a leap year).
            let leap_days = ((year - 1) - 1968) / 4;

            // Number of normal non-leap days in all fully elapsed years.
            let full_year_days = (year - 1970) * 365;

            // Number of days since the start of the current year.
            let partial_days = {
                let mut partial_days = (self.date() as u32) - 1;

                let month = self.month() - 1;
                for i in 0..month {
                    partial_days += DAYS_PER_MONTH[i as usize] as u32;
                }

                // If this is a leap year and it is after February, add 1 for the 29th day of
                // February.
                if year % 4 == 0 && month > 1 {
                    partial_days += 1;
                }

                partial_days
            };

            leap_days + full_year_days + partial_days
        };

        let hours = (days * 24) + (self.hours_24() as u32);

        let minutes = (hours * 60) + (self.minutes() as u32);

        let seconds = (minutes * 60) + (self.seconds() as u32);

        seconds
    }

    /// NOTE: The behavior is undefined if the year is > 2099.
    pub fn from_atomic_seconds(seconds: u32) -> Self {
        let mut data = [0u8; CURRENT_TIME_SIZE];

        let s = seconds % 60;
        data[0] = Self::encode_2digit_bcd(s as u8);

        let minutes = seconds / 60;
        let m = minutes % 60;
        data[1] = Self::encode_2digit_bcd(m as u8);

        let hours = minutes / 60;
        let h = hours % 24;
        // NOTE: The 12-hour bit won't be set so this will use the 24 hour mode.
        data[2] = Self::encode_2digit_bcd(h as u8);

        let mut days = hours / 24;

        // January 1st, 1970 was a Thursday.
        let day = ((((Day::Thursday.to_value() as u32) - 1) + days) % 7) + 1;
        data[3] = day as u8;

        let mut year: u16 = 1970;
        loop {
            let mut days_in_year = 365;
            if year % 4 == 0 {
                days_in_year += 1;
            }

            if days >= days_in_year {
                days -= days_in_year;
                year += 1;
            } else {
                break;
            }
        }

        // At this point, the 'year' is correct and in the range [1970, ...) and 'days'
        // is the number of days elapsed in the current year.

        let mut month: u8 = 0;
        loop {
            let mut days_in_month = DAYS_PER_MONTH[month as usize] as u32;
            if year % 4 == 0 && month == 1 {
                days_in_month += 1;
            }

            if days >= days_in_month {
                days -= days_in_month;
                month += 1;
            } else {
                break;
            }
        }

        // At this point, 'month' is in the range [0, 11] and the 'days' is in the range
        // [0, 30].

        data[4] = Self::encode_2digit_bcd(days as u8 + 1);

        let century_bit = {
            if year >= 2000 {
                year -= 2000;
                1 << 7
            } else {
                year -= 1900;
                0
            }
        };

        data[5] = Self::encode_2digit_bcd(month as u8 + 1) | century_bit;

        data[6] = Self::encode_2digit_bcd(year as u8);

        Self { data }
    }
}

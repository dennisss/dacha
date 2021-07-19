use std::fmt::Debug;

use common::errors::*;

const MAX_PRIMARY_LANGUAGE_ID: u16 = (1 << 10) - 1;
const MAX_SUB_LANGUAGE_ID: u8 = (1 << 6) - 1; 

/// Values pulled from https://www.usb.org/developers/docs/USB_LANGIDs.pdf
#[derive(Clone, Copy)]
pub struct Language {
    id: u16
}

impl Language {
    pub fn new(primary_language: PrimaryLanguage, sub_language: SubLanguage) -> Result<Self> {
        let primary_id = primary_language.to_value();
        let sub_id = sub_language.to_id(primary_language)?;

        if primary_id > MAX_PRIMARY_LANGUAGE_ID || sub_id > MAX_SUB_LANGUAGE_ID {
            return Err(err_msg("Primary/sub language id out of range"));
        }

        Ok(Self {
            id: primary_id | ((sub_id as u16) << 10)
        })
    }

    pub fn id(&self) -> u16 {
        self.id
    }

    pub fn primary_language(&self) -> PrimaryLanguage {
        PrimaryLanguage::from_value(self.id & MAX_PRIMARY_LANGUAGE_ID)
    }

    pub fn sub_language(&self) -> SubLanguage {
        SubLanguage::from_id(self.primary_language(), (self.id >> 10) as u8)
    }

    pub fn from_id(id: u16) -> Self {
        Self { id }
    }
}

impl Debug for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}::{:?}", self.primary_language(), self.sub_language())
    }
}

// TODO: Need to implement this with a custom comparison function
// 10-bit primary language id
enum_def_with_unknown!(PrimaryLanguage u16 =>
    Reserved = 0x00,
    Arabic = 0x01,
    Bulgarian = 0x02,
    Catalan = 0x03,
    Chinese = 0x04,
    Czech = 0x05,
    Danish = 0x06,
    German = 0x07,
    Greek = 0x08,
    English = 0x09,
    Spanish = 0x0a,
    Finnish = 0x0b,
    French = 0x0c,
    Hebrew = 0x0d,
    Hungarian = 0x0e,
    Icelandic = 0x0f,
    Italian = 0x10,
    Japanese = 0x11,
    Korean = 0x12,
    Dutch = 0x13,
    Norwegian = 0x14,
    Polish = 0x15,
    Portuguese = 0x16,
    Romanian = 0x18,
    Russian = 0x19,
    Croatian = 0x1a,
    Serbian = 0x1a,
    Slovak = 0x1b,
    Albanian = 0x1c,
    Swedish = 0x1d,
    Thai = 0x1e,
    Turkish = 0x1f,
    Urdu = 0x20,
    Indonesian = 0x21,
    Ukrainian = 0x22,
    Belarusian = 0x23,
    Slovenian = 0x24,
    Estonian = 0x25,
    Latvian = 0x26,
    Lithuanian = 0x27,
    Farsi = 0x29,
    Vietnamese = 0x2a,
    Armenian = 0x2b,
    Azeri = 0x2c,
    Basque = 0x2d,
    Macedonian = 0x2f,
    Afrikaans = 0x36,
    Georgian = 0x37,
    Faeroese = 0x38,
    Hindi = 0x39,
    Malay = 0x3e,
    Kazak = 0x3f,
    Swahili = 0x41,
    Uzbek = 0x43,
    Tatar = 0x44,
    Bengali = 0x45,
    Punjabi = 0x46,
    Gujarati = 0x47,
    Oriya = 0x48,
    Tamil = 0x49,
    Telugu = 0x4a,
    Kannada = 0x4b,
    Malayalam = 0x4c,
    Assamese = 0x4d,
    Marathi = 0x4e,
    Sanskrit = 0x4f,
    Konkani = 0x57,
    Manipuri = 0x58,
    Sindhi = 0x59,
    Kashmiri = 0x60,
    Nepali = 0x61,
    HID = 0xff
);

#[derive(Clone, Copy, Debug)]
pub enum SubLanguage {
    SaudiArabia,
    Iraq,
    Egypt,
    Libya,
    Algeria,
    Morocco,
    Tunisia,
    Oman,
    Yemen,
    Syria,
    Jordan,
    Lebanon,
    Kuwait,
    UAE,
    Bahrain,
    Qatar,
    Cyrillic,
    Latin,
    Traditional,
    Simplified,
    HongKong,
    Singapore,
    Macau,
    Standard,
    Belgian,
    US,
    UK,
    Australian,
    Canadian,
    NewZealand,
    Ireland,
    SouthAfrica,
    Jamaica,
    Caribbean,
    Belize,
    Trinidad,
    Zimbabwe,
    Philippines,
    Swiss,
    Luxembourg,
    Monaco,
    Austrian,
    Liechtenstein,
    India,
    Lithuanian,
    Malaysia,
    BruneiDarassalam,
    Bokmal,
    Nynorsk,
    Brazilian,
    Castilian,
    Mexican,
    Modern,
    Guatemala,
    CostaRica,
    Panama,
    DominicanRepublic,
    Venezuela,
    Colombia,
    Peru,
    Argentina,
    Ecuador,
    Chile,
    Uruguay,
    Paraguay,
    Bolivia,
    ElSalvador,
    Honduras,
    Nicaragua,
    PuertoRico,
    Finland,
    Pakistan,
    UsageDataDescriptor,
    VendorDefined1,
    VendorDefined2,
    VendorDefined3,
    VendorDefined4,
    Unknown(u8)    
}

macro_rules! sublang_matchers {
    ($( ( $id:expr, $primary:ident, $sub:ident ) ),*) => {
        impl SubLanguage {
            pub fn from_id(primary_language: PrimaryLanguage, id: u8) -> Self {
                match (primary_language, id) {
                    $(
                        (PrimaryLanguage::$primary, $id) => SubLanguage::$sub,
                    )*
                    _ => SubLanguage::Unknown(id)
                }
            }

            pub fn to_id(&self, primary_language: PrimaryLanguage) -> Result<u8> {
                Ok(match (primary_language, *self) {
                    $(
                        (PrimaryLanguage::$primary, SubLanguage::$sub) => $id,
                    )*
                    (_, SubLanguage::Unknown(v)) => v,
                    _ => {
                        return Err(err_msg("Mismatching primary/sub language"));
                    }
                })
            }

        }
    };
}

sublang_matchers!(
    (0x01, Arabic, SaudiArabia),
    (0x02, Arabic, Iraq),
    (0x03, Arabic, Egypt),
    (0x04, Arabic, Libya),
    (0x05, Arabic, Algeria),
    (0x06, Arabic, Morocco),
    (0x07, Arabic, Tunisia),
    (0x08, Arabic, Oman),
    (0x09, Arabic, Yemen),
    (0x10, Arabic, Syria),
    (0x11, Arabic, Jordan),
    (0x12, Arabic, Lebanon),
    (0x13, Arabic, Kuwait),
    (0x14, Arabic, UAE),
    (0x15, Arabic, Bahrain),
    (0x16, Arabic, Qatar),
    (0x01, Azeri, Cyrillic),
    (0x02, Azeri, Latin),
    (0x01, Chinese, Traditional),
    (0x02, Chinese, Simplified),
    (0x03, Chinese, HongKong),
    (0x04, Chinese, Singapore),
    (0x05, Chinese, Macau),
    (0x01, Dutch, Standard),
    (0x02, Dutch, Belgian),
    (0x01, English, US),
    (0x02, English, UK),
    (0x03, English, Australian),
    (0x04, English, Canadian),
    (0x05, English, NewZealand),
    (0x06, English, Ireland),
    (0x07, English, SouthAfrica),
    (0x08, English, Jamaica),
    (0x09, English, Caribbean),
    (0x0a, English, Belize),
    (0x0b, English, Trinidad),
    (0x0c, English, Zimbabwe),
    (0x0d, English, Philippines),
    (0x01, French, Standard),
    (0x02, French, Belgian),
    (0x03, French, Canadian),
    (0x04, French, Swiss),
    (0x05, French, Luxembourg),
    (0x06, French, Monaco),
    (0x01, German, Standard),
    (0x02, German, Swiss),
    (0x03, German, Austrian),
    (0x04, German, Luxembourg),
    (0x05, German, Liechtenstein),
    (0x01, Italian, Standard),
    (0x02, Italian, Swiss),
    (0x02, Kashmiri, India),
    (0x01, Korean, Standard),
    (0x01, Lithuanian, Standard),
    (0x01, Malay, Malaysia),
    (0x02, Malay, BruneiDarassalam),
    (0x02, Nepali, India),
    (0x01, Norwegian, Bokmal),
    (0x02, Norwegian, Nynorsk),
    (0x01, Portuguese, Standard),
    (0x02, Portuguese, Brazilian),
    (0x02, Serbian, Latin),
    (0x03, Serbian, Cyrillic),
    (0x01, Spanish, Castilian),
    (0x02, Spanish, Mexican),
    (0x03, Spanish, Modern),
    (0x04, Spanish, Guatemala),
    (0x05, Spanish, CostaRica),
    (0x06, Spanish, Panama),
    (0x07, Spanish, DominicanRepublic),
    (0x08, Spanish, Venezuela),
    (0x09, Spanish, Colombia),
    (0x0a, Spanish, Peru),
    (0x0b, Spanish, Argentina),
    (0x0c, Spanish, Ecuador),
    (0x0d, Spanish, Chile),
    (0x0e, Spanish, Uruguay),
    (0x0f, Spanish, Paraguay),
    (0x10, Spanish, Bolivia),
    (0x11, Spanish, ElSalvador),
    (0x12, Spanish, Honduras),
    (0x13, Spanish, Nicaragua),
    (0x14, Spanish, PuertoRico),
    (0x01, Swedish, Standard),
    (0x02, Swedish, Finland),
    (0x01, Urdu, Pakistan),
    (0x02, Urdu, India),
    (0x01, Uzbek, Latin),
    (0x02, Uzbek, Cyrillic),
    (0x01, HID, UsageDataDescriptor),
    (0x3c, HID, VendorDefined1),
    (0x3d, HID, VendorDefined2),
    (0x3e, HID, VendorDefined3),
    (0x3f, HID, VendorDefined4)
);

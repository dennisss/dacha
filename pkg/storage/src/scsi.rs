// This uses the https://tldp.org/HOWTO/SCSI-Generic-HOWTO/
// driver to retrieve properties from Linux connected disks.
//
// SCSI command reference:
// https://www.seagate.com/files/staticfiles/support/docs/manual/Interface%20manuals/100293068j.pdf
//
// SCSI -> ATA Pass-through:
// https://www.t10.org/ftp/t10/document.04/04-262r8.pdf
//
// ATA command reference
// https://people.freebsd.org/~imp/asiabsdcon2015/works/d2161r5-ATAATAPI_Command_Set_-_3.pdf
// https://web.archive.org/web/20200616054353if_/http://t13.org/Documents/UploadedDocuments/docs2017/di529r18-ATAATAPI_Command_Set_-_4.pdf
// https://tc.gts3.org/cs3210/2016/spring/r/hardware/ATA8-ACS.pdf
//
// Code references:
// - https://github.com/Distrotech/hdparm/blob/master/sgio.c
// - https://github.com/smartmontools/smartmontools/blob/master/smartmontools/os_linux.cpp
//
// Note that in ATA:
// - integers are stored in little endian
// - A 'word' is 16-bit
// - a 'dword' is 32-bit
//
// SMART Attribute References (vendor specific):
// - https://download.semiconductor.samsung.com/resources/others/SSD_Application_Note_SMART_final.pdf
// - TODO: Link the Seagate one.
//
// 'Concurrent Positioning Ranges'
// - https://github.com/torvalds/linux/blob/0de63bb7d91975e73338300a57c54b93d3cc151c/drivers/scsi/sd.c#L3507
//   - Linux just uses the SCSI page (0xB9)
// - https://github.com/Seagate/opensea-operations/blob/aefff734866308af17a56ff4218cca29186010cb/src/operations.c#L2630
//   - This shows to read either the ATA (the one we use) or the SCSI one.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt::{Debug, Formatter};

use common::{errors::*, format::format_bytes};
use file::LocalFile;
use sys::bindings::{
    sg_io_hdr_t, SG_DXFER_FROM_DEV, SG_GET_VERSION_NUM, SG_INFO_OK, SG_INFO_OK_MASK, SG_IO,
};

use crate::smart::*;

/// Maximum amount of time to wait for individual SCSI commands to execute.
const SCSI_TIMEOUT_MILLIS: u32 = 500;

/// Interface for controlling a disk that is supported by the linux 'SCSI
/// Generic' (SG) driver. This ends up working for ATA devices as well via SCSI
/// -> ATA command passthrough.
///
/// NOTE: All operations on this use potentially blocking ioctl calls.
pub struct SCSIDevice {
    disk: LocalFile,
}

impl SCSIDevice {
    /// Creates a new device instance from an opened block device file.
    /// This will return an error if the SG driver isn't supported on this file.
    pub fn create(mut disk: LocalFile) -> Result<Self> {
        Self::check_driver_version(&mut disk)?;
        Ok(Self { disk })
    }

    pub fn into_inner(self) -> LocalFile {
        self.disk
    }

    fn check_driver_version(disk: &mut LocalFile) -> Result<()> {
        let mut versions = 0;

        let fd = unsafe { disk.as_raw_fd() };

        let res = unsafe {
            sys::ioctl(
                fd,
                SG_GET_VERSION_NUM,
                core::mem::transmute::<&mut u64, u64>(&mut versions),
            )
        }?;

        if res != 0 {
            return Err(err_msg("Unexpected response from SG_GET_VERSION_NUM"));
        }

        if versions < 30000 {
            return Err(err_msg("Old or invalid SG driver"));
        }

        Ok(())
    }

    /// Issues an SCSI command that reads data from the device.
    fn read(&mut self, cdb: &[u8], data: &mut [u8]) -> Result<()> {
        let fd = unsafe { self.disk.as_raw_fd() };

        // For returning error data.
        let mut sense_data = [0u8; 64];

        let mut hdr = sg_io_hdr_t::default();
        hdr.interface_id = b'S' as i32;

        hdr.cmdp = unsafe { core::mem::transmute(cdb.as_ptr()) };
        hdr.cmd_len = cdb.len() as u8;

        hdr.dxferp = unsafe { core::mem::transmute(data.as_ptr()) };
        hdr.dxfer_len = data.len() as u32;
        hdr.dxfer_direction = SG_DXFER_FROM_DEV;

        hdr.sbp = unsafe { core::mem::transmute(sense_data.as_ptr()) };
        hdr.mx_sb_len = sense_data.len() as u8;

        hdr.timeout = SCSI_TIMEOUT_MILLIS;

        let res = unsafe { sys::ioctl(fd, SG_IO, core::mem::transmute::<_, u64>(&mut hdr)) }?;

        if (hdr.info & SG_INFO_OK_MASK) != SG_INFO_OK
            || hdr.status != 0
            || hdr.msg_status != 0
            || hdr.sb_len_wr != 0
            || hdr.masked_status != 0
            || hdr.host_status != 0
            || hdr.driver_status != 0
        {
            return Err(format_err!("Failure: {:?}", hdr));
        }

        // TODO: Check 'resid' to check for overflow.

        Ok(())
    }

    /// Fetches the device's serial number (as specified in the SCSI 'unit
    /// serial number' page).
    pub fn unit_serial_number(&mut self) -> Result<String> {
        let mut cdb = [
            0x12, // INQUIRY op code
            0x01, // EVPD (return the 'vital product info' based on the page code)
            0x80, // Page Code ('Unit serial number')
            0,    // Allocation Length MSB
            0,    // Allocation Length LSB
            0,    // Control
        ];

        let mut data = [0u8; 256];
        *array_mut_ref![cdb, 3, 2] = (data.len() as u16).to_be_bytes();

        self.read(&cdb, &mut data)?;

        let page_length = data[3] as usize;

        // Offset of the first byte of the serial number.
        let first_offset = 4;
        if first_offset + page_length > data.len() {
            return Err(err_msg("Buffer overflow"));
        }

        let ret = str_from_ascii(&data[first_offset..(first_offset + page_length)])
            .ok_or_else(|| err_msg("Serial number is not valid ASCII"))?;

        Ok(ret.to_string())
    }

    pub fn ata_general_purpose_log_directory(&mut self) -> Result<ATALogDirectory> {
        let mut data = [0u8; 512];
        self.ata_read_log_ext(0, &mut data)?;
        ATALogDirectory::parse(&data)
    }

    pub fn ata_smart_log_directory(&mut self) -> Result<ATALogDirectory> {
        let mut data = [0u8; 512];
        self.ata_smart_read_log(0, &mut data)?;
        ATALogDirectory::parse(&data)
    }

    /// Returns None if the device doesn't support the 'Concurrent Positioning
    /// Ranges' log page.
    pub fn ata_concurrent_positioning_ranges(
        &mut self,
    ) -> Result<Option<Vec<ATAConcurrentPositioningRange>>> {
        // First 64 bytes is a header.
        // - data[0] is the number of ranges
        //
        // Ranges start at &data[64..]
        // - Each is 32 bytes
        let mut data = [0u8; 512];

        let log = self.ata_general_purpose_log_directory()?;
        if log.num_pages_for_address(0x47) == 0 {
            return Ok(None);
        }

        self.ata_read_log_ext(0x47, &mut data)?;

        Ok(Some(Self::parse_ata_concurrent_ranges(&data)?))
    }

    fn parse_ata_concurrent_ranges(data: &[u8]) -> Result<Vec<ATAConcurrentPositioningRange>> {
        let mut num_ranges = data[0] as usize;

        if 64 + (num_ranges * 32) > data.len() {
            return Err(err_msg("Overflow supported number of ranges"));
        }

        let mut out = vec![];

        for i in 0..num_ranges {
            let offset = 64 + i * 32;

            // Index of this range in the overall list.
            let range_num = data[offset];
            if range_num != i as u8 {
                return Err(err_msg("Invalid range number"));
            }

            // Usualy 1?
            let num_storage_elements = data[offset + 1];

            let lowest_lba = {
                let mut b = [0u8; 8];
                b[0..6].copy_from_slice(array_ref![data, offset + 8, 6]);
                u64::from_le_bytes(b)
            };

            let num_lbas = u64::from_le_bytes(*array_ref![data, offset + 16, 8]);

            out.push(ATAConcurrentPositioningRange {
                num_lbas,
                num_storage_elements,
                lowest_lba,
            });
        }

        Ok(out)
    }

    /// Reads a General Purpose Log from the device.
    fn ata_read_log_ext(&mut self, log_address: u8, data: &mut [u8]) -> Result<()> {
        // NOTE: We keep the page number as 0 so this will always read from the start of
        // the log.
        self.ata_read_48bit(
            ATACommand {
                feature: 0,
                lba: (log_address as u32),
                device: 0,
                command: 0x2F,
            },
            data,
        )
    }

    pub fn ata_identify_device(&mut self) -> Result<ATAIdentityDevice> {
        let mut data = [0u8; 512];
        self.ata_read_28bit(
            ATACommand {
                feature: 0,
                lba: 0,
                device: 0,
                command: 0xEC, // 'IDENTIFY DEVICE'
            },
            &mut data,
        )?;

        ATAIdentityDevice::from_data(data)
    }

    pub fn ata_smart_read_data(&mut self) -> Result<SMARTAttributeSector> {
        let mut data = [0u8; 512];
        self.ata_read_28bit(
            ATACommand {
                feature: 0xD0,
                command: 0xB0,
                lba: (0xC24F << 8),
                device: 0,
            },
            &mut data,
        )?;

        let mut checksum: u8 = 0;
        for b in data.iter().cloned() {
            checksum = checksum.wrapping_add(b);
        }

        if checksum != 0 {
            return Err(err_msg("Invalid SMART read data checksum"));
        }

        let (attrs, _) = SMARTAttributeSector::parse(&data)?;

        Ok(attrs)
    }

    // TODO: This tends to be empty on the devices i've tested with (valid only for
    // page 0).
    //
    // TODO: Check against the directory if the log is available.
    pub fn ata_smart_device_statistics(&mut self) -> Result<ATADeviceStatisticsLog> {
        let mut data = vec![0u8; 512 * 8];
        self.ata_smart_read_log(0x04, &mut data)?;

        let inst = ATADeviceStatisticsLog { data };

        if inst.page(0).is_none() {
            return Err(err_msg("Device statistics log has no valid pages"));
        }

        Ok(inst)
    }

    fn ata_smart_read_log(&mut self, log_address: u8, data: &mut [u8]) -> Result<()> {
        self.ata_read_28bit(
            ATACommand {
                feature: 0xD5,
                command: 0xB0,
                lba: (0xC24F << 8) | (log_address as u32),
                device: 0,
            },
            data,
        )
    }

    /// Executes a 28-bit ATA Command that reads data from the device using the
    /// 'PIO Data-in' protocol.
    ///
    /// This uses the 12-byte long SCSI->ATA passthrough command.
    fn ata_read_28bit(&mut self, command: ATACommand, data: &mut [u8]) -> Result<()> {
        // TODO: Figure out if this can be dynamic in ATA
        const SECTOR_SIZE: usize = 512;

        let sector_count = {
            if data.len() % SECTOR_SIZE != 0 {
                return Err(err_msg("Can only read exact sectors"));
            }

            if data.len() / SECTOR_SIZE > 0xff {
                return Err(err_msg("Too many sectors to read in one 28-bit command"));
            }

            (data.len() / SECTOR_SIZE) as u8
        };

        let lba_bytes = command.lba.to_le_bytes();
        if lba_bytes[3] != 0 {
            return Err(err_msg("LBA too large for 28-bit command"));
        }

        let mut cdb = [0u8; 12];
        cdb[0] = 0xA1; // ATA PASS-THROUGH (12 byte)
        cdb[1] = 4 << 1; // PIO Data-in

        // T_DIR = 'Direction: From Device to Host'
        // BYT_BLOCK = 'The T_LENGTH field is the number of SECTORS (not bytes) to
        // transfer'
        // T_LENGTH = 'The length of the transfer is the SECTOR_COUNT field'.
        cdb[2] = (1 << 3) | (1 << 2) | (2 << 0);

        cdb[3] = command.feature;
        cdb[4] = sector_count;
        cdb[5] = lba_bytes[0];
        cdb[6] = lba_bytes[1];
        cdb[7] = lba_bytes[2];
        cdb[8] = command.device;
        cdb[9] = command.command;

        self.read(&cdb, data)?;

        Ok(())
    }

    fn ata_read_48bit(&mut self, command: ATACommand, data: &mut [u8]) -> Result<()> {
        // TODO: Figure out if this can be dynamic in ATA
        const SECTOR_SIZE: usize = 512;

        let mut cdb = [0u8; 16];
        cdb[0] = 0x85; // ATA PASS-THROUGH (16 byte)
        cdb[1] = 4 << 1; // PIO Data-in

        // T_DIR = 'Direction: From Device to Host'
        // BYT_BLOCK = 'The T_LENGTH field is the number of SECTORS (not bytes) to
        // transfer'
        // T_LENGTH = 'The length of the transfer is the SECTOR_COUNT field'.
        cdb[2] = (1 << 3) | (1 << 2) | (2 << 0);

        cdb[3] = 0; // command.feature high bits if u16
        cdb[4] = command.feature;

        cdb[5] = 0; // TODO: sector_count high bits
        cdb[6] = (data.len() / SECTOR_SIZE) as u8;

        // TODO: Add the rest of the LDB bits.
        cdb[8] = (command.lba & 0xff) as u8;

        cdb[13] = command.device;
        cdb[14] = command.command;

        self.read(&cdb, data)
    }
}

struct ATACommand {
    feature: u8,
    lba: u32,
    device: u8,
    command: u8,
}

fn str_from_ascii(data: &[u8]) -> Option<&str> {
    for b in data {
        if !b.is_ascii() {
            return None;
        }
    }

    core::str::from_utf8(data).ok()
}

/// The log directory lists out how many log pages are available for
/// reading/writing at each log address.
///
/// Note that there are general GPL and SMART directories / log address spaces.
#[derive(Clone, Debug)]
pub struct ATALogDirectory {
    data: Vec<u16>,
}

impl ATALogDirectory {
    pub fn num_pages_for_address(&self, addr: u8) -> u16 {
        self.data[(addr as usize) - 1]
    }

    fn parse(data: &[u8; 512]) -> Result<Self> {
        if &data[0..2] != &[1, 0] {
            return Err(err_msg("Unknown logging version"));
        }

        let mut num_pages = vec![];

        for i in 1..256 {
            let num = u16::from_le_bytes(*array_ref![data, 2 * i, 2]);
            // if num > 0xff {
            //     // TODO: This is allowed for GPL logs but not for SMART logs.
            //     return Err(err_msg("Very large log"));
            // }

            num_pages.push(num);
        }

        Ok(Self { data: num_pages })
    }
}

/// Response from an ATA 'IDENTIFY DEVICE' command.
///
/// NOTE: Internally the implemation uses word (2-byte) offsets to match the
/// convention in the standard.
pub struct ATAIdentityDevice {
    data: [u8; 512],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SATASpeed {
    Gen1,
    Gen2,
    Gen3,
}

impl Debug for ATAIdentityDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ATAIdentityDevice")
            .field("serial_number", &self.serial_number())
            .field("firmware_version", &self.firmware_revision())
            .field("model_number", &self.model_number())
            .field("sata_speed", &self.sata_speed())
            .field("maximum_queue_depth", &self.maximum_queue_depth())
            .field(
                "smart_feature_set_supported",
                &self.smart_feature_set_supported(),
            )
            .finish()
    }
}

impl ATAIdentityDevice {
    /// Performs basic validation on the 'IDENTIFY DEVICE' response data and
    /// wraps it in a struct instance.
    fn from_data(data: [u8; 512]) -> Result<Self> {
        let inst = Self { data };

        let word0 = inst.word(0);
        if word0 & (1 << 15) != 0 {
            return Err(err_msg("Not an ATA device"));
        }

        inst.serial_number_checked()
            .ok_or_else(|| err_msg("Invalid serial number"))?;

        inst.firmware_revision_checked()
            .ok_or_else(|| err_msg("Invalid firmware revision"))?;

        inst.model_number_checked()
            .ok_or_else(|| err_msg("Invalid model number"))?;

        inst.sata_speed_checked()
            .ok_or_else(|| err_msg("Invalid SATA speed"))?;

        for i in 76..(79 + 1) {
            if inst.word(i) & 0b1 != 0 {
                return Err(err_msg("Invalid bits"));
            }
        }

        let w106 = inst.word(106);
        if (w106 >> 14) & 0b11 != 0b01 {
            return Err(err_msg("Invalid bits in #106"));
        }

        /*
        word 80
            major version number
            10 supports ACS-3
            9 supports ACS-2
            8 supports ATA8-ACS

        word 84
            1 The SMART self-test is supported
            0 SMART error logging is supported

        word 85
            bit 0 The SMART feature set is enabled
        */

        Ok(inst)
    }

    // TODO: This is only valid if the corresponding 'supported' bit is set.
    // pub fn logical_sector_size(&self) -> usize {
    //     self.dword(117) as usize
    // }

    pub fn serial_number(&self) -> String {
        self.serial_number_checked().unwrap()
    }

    fn serial_number_checked(&self) -> Option<String> {
        self.word_string(10, 19)
    }

    pub fn firmware_revision(&self) -> String {
        self.firmware_revision_checked().unwrap()
    }

    fn firmware_revision_checked(&self) -> Option<String> {
        self.word_string(23, 26)
    }

    pub fn model_number(&self) -> String {
        self.model_number_checked().unwrap()
    }

    fn model_number_checked(&self) -> Option<String> {
        self.word_string(27, 46)
    }

    pub fn sata_speed(&self) -> SATASpeed {
        self.sata_speed_checked().unwrap()
    }

    fn sata_speed_checked(&self) -> Option<SATASpeed> {
        // TODO: Multiple bits may be set.

        let w = self.word(76);
        if w & (1 << 3) != 0 {
            return Some(SATASpeed::Gen3);
        }
        if w & (1 << 2) != 0 {
            return Some(SATASpeed::Gen2);
        }
        if w & (1 << 1) != 0 {
            return Some(SATASpeed::Gen1);
        }

        None
    }

    pub fn maximum_queue_depth(&self) -> usize {
        (self.word(75) & 0b1111) as usize
    }

    pub fn smart_feature_set_supported(&self) -> bool {
        self.word(82) & 1 != 0
    }

    fn word(&self, word_i: usize) -> u16 {
        u16::from_le_bytes(*array_ref![self.data, 2 * word_i, 2])
    }

    fn dword(&self, word_i: usize) -> u32 {
        u32::from_le_bytes(*array_ref![self.data, 2 * word_i, 4])
    }

    fn word_string(&self, word_i: usize, word_j: usize) -> Option<String> {
        let mut data = vec![];
        for i in word_i..(word_j + 1) {
            // Undo little endian encoding of each word.
            data.push(self.data[2 * i + 1]);
            data.push(self.data[2 * i]);
        }

        for b in &data {
            if !b.is_ascii_graphic() && *b != b' ' {
                return None;
            }
        }

        String::from_utf8(data)
            .map(|mut v| v.trim_start_matches(' ').trim_end_matches(' ').to_string())
            .ok()
    }
}

pub struct ATADeviceStatisticsLog {
    data: Vec<u8>,
}

impl ATADeviceStatisticsLog {
    /// Retrieves a single page from the log.
    /// This will return None if the page is not supported.
    fn page(&self, number: u8) -> Option<&[u8]> {
        if self.data.len() < ((number as usize) + 1) * 512 {
            return None;
        }

        let page0 = &self.data[0..512];

        // Verify the page 0 header is ok.
        {
            let header = u64::from_le_bytes(*array_ref![page0, 0, 8]);
            let log_page_num = ((header >> 16) & 0xff) as u8;
            let revision_num = (header & 0xffff) as u16;
            if log_page_num != 0 || revision_num == 0 {
                return None;
            }
        }

        let num_entries = page0[8] as usize;
        if num_entries == 0 {
            return None;
        }

        // Page 0 must be in the list
        if page0[9] != 0 {
            return None;
        }

        let mut found = false;
        for i in 0..num_entries {
            if page0[9 + i] == number {
                found = true;
                break;
            }
        }

        if !found {
            return None;
        }

        let page_i = &self.data[(512 * (number as usize))..(512 * ((number as usize) + 1))];

        // Verify the header.
        {
            let header = u64::from_le_bytes(*array_ref![page_i, 0, 8]);
            let log_page_num = ((header >> 16) & 0xff) as u8;
            let revision_num = (header & 0xffff) as u16;
            if log_page_num != number || revision_num == 0 {
                return None;
            }
        }

        Some(page_i)
    }

    /// Returns the current temperature in Celsius.
    pub fn current_temperature(&self) -> Option<i8> {
        let page = match self.page(0x05) {
            Some(v) => v,
            None => return None,
        };

        Some(page[8] as i8)
    }
}

#[derive(Clone, Debug)]
pub struct ATAConcurrentPositioningRange {
    pub num_storage_elements: u8,
    pub lowest_lba: u64,
    pub num_lbas: u64,
}

#[cfg(test)]
mod tests {
    use super::SCSIDevice;

    #[test]
    fn concurrent_positioning_ranges_test() {
        // Response page from a Seagate 18TB 2X18 drive

        let data = [
            2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 23, 4, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 192, 23, 4, 0, 0, 0, 0, 0,
            192, 23, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];

        let out = SCSIDevice::parse_ata_concurrent_ranges(&data).unwrap();

        println!("{:?}", out);
    }
}

use std::io::Cursor;
use std::os::linux::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
/// See specification in:
/// https://en.wikipedia.org/wiki/Tar_(computing)
/// https://pubs.opengroup.org/onlinepubs/9699919799/utilities/pax.html#tag_20_92_13_06
use std::usize;

use common::check_zero_padding;
use common::errors::*;
use common::io::{Readable, Seekable, Writeable};
use file::{LocalFile, LocalFileOpenOptions, LocalPath, LocalPathBuf};

const BLOCK_SIZE: u64 = 512;

const USTAR_OLD_GNU_MAGIC: &'static [u8; 6] = b"ustar ";
const USTAR_OLD_GNU_VERSION: &'static [u8; 2] = b" \0";

const USTAR_POSIX_MAGIC: &'static [u8; 6] = b"ustar\0";
const USTAR_POSIX_VERSION: &'static [u8; 2] = b"00";

mod proto {
    #![allow(dead_code, non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/src/tar.rs"));
}

enum_def_with_unknown!(FileType u8 =>
    // NOTE: This could also be b'\0'
    NormalFile = b'0',

    HardLink = b'1',
    SymbolicLink = b'2',

    // The below definitions are only available in the USTar format.
    CharacterSpecial = b'3',
    BlockSpecial = b'4',
    Directory = b'5',
    FIFO = b'6',
    ContiguousFile = b'7'
);

#[derive(Debug)]
pub struct FileEntry {
    pub metadata: FileMetadata,

    /// Position in the archive file at which
    pub offset: u64,
}

impl FileEntry {
    pub fn is_directory(&self) -> bool {
        if self.metadata.ustar_extension.is_some() {
            return self.metadata.header.file_type == FileType::Directory;
        }

        // NOTE: This is a heuristic and not defined in an official specification.
        self.metadata.header.file_type == FileType::NormalFile
            && self.metadata.header.file_name.ends_with("/")
    }

    pub fn is_regular(&self) -> bool {
        if self.is_directory() {
            return false;
        }

        self.metadata.header.file_type == FileType::NormalFile
    }
}

#[derive(Debug)]
pub struct FileMetadata {
    pub header: Header,

    // NOTE: It is recommended to always include this when serializing.
    pub ustar_extension: Option<USTarHeaderExtension>,
}

#[derive(Debug)]
pub struct Header {
    pub file_name: String,
    pub file_mode: Option<u32>,
    pub owner_id: Option<u32>,
    pub group_id: Option<u32>,
    pub file_size: Option<u64>,
    pub last_modified_time: Option<u64>,
    pub file_type: FileType,
    pub linked_file_name: String,
}

#[derive(Debug)]
pub struct USTarHeaderExtension {
    pub owner_name: String,
    pub group_name: String,
    pub device_major_number: Option<u32>,
    pub device_minor_number: Option<u32>,
    pub file_name_prefix: String,
}

/// TODO: Move this into a general location so that we don't need to
pub struct AppendFileOptions {
    /// Root directory to use for the new Tar archive.
    ///
    /// - All files appended to the archive MUST be currently located in this
    ///   directory in the real filesystem.
    /// - This path will be 'mounted' to 'output_dir' inside of the archive
    ///   namespace.
    pub root_dir: LocalPathBuf,

    /// Directory in the archive namespace in which all files will be added.
    ///
    /// Defaults to "" meaning that the a file entry will have its name set to
    /// the path of the original file relative to 'root_dir'.
    pub output_dir: Option<LocalPathBuf>,

    pub mask: FileMetadataMask,

    pub anonymize: bool,
}

/// When writing files originally from the local file system to
pub struct FileMetadataMask {}

pub struct Reader<Input> {
    file: Input,

    /// Current location in the file. We assume that 'file' is seeked to this
    /// position.
    current_offset: u64,

    /// Offset at which the current file's data ends (excludes block padding).
    data_end_offset: u64,

    /// Offset into the archive file of the next unread file header.
    next_offset: u64,
}

impl Reader<LocalFile> {
    pub async fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        Ok(Self::new(LocalFile::open(path)?))
    }
}

impl<Input: Readable> Reader<Input> {
    /// NOTE: The file should be currently seeked to the beginning.
    pub fn new(file: Input) -> Self {
        Self {
            file,
            current_offset: 0,
            data_end_offset: 0,
            next_offset: 0,
        }
    }

    /// Reads the next file entry in the archive.
    ///
    /// This is only allowed to be called at the beginning of the file or after
    /// reading (or skipping) all data in the previous file.
    pub async fn read_entry(&mut self) -> Result<Option<FileEntry>> {
        if self.current_offset != self.next_offset {
            return Err(err_msg(
                "Not at a file offset. Must read or skip all data in the previous file.",
            ));
        }

        let mut block = [0u8; BLOCK_SIZE as usize];
        self.file.read_exact(&mut block).await?;
        self.current_offset += BLOCK_SIZE;

        // The end of the archive is marked by two nul records.
        if check_zero_padding(&block).is_ok() {
            self.file.read_exact(&mut block).await?;
            check_zero_padding(&block)?;

            // NOTE: We can't check that we hit the end of the file as some implementations
            // may pad up to even larger block sizes.
            /*
            let current_position = self.file.seek(SeekFrom::Current(0)).await?;
            let archive_length = self.file.metadata().await?.len();

            if current_position != archive_length {
                return Err(err_msg("Saw null records before end of file"));
            }
            */

            return Ok(None);
        }

        let (header, mut rest) = Self::parse_header(&block)?;

        let ustar = {
            if let Some((ustar, ustar_rest)) = Self::parse_ustar_extension(rest)? {
                rest = ustar_rest;
                Some(ustar)
            } else {
                None
            }
        };

        check_zero_padding(rest).map_err(|_| err_msg("Pad padding after tar header"))?;

        let entry = FileEntry {
            metadata: FileMetadata {
                header,
                ustar_extension: ustar,
            },
            offset: self.next_offset,
        };

        let file_size = entry.metadata.header.file_size.unwrap_or(0);

        self.data_end_offset = self.current_offset + file_size;

        self.next_offset = self.current_offset
            + BLOCK_SIZE * (common::ceil_div(file_size as usize, BLOCK_SIZE as usize) as u64);

        Ok(Some(entry))
    }

    // NOTE: Data should be the entire 512 byte block.
    fn parse_header(data: &[u8]) -> Result<(Header, &[u8])> {
        let (raw_header, raw_header_rest) = proto::Header::parse(data)?;

        let stored_checksum = Self::parse_checksum_value(&raw_header.header_checksum)?;

        // NOTE: The checksum is computed over the entire block.
        let expected_checksum = calculate_checksum(data);

        if stored_checksum != expected_checksum {
            return Err(err_msg("Invalid checksum in header"));
        }

        let mut raw_file_type = raw_header.file_type;
        if raw_file_type == 0 {
            raw_file_type = b'1';
        }

        Ok((
            Header {
                file_name: Self::parse_string_value(&raw_header.file_name)?,
                file_mode: Self::parse_numeric_value(&raw_header.file_mode)?.map(|v| v as u32),
                owner_id: Self::parse_numeric_value(&raw_header.owner_id)?.map(|v| v as u32),
                group_id: Self::parse_numeric_value(&raw_header.group_id)?.map(|v| v as u32),
                file_size: Self::parse_numeric_value(&raw_header.file_size)?,
                last_modified_time: Self::parse_numeric_value(&raw_header.last_modified_time)?,
                file_type: FileType::from_value(raw_file_type),
                linked_file_name: Self::parse_string_value(&raw_header.linked_file_name)?,
            },
            raw_header_rest,
        ))
    }

    fn parse_ustar_extension(data: &[u8]) -> Result<Option<(USTarHeaderExtension, &[u8])>> {
        let (raw_ustar, raw_ustar_rest) = proto::USTarHeaderExtension::parse(data)?;

        let magic_version = (&raw_ustar.ustar_magic, &raw_ustar.ustar_version);

        let valid = (magic_version == (USTAR_OLD_GNU_MAGIC, USTAR_OLD_GNU_VERSION)
            || magic_version == (USTAR_POSIX_MAGIC, USTAR_POSIX_VERSION));

        if !valid {
            return Ok(None);
        }

        Ok(Some((
            USTarHeaderExtension {
                owner_name: Self::parse_string_value(&raw_ustar.owner_name)?,
                group_name: Self::parse_string_value(&raw_ustar.group_name)?,
                device_major_number: Self::parse_numeric_value(&raw_ustar.device_major_number)?
                    .map(|v| v as u32),
                device_minor_number: Self::parse_numeric_value(&raw_ustar.device_minor_number)?
                    .map(|v| v as u32),
                file_name_prefix: Self::parse_string_value(&raw_ustar.file_name_prefix)?,
            },
            raw_ustar_rest,
        )))
    }

    /// NOTE: All the tar strings are strictly Ascii and terminated with zero
    /// padding.
    fn parse_string_value(data: &[u8]) -> Result<String> {
        let mut string_end = None;

        for i in 0..data.len() {
            if string_end.is_none() {
                if data[i] == 0 {
                    string_end = Some(i);
                } else if !data[i].is_ascii() {
                    return Err(err_msg("String contains non-ASCII bytes"));
                }
            } else {
                if data[i] != 0 {
                    return Err(err_msg("Expected string to be padded with null bytes"));
                }
            }
        }

        let string_end = string_end.unwrap_or(data.len());

        let s = std::str::from_utf8(&data[0..string_end])?;

        Ok(s.to_string())
    }

    /// Parses a numeric value from one of the header fields.
    /// The format is in octal with leading zeros and followed by a null or
    /// space character.
    fn parse_numeric_value(data: &[u8]) -> Result<Option<u64>> {
        if data[0] == 0 {
            check_zero_padding(data)?;
            return Ok(None);
        }

        let last_byte = data[data.len() - 1];
        if last_byte != b'\0' && last_byte != b' ' {
            return Err(err_msg(
                "Numeric field doens't end in NUL or space character",
            ));
        }

        let octal_data = &data[0..(data.len() - 1)];
        let num = Self::parse_octal(octal_data)?;
        Ok(Some(num))
    }

    /// Parses the header checksum value.
    fn parse_checksum_value(data: &[u8]) -> Result<u32> {
        let octal_data = data
            .strip_suffix(b"\0 ")
            .ok_or_else(|| err_msg("Invalid suffix on checksum field"))?;
        let num = Self::parse_octal(octal_data)? as u32;
        Ok(num)
    }

    fn parse_octal(data: &[u8]) -> Result<u64> {
        if data.len() == 0 {
            return Ok(0);
        }

        let mut out = 0;
        for i in 0..data.len() {
            let digit = (data[i] as char)
                .to_digit(8)
                .ok_or_else(|| err_msg("Invalid octal digit"))? as u64;

            out = (out << 3) | digit;
        }

        Ok(out)
    }
}

#[async_trait]
impl<Input: Readable> Readable for Reader<Input> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.current_offset == self.next_offset {
            return Ok(0);
        }

        let n = core::cmp::min(
            buf.len(),
            (self.data_end_offset - self.current_offset) as usize,
        );
        let n_read = self.file.read(&mut buf[0..n]).await?;

        self.current_offset += n_read as u64;
        if self.current_offset == self.data_end_offset {
            let padding_amount = (self.next_offset - self.data_end_offset) as usize;
            let mut block = [0u8; BLOCK_SIZE as usize];

            self.file.read_exact(&mut block[0..padding_amount]).await?;
            check_zero_padding(&block[padding_amount..])?;
            self.current_offset = self.next_offset;
        }

        Ok(n_read)
    }
}

impl<Input: Seekable> Reader<Input> {
    pub async fn seek_to_file(&mut self, entry: &FileEntry) -> Result<()> {
        self.current_offset = entry.offset + BLOCK_SIZE;

        // TODO: Deduplicate with above.
        let file_size = entry.metadata.header.file_size.unwrap_or(0);
        self.data_end_offset = self.current_offset + file_size;
        self.next_offset = self.current_offset
            + BLOCK_SIZE * (common::ceil_div(file_size as usize, BLOCK_SIZE as usize) as u64);

        self.file.seek(self.current_offset).await
    }

    pub async fn skip_current_file(&mut self) -> Result<()> {
        self.file.seek(self.next_offset).await
    }
}

impl<Input: Readable> Reader<Input> {
    pub async fn extract_files(&mut self, output_dir: &LocalPath) -> Result<()> {
        self.extract_files_with_modes(output_dir, None, None).await
    }

    // TODO: Deduplicate this code.
    async fn create_dir_all(
        &self,
        output_dir: &LocalPath,
        mut dir: &LocalPath,
        dir_mode: Option<u32>,
    ) -> Result<()> {
        let mut pending = vec![];

        loop {
            if dir == output_dir || file::exists(dir).await? {
                break;
            }

            pending.push(dir);

            dir = dir
                .parent()
                .ok_or_else(|| err_msg("CAn't get parent path"))?;
        }

        while let Some(path) = pending.pop() {
            file::create_dir(path).await?;

            if let Some(mode) = dir_mode {
                let mut perms = file::metadata(path).await?.permissions();
                perms.set_mode(mode);
                file::set_permissions(&path, perms).await?;
            }
        }

        Ok(())
    }

    // TODO: For the purposes of resuming the extraction of a bundle, we need to
    // support writing over an existing archive.

    // NOTE: This will only success if none of the files we are extracting exist yet
    // in the output_dir.
    pub async fn extract_files_with_modes(
        &mut self,
        output_dir: &LocalPath,
        file_mode: Option<u32>,
        dir_mode: Option<u32>,
    ) -> Result<()> {
        let mut output_dir = output_dir.to_owned();
        if !output_dir.is_absolute() {
            output_dir = file::current_dir()?.join(output_dir);
        }

        while let Some(entry) = self.read_entry().await? {
            let mut relpath = LocalPath::new(&entry.metadata.header.file_name);

            if relpath.is_absolute() {
                relpath = relpath.strip_prefix("/").unwrap();
            }

            // Path in the real filesystem at which we will write the extracted file.
            // NOTE: We normalize and re-validate the prefix in case of '..' in the paths.
            let path = output_dir.join(&relpath).normalized();
            if !path.starts_with(&output_dir) {
                return Err(err_msg(
                    "Archive attempted to extract to a path outside the output directory",
                ));
            }

            // NOTE: We assume that separate directory entries are present and precede all
            // entries within that directory.
            // TODO: Make this optional as we should prefer to have directory entries in the
            // tar.
            {
                let dir = path
                    .parent()
                    .ok_or_else(|| err_msg("Can't get parent path"))?;

                self.create_dir_all(&output_dir, dir, dir_mode).await?;
            }

            if entry.is_regular() {
                let mut file = LocalFile::open_with_options(
                    &path,
                    &LocalFileOpenOptions::new().create_new(true).write(true),
                )?;
                // TODO: Reserve contiguous space on disk for the file.

                self.pipe(&mut file).await?;
                file.flush().await?;

                // Preserve any execute bits on regular files.
                {
                    let mut perms = file.metadata().await?.permissions();

                    let mut base_mode = perms.mode();
                    if let Some(mode) = file_mode {
                        base_mode = mode;
                    }

                    perms.set_mode(
                        base_mode | (entry.metadata.header.file_mode.unwrap_or(0) & 0o111),
                    );

                    // TODO: Only run this if the permissions changed.
                    file.set_permissions(perms).await?;
                }
            } else if entry.is_directory() {
                if !file::exists(&path).await? {
                    file::create_dir(&path).await?;
                }

                if let Some(mode) = dir_mode {
                    let mut perms = file::metadata(&path).await?.permissions();
                    perms.set_mode(mode);
                    file::set_permissions(&path, perms).await?;
                }
            } else {
                return Err(err_msg("Unsupported entry"));
            }
        }

        Ok(())
    }
}

fn calculate_checksum(block: &[u8]) -> u32 {
    let mut sum = 0;
    for i in 0..block.len() {
        // Sum the checksum field as spaces.
        if i >= 148 && i < 148 + 8 {
            sum += b' ' as u32;
            continue;
        }

        sum += block[i] as u32;
    }

    sum
}

pub struct Writer<Output> {
    // NOTE: This should always be seeked to the end of the file.
    file: Output,
}

impl Writer<LocalFile> {
    /// TODO: Support appending files to the end of an archive.
    /// (quickest way is to find scan backwards )
    pub async fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        Ok(Self {
            file: LocalFile::open_with_options(
                path,
                &LocalFileOpenOptions::new()
                    //.create_new(true)
                    .create(true)
                    .write(true),
            )?,
        })
    }
}

impl<Output: Writeable> Writer<Output> {
    /// Creates a writer using a writer which corresponds to an EMPTY file.
    pub fn new(file: Output) -> Self {
        Self { file }
    }

    /// Appends a single file to the end of the file using the exact metadata
    /// given.
    pub async fn append(
        &mut self,
        metadata: &FileMetadata,
        reader: &mut dyn Readable,
    ) -> Result<()> {
        let mut header_block = vec![];

        let mut raw_header = proto::Header {
            file_name: [0; 100],
            file_mode: [0; 8],
            owner_id: [0; 8],
            group_id: [0; 8],
            file_size: [0; 12],
            last_modified_time: [0; 12],
            header_checksum: [0; 8],
            file_type: 0,
            linked_file_name: [0u8; 100],
        };

        Self::serialize_string(&metadata.header.file_name, &mut raw_header.file_name)?;
        Self::serialize_numeric_value(
            &metadata.header.file_mode.map(|v| v as u64),
            &mut raw_header.file_mode,
        )?;
        Self::serialize_numeric_value(
            &metadata.header.owner_id.map(|v| v as u64),
            &mut raw_header.owner_id,
        )?;
        Self::serialize_numeric_value(
            &metadata.header.group_id.map(|v| v as u64),
            &mut raw_header.group_id,
        )?;
        Self::serialize_numeric_value(&metadata.header.file_size, &mut raw_header.file_size)?;
        Self::serialize_numeric_value(
            &metadata.header.last_modified_time,
            &mut raw_header.last_modified_time,
        )?;
        raw_header.file_type = metadata.header.file_type.to_value();
        Self::serialize_string(
            &metadata.header.linked_file_name,
            &mut raw_header.linked_file_name,
        )?;
        raw_header.serialize(&mut header_block)?;

        if let Some(ustar) = &metadata.ustar_extension {
            let mut raw_ustar = proto::USTarHeaderExtension {
                ustar_magic: *USTAR_POSIX_MAGIC,
                ustar_version: *USTAR_POSIX_VERSION,
                owner_name: [0u8; 32],
                group_name: [0u8; 32],
                device_major_number: [0u8; 8],
                device_minor_number: [0u8; 8],
                file_name_prefix: [0u8; 155],
            };

            Self::serialize_string(&ustar.owner_name, &mut raw_ustar.owner_name)?;
            Self::serialize_string(&ustar.group_name, &mut raw_ustar.group_name)?;
            Self::serialize_numeric_value(
                &ustar.device_major_number.map(|v| v as u64),
                &mut raw_ustar.device_major_number,
            )?;
            Self::serialize_numeric_value(
                &ustar.device_minor_number.map(|v| v as u64),
                &mut raw_ustar.device_minor_number,
            )?;

            // TODO: Support long file names using the file_name_prefix

            raw_ustar.serialize(&mut header_block)?;
        }

        // Add checksum now that we are done writing.
        {
            let checksum = calculate_checksum(&header_block);
            let checksum_data = &mut header_block[148..(148 + 8)];

            let s = format!("{:06o}\0 ", checksum);
            assert_eq!(s.len(), checksum_data.len());

            checksum_data.copy_from_slice(s.as_bytes());
        }

        header_block.resize(BLOCK_SIZE as usize, 0);

        self.file.write_all(&header_block).await?;

        // Maximum number of bytes that we will transfer in a single loop cycle.
        // NOTE: Must be divisible by the BLOCK_SIZE.
        const TRANSFER_BLOCK_SIZE: usize = 8192;

        // Now writing the file itself.
        // TODO: Add a helper for this (it's basically piping from one reader into
        // another writer)
        let file_size = metadata.header.file_size.unwrap_or(0);
        let mut n = 0;
        while n < file_size {
            let mut block = [0u8; TRANSFER_BLOCK_SIZE];
            let nblock = std::cmp::min(block.len() as u64, file_size - n) as usize;
            reader.read_exact(&mut block[0..nblock]).await?;

            let nblock_padded =
                common::ceil_div(nblock, BLOCK_SIZE as usize) * (BLOCK_SIZE as usize);

            self.file.write_all(&mut block[0..nblock_padded]).await?;
            n += nblock_padded as u64;
        }

        // Verify we hit the end of the reader.
        {
            let mut buf = [0u8; 1];
            if reader.read(&mut buf).await? != 0 {
                return Err(err_msg("Extra data in input file"));
            }
        }

        Ok(())
    }

    pub async fn append_regular_file(
        &mut self,
        name: &str,
        size: u64,
        reader: &mut dyn Readable,
    ) -> Result<()> {
        let metadata = FileMetadata {
            header: Header {
                file_name: name.to_string(),
                file_mode: None,
                owner_id: None,
                group_id: None,
                file_size: Some(size),
                last_modified_time: None,
                file_type: FileType::NormalFile,
                linked_file_name: "".into(),
            },
            ustar_extension: None,
        };

        self.append(&metadata, reader).await
    }

    fn serialize_string(value: &str, out: &mut [u8]) -> Result<()> {
        if value.len() + 1 > out.len() {
            return Err(err_msg("String doesn't fit"));
        }

        for byte in value.as_bytes() {
            if *byte == 0 || !byte.is_ascii() {
                return Err(err_msg("Can only use ASCII strings in tar file"));
            }
        }

        out[0..value.len()].copy_from_slice(value.as_bytes());
        out[value.len()] = 0;
        Ok(())
    }

    fn serialize_numeric_value(value: &Option<u64>, out: &mut [u8]) -> Result<()> {
        if let Some(value) = value {
            let num_str = format!("{0:01$o}\0", value, out.len() - 1);
            if num_str.len() > out.len() {
                return Err(err_msg("Number overflows tar field range"));
            }

            out[0..num_str.len()].copy_from_slice(num_str.as_bytes());
        } else {
            for i in 0..out.len() {
                out[i] = 0;
            }
        }

        Ok(())
    }

    /// Appends a file currently on disk to the archive.
    ///
    /// If the file is a
    // TODO: We should also append entries for each directory. that is a parent of
    // the path
    pub async fn append_file(
        &mut self,
        path: &LocalPath,
        options: &AppendFileOptions,
    ) -> Result<()> {
        // Ideally normalize and make absolute all the paths.

        let mut pending_paths: Vec<LocalPathBuf> = vec![];
        pending_paths.push(path.to_owned());

        // DFS
        while let Some(path) = pending_paths.pop() {
            self.append_single_file(&path, options, &mut pending_paths)
                .await?;
        }

        Ok(())
    }

    async fn append_single_file(
        &mut self,
        path: &LocalPath,
        options: &AppendFileOptions,
        pending_paths: &mut Vec<LocalPathBuf>,
    ) -> Result<()> {
        // NOTE: We will not follow symlinks when resolving metadata.
        // TODO: Switch back this.
        let metadata = file::metadata(path).await?;

        let (file_type, file_size, mut reader): (FileType, u64, Box<dyn Readable>) = {
            if metadata.is_dir() {
                (FileType::Directory, 0, Box::new(Cursor::new(&[])))
            } else if metadata.is_file() || metadata.is_symlink() {
                let file = LocalFile::open(path)?;
                // If this is a symlink, then the length will be wrong.
                (FileType::NormalFile, metadata.len(), Box::new(file))
            } else {
                return Err(err_msg("Unsupported file type"));
            }
        };

        // Name/path of the file as it will appear in the tar file.
        let mut file_name = options
            .output_dir
            .as_ref()
            .map(|v| v.as_path())
            .unwrap_or(LocalPath::new(""))
            .join(
                path.strip_prefix(&options.root_dir)
                    .ok_or_else(|| err_msg("Path does not start with root_dir"))?,
            )
            .normalized()
            .to_string();

        // Only directories will end up '/'
        if file_type == FileType::Directory {
            file_name.push('/');
        }

        let mut last_modified_time = None;
        last_modified_time = Some(
            metadata
                .modified()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );

        let mut archive_metadata = FileMetadata {
            header: Header {
                file_name,
                file_mode: Some(metadata.st_mode() & 0o777),
                owner_id: Some(metadata.st_uid()),
                group_id: Some(metadata.st_gid()),
                file_size: Some(file_size),
                last_modified_time,
                file_type,
                linked_file_name: String::new(),
            },
            ustar_extension: Some(USTarHeaderExtension {
                // TODO: Look these up: https://man7.org/linux/man-pages/man3/getpwuid.3.html
                owner_name: String::new(),
                group_name: String::new(),
                device_major_number: None,
                device_minor_number: None,
                file_name_prefix: String::new(),
            }),
        };

        if options.anonymize {
            archive_metadata.header.last_modified_time = None;
            archive_metadata.header.owner_id = None;
            archive_metadata.header.group_id = None;
        }

        self.append(&archive_metadata, reader.as_mut()).await?;

        if metadata.is_dir() {
            for entry in file::read_dir(path)? {
                pending_paths.push(path.join(entry.name()));
            }
        }

        Ok(())
    }

    pub async fn flush(&mut self) -> Result<()> {
        self.file.flush().await?;
        Ok(())
    }

    /// Call when you are done appending records to add the end marker to the
    /// file.
    pub async fn finish(mut self) -> Result<()> {
        let zero_blocks = [0u8; 2 * BLOCK_SIZE as usize];
        self.file.write_all(&zero_blocks).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use file::temp::TempDir;

    use super::*;

    #[testcase]
    async fn read_existing_archive() -> Result<()> {
        let mut variations = [
            project_path!("testdata/tar/archive.tar"),
            // Doesn't contain directory entries so we must create them implicitly.
            // TODO: Check the above comment is correct.
            project_path!("testdata/tar/archive2.tar"),
        ];

        for archive_path in variations {
            let temp_dir = TempDir::create()?;

            let mut reader = Reader::open(archive_path).await?;
            reader.extract_files(temp_dir.path()).await?;

            assert_eq!(
                file::read_to_string(temp_dir.path().join("first_file")).await?,
                "Hello world\n"
            );
            assert_eq!(
                file::read_to_string(temp_dir.path().join("second_dir/third_file")).await?,
                "This is data in the directory\n"
            );
        }

        Ok(())
    }

    #[testcase]
    async fn write_archive() -> Result<()> {
        let temp_dir = TempDir::create()?;

        let archive_path = temp_dir.path().join("archive.tar");

        {
            let mut writer = Writer::open(&archive_path).await?;

            writer
                .append_file(
                    &project_path!("testdata/tar/data"),
                    &AppendFileOptions {
                        root_dir: project_path!("testdata/tar/data"),
                        output_dir: None,
                        mask: FileMetadataMask {},
                        anonymize: false,
                    },
                )
                .await?;

            writer.finish().await?;
        }

        {
            let output_dir = temp_dir.path().join("out");
            // file::create_dir(&output_dir).await?;

            let mut reader = Reader::open(&archive_path).await?;
            reader.extract_files(&output_dir).await?;

            assert_eq!(
                file::read_to_string(output_dir.join("first_file")).await?,
                "Hello world\n"
            );
            assert_eq!(
                file::read_to_string(output_dir.join("second_dir/third_file")).await?,
                "This is data in the directory\n"
            );
        }

        Ok(())
    }
}

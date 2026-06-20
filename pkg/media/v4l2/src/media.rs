use std::{collections::HashSet, sync::Arc};

use base_error::*;
use base_util::null_terminated::read_null_terminated_string;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::AsyncVariable;
use file::LocalPathBuf;
use file::{LocalFile, LocalFileOpenOptions, LocalPath, DeviceNumber};
use sys::EpollEvents;
use sys::Errno;

use crate::io::*;
use crate::stream::*;
use crate::ControlDefinition;
use crate::{bindings::*, ControlMenuItem};


pub struct MediaDevice {
    file: LocalFile,
    path: LocalPathBuf,
    device_info: media_device_info
}

impl MediaDevice {
    pub async fn list() -> Result<Vec<Self>> {
        let mut out = vec![];
        for entry in file::read_dir("/dev")? {
            if !entry.name().starts_with("media") {
                continue;
            }

            let path = LocalPath::new("/dev").join(entry.name());

            out.push(Self::open(path).await?);
        }

        // TODO: Sort by the number.

        out.sort_by(|a, b| a.path().as_str().cmp(b.path().as_str()));

        Ok(out)
    }

    pub async fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = file::LocalFile::open_with_options(
            path,
            &LocalFileOpenOptions::new()
                .read(true)
                .write(true)
                .non_blocking(true),
        )?;

        let mut device_info = media_device_info::default();
        unsafe { media_ioc_device_info(file.as_raw_fd(), &mut device_info) }?;

        Ok(Self {
            file,
            device_info,
            path: path.to_owned()
        })
    }

    pub fn path(&self) -> &LocalPath {
        &self.path
    }

    pub fn driver(&self) -> Result<String> {
        read_null_terminated_string(&self.device_info.driver)
    }

    pub fn print_device_info(&self) -> Result<()> {
        println!("Driver: {}", self.driver()?);
        println!("Model: {}", read_null_terminated_string(&self.device_info.model)?);
        println!("Serial: {}", read_null_terminated_string(&self.device_info.serial)?);
        println!("Bus Info: {}", read_null_terminated_string(&self.device_info.bus_info)?);

        Ok(())
    }

    pub fn list_entities(&mut self) -> Result<Vec<MediaEntity>> {
        let file = &mut self.file;

        let mut out = vec![];

        let mut raw = media_entity_desc::default();
        raw.id = 0 | MEDIA_ENT_ID_FLAG_NEXT;

        loop {
            match unsafe { media_ioc_enum_entities(file.as_raw_fd(), &mut raw) } {
                Ok(i) => {
                    assert_eq!(i, 0);
                }
                Err(Errno::EINVAL) => break,
                Err(e) => break,
            };

            let mut pads = vec![media_pad_desc::default(); raw.pads as usize];
            let mut links = vec![media_link_desc::default(); raw.links as usize];

            let mut links_enum = media_links_enum::default();
            links_enum.entity = raw.id;
            links_enum.pads = pads.as_mut_ptr();
            links_enum.links = links.as_mut_ptr();
            unsafe { media_ioc_enum_links(file.as_raw_fd(), &mut links_enum) }?;

            out.push(MediaEntity {
                desc: raw,
                pads: pads.into_iter().map(|desc| MediaPad { desc }).collect(),
                links: links.into_iter().map(|desc| MediaLink { desc }).collect()
            });
            raw.id |= MEDIA_ENT_ID_FLAG_NEXT
        }

        Ok(out)
    }

    pub fn enable_link(&mut self, link: &MediaLink) -> Result<()> {
        let mut desc = link.desc.clone();

        let mut flags = MediaLinkFlags::from_raw(desc.flags);
        flags |= MediaLinkFlags::Enabled;
        desc.flags = flags.to_raw();

        unsafe { media_ioc_setup_link(self.file.as_raw_fd(), &mut desc) }?;

        Ok(())
    }
}


define_bit_flags!(MediaPadFlags u32 {
    Sink = MEDIA_PAD_FL_SINK,
    Source = MEDIA_PAD_FL_SOURCE,
    MustConnect = MEDIA_PAD_FL_MUST_CONNECT
});

define_bit_flags!(MediaLinkFlags u32 {
    Enabled = MEDIA_LNK_FL_ENABLED,
    Immutable = MEDIA_LNK_FL_IMMUTABLE,
    Dynamic = MEDIA_LNK_FL_DYNAMIC
});

enum_def_with_unknown!(MediaEntityType u32 =>
    V4L2_VIDEO = MEDIA_ENT_T_V4L2_VIDEO,
    V4L2_SUBDEV = MEDIA_ENT_T_V4L2_SUBDEV,
    V4L2_SUBDEV_SENSOR = MEDIA_ENT_T_V4L2_SUBDEV_SENSOR
);

pub struct MediaEntity {
    desc: media_entity_desc,
    pads: Vec<MediaPad>,
    links: Vec<MediaLink>
}

impl MediaEntity {

    pub fn id(&self) -> u32 {
        self.desc.id
    }

    pub fn name(&self) -> Result<String> {
        read_null_terminated_string(&self.desc.name)
    }

    pub fn typ(&self) -> MediaEntityType {
        MediaEntityType::from_value(self.desc.type_) 
    } 

    pub fn device_num(&self) -> Option<DeviceNumber> {
        // Verify the type supports having a device.
        match self.typ() {
            MediaEntityType::V4L2_VIDEO => {},
            MediaEntityType::V4L2_SUBDEV => {},
            MediaEntityType::V4L2_SUBDEV_SENSOR => {},
            MediaEntityType::Unknown(_) => return None
        };

        let (major, minor) = unsafe {
            (self.desc.__bindgen_anon_1.dev.major,
                self.desc.__bindgen_anon_1.dev.minor
            )
        };

        Some(DeviceNumber::new(major, minor))
    }

    pub fn pads(&self) -> &[MediaPad] {
        &self.pads
    }

    pub fn links(&self) -> &[MediaLink] {
        &self.links
    }
}

pub struct MediaPad {
    desc: media_pad_desc
}

impl MediaPad {
    pub fn entity_id(&self) -> u32 {
        self.desc.entity
    }

    pub fn index(&self) -> usize {
        self.desc.index as usize
    }

    pub fn flags(&self) -> MediaPadFlags {
        MediaPadFlags::from_raw(self.desc.flags)
    }
}

pub struct MediaLink {
    desc: media_link_desc
}

impl MediaLink {
    pub fn flags(&self) -> MediaLinkFlags {
        MediaLinkFlags::from_raw(self.desc.flags)
    }

    pub fn source(&self) -> MediaPad {
        MediaPad { desc: self.desc.source.clone() }
    }

    pub fn sink(&self) -> MediaPad {
        MediaPad { desc: self.desc.sink.clone() }
    }
}

use std::collections::HashMap;

use base_error::*;

const SETTINGS: &'static [CameraSettings] = &[
    CameraSettings {
        model_name: "ov9281",
        width: 1280,
        height: 800,
        subdev_format_code: v4l2::MEDIA_BUS_FMT_Y8_1X8,
        video_pixel_format: v4l2::V4L2_PIX_FMT_GREY
    },
    CameraSettings {
        model_name: "mira220",
        width: 1600,
        height: 1400,
        subdev_format_code: v4l2::MEDIA_BUS_FMT_SGRBG8_1X8,
        video_pixel_format: v4l2::V4L2_PIX_FMT_SGRBG8
    },
    CameraSettings {
        model_name: "ar0234",
        width: 1920,
        height: 1200,
        subdev_format_code: v4l2::MEDIA_BUS_FMT_Y8_1X8,
        video_pixel_format: v4l2::V4L2_PIX_FMT_GREY
    }
];


/// Direct camera access on Raspberry Pis that use the RP1 chip.
///
/// Currently this hardcodes configuring monochrome cameras at max resolution. 
pub struct RP1DirectCamera {
    pub model_name: String,

    /// Use this for configuring controls.
    pub camera_subdev: v4l2::SubDevice,

    pub capture_stream: v4l2::UnconfiguredStream,

    pub width: u32,

    pub height: u32,
}

struct CameraSettings {
    model_name: &'static str,
    width: u32,
    height: u32,
    subdev_format_code: u32,
    video_pixel_format: u32
}

impl RP1DirectCamera {

    pub async fn open() -> Result<Self> {
        let mut video_devs = {
            let mut out = HashMap::new();
            for dev in v4l2::Device::list().await? {
                out.insert(dev.device_num(), dev);
            }

            out
        };

        let mut sub_devs = {
            let mut out = HashMap::new();
            for dev in v4l2::SubDevice::list().await? {
                out.insert(dev.device_num(), dev);
            }

            out
        };

        let mut selected = None;
        for mut media_dev in v4l2::MediaDevice::list().await? {

            let entities = media_dev.list_entities()?;

            for entity in &entities {
                if entity.typ() == v4l2::MediaEntityType::V4L2_SUBDEV_SENSOR {
                    let entity_id = entity.id();
                    println!("Found camera entity '{}' (id {}) on '{}'", entity.name()?, entity_id, media_dev.path().as_str());
                    selected = Some((media_dev, entities, entity_id));
                    break;
                }
            }

            if selected.is_some() {
                break;
            }
        }

        let (mut media_dev, entities, camera_id) = selected.ok_or_else(|| err_msg("No camera sensor found"))?;

        let mut entities_by_id = HashMap::new();
        let mut entity_names = HashMap::new();
        for entity in entities {
            entity_names.insert(entity.name()?, entity.id());
            entities_by_id.insert(entity.id(), entity);
        }

        let camera_entity = entities_by_id.get(&camera_id).unwrap();

        let settings = {
            let name = camera_entity.name()?;

            let mut found_settings = None;
            for settings in SETTINGS {
                if name.contains(settings.model_name) {
                    found_settings = Some(settings);
                    break;
                }
            }

            found_settings.ok_or_else(|| err_msg("Unsupported camera"))?
        };



        let csi2_id = *entity_names.get("csi2").ok_or_else(|| err_msg("Failed to find the csi2 entity"))?;
        println!("CSI2 Entity Id: {}", csi2_id);

        let cfe_id = *entity_names.get("rp1-cfe-csi2_ch0").ok_or_else(|| err_msg("Failed to find the RP1 CFE entity"))?;
        println!("CFE ID: {}", cfe_id);

        // Reset all links
        for entity in entities_by_id.values() {
            println!("{}", entity.name()?);

            for link in entity.links() {
                if link.flags().contains(v4l2::MediaLinkFlags::Immutable) {
                    continue;
                }

                if link.flags().contains(v4l2::MediaLinkFlags::Enabled) {
                    println!("TODO: Disable enabled link!");;

                    // TODO: Disable me.
                }
            }
        }

        let camera_source_pad = 0;

        // NOTE: We are assuming that internally the CSI2 device wires up pad 0 to 4
        let csi2_sink_pad = 0;
        let csi2_source_pad = 4;

        // Verify camera pad to csi2:0 connections.
        {
            if camera_entity.pads().len() == 0 {
                return Err(err_msg("Expecting at least one camera pad"));
            }

            // Every pad on the camera needs to:
            // - Map to the next pad on the CSI2 device.
            // - Be a source
            // - Have an already enabled/immutable link (since we don't expect to need to enable them).
            for camera_pad_i in 0..camera_entity.pads().len() {
                let mut found_link = false;

                for l in camera_entity.links() {
                    if l.source().entity_id() != camera_id {
                        return Err(err_msg("Camera should only be the source in links"));
                    }

                    if l.sink().entity_id() != csi2_id {
                        return Err(err_msg("Camera linked to something other than the CSI2 device"));
                    }
                    
                    if l.source().index() != camera_pad_i ||
                        l.sink().index() != csi2_sink_pad + camera_pad_i {
                        continue;
                    }

                    if !l.flags().contains(v4l2::MediaLinkFlags::Immutable) || !l.flags().contains(v4l2::MediaLinkFlags::Enabled) {
                        return Err(err_msg("Expected camera link to be enabled/immutable"));
                    }
                    
                    found_link = true;
                    break;
                }

                if !found_link {
                    return Err(format_err!("Failed to find link between camera pad {} and CSI port", camera_pad_i));
                }
            }
        }

        // The CFE device should just have a single pad.
        let cfe_pad = 0;

        println!("Linking csi2 -> cfe...");
        let csi2_entity = entities_by_id.get(&csi2_id).unwrap();
        {
            let mut found = false; 
            for l in csi2_entity.links() {
                if l.source().entity_id() == csi2_id && l.source().index() == csi2_source_pad &&
                l.sink().entity_id() == cfe_id && l.sink().index() == cfe_pad {

                    media_dev.enable_link(l)?;
                    println!("=> Enabled");

                    found = true;
                    break;
                }
            }

            if !found {
                return Err(err_msg("Failed to find suitable link"));
            }
        }

        let cfe_entity = entities_by_id.get(&cfe_id).unwrap();

        // TODO: Remove the unwraps.
        let mut camera_subdev = sub_devs.remove(&camera_entity.device_num().unwrap())
            .ok_or_else(|| err_msg("Missing camera subdev"))?;
        let mut csi2_subdev = sub_devs.remove(&csi2_entity.device_num().unwrap())
            .ok_or_else(|| err_msg("Missing csi2 subdev"))?;
        let mut cfe_video = video_devs.remove(&cfe_entity.device_num().unwrap())
            .ok_or_else(|| err_msg("Missing video device"))?;

        println!("Configuring formats...");

        let subdev_format = {
            let mut fmt = v4l2::v4l2_subdev_format::default();
            fmt.format.width = settings.width;
            fmt.format.height = settings.height;
            fmt.format.code = settings.subdev_format_code;
            fmt.format.field = v4l2::v4l2_field::V4L2_FIELD_NONE.0;
            fmt
        };

        camera_subdev.set_format(camera_source_pad, &subdev_format).await?;

        // Copy all pad formats froom the camera to connected CSI2 pads.
        // Usually there will just be one video pad but there may be more which have metadata
        // (e.g. MEDIA_BUS_FMT_SENSOR_DATA)
        for i in 0..camera_entity.pads().len() {
            let fmt = camera_subdev.format(i).await?;
            csi2_subdev.set_format(csi2_sink_pad + i, &fmt).await?;
            csi2_subdev.set_format(csi2_source_pad + i, &fmt).await?;
        }

        let mut capture_stream = cfe_video.new_capture_stream()?;
        {
            let mut format = capture_stream.get_format().await?;

            format.set_width(settings.width);
            format.set_height(settings.height);
            format.set_pixelformat(settings.video_pixel_format);
            format.set_field(v4l2::v4l2_field::V4L2_FIELD_NONE.0);
            // format.set_colorspace(v4l2::v4l2_colorspace::V4L2_COLORSPACE_SRGB.0);
            // format.set_xfer_func(v4l2::v4l2_xfer_func::V4L2_XFER_FUNC_SRGB.0);

            format.set_num_planes(1);
            format.set_plane_format(0, {
                let mut f = v4l2::v4l2_plane_pix_format::default();
                f.bytesperline = settings.width;
                f.sizeimage = 0; 
                f
            });

            capture_stream.set_format(format).await?;
        }

        Ok(Self {
            model_name: settings.model_name.to_string(),
            camera_subdev,
            capture_stream,
            width: settings.width,
            height: settings.height
        })
    }
}

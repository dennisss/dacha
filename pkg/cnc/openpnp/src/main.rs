

/*

mkdir dump/ar0234-pnp

cargo run --bin openpnp -- \
    --board_path=pkg/media/camera/boards/camera_ar0234/r1/board-latest.kicad_pcb \
    --config_path=pkg/media/camera/boards/camera_ar0234/placement.txtpb \
    --output_dir=dump/ar0234-pnp


TODO: Tape advance pitch?
*/


#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::process::Command;
use std::io::Write;
use std::collections::HashSet;

use common::errors::*;
use kicad::library::*;
use kicad::reader::*;
use kicad::serializer::*;
use kicad::export::*;
use file::{LocalPath, LocalPathBuf};
use math::{
    geometry::bounding_box::{BoundingBoxBuilder, BoundingBox2},
    matrix::{vec2f, Matrix3f, MatrixXd},
};
use math_compute::io::CSVDataReader;
use openpnp_proto::openpnp::*;


#[derive(Debug, Clone)]
pub struct ComponentPosition {
    pub reference: String,
    pub value: String,
    pub package: String,
    pub pos_x: f64,
    pub pos_y: f64,
    pub rotation: f64,
    pub side: String,
}

impl ComponentPosition {
    pub async fn read_csv(path: &LocalPath) -> Result<Vec<ComponentPosition>> {
        let mut csv = CSVDataReader::create(path).await?;

        let mut out = vec![];

        while let Some(row) = csv.read().await? {
            out.push(Self {
                reference: row.str_field("Ref")?.to_string(),
                value: row.str_field("Val")?.to_string(),
                package: row.str_field("Package")?.to_string(),
                pos_x: row.f64_field("PosX")?,
                pos_y: row.f64_field("PosY")?,
                rotation: row.f64_field("Rot")?,
                side: row.str_field("Side")?.to_string(),
            });
        }

        Ok(out)
    }

}


#[derive(Args)]
struct Args {
    board_path: LocalPathBuf,
    config_path: LocalPathBuf,
    output_dir: LocalPathBuf
}

fn make_package(name: &str) -> String {
    format!(
        r#"
       <package version="1.1" id="{name}" pick-vacuum-level="0.0" place-blow-off-level="0.0">
            <footprint units="Millimeters" body-width="0.0" body-height="0.0" outer-dimension="0.0" inner-dimension="0.0" pad-count="0" pad-pitch="0.0" pad-across="0.0" pad-roundness="0.0"/>
            <compatible-nozzle-tip-ids class="java.util.ArrayList">
                <string>NT1</string>
            </compatible-nozzle-tip-ids>
        </package>
        "#,
        name = name
    )
}

fn make_part(proto: &Part) -> String {
    format!(
        r#"
        <part id="{name}" height-units="Millimeters" height="{height}" package-id="{package}" speed="1.0" pick-retry-count="0"/>
        "#,
        name = proto.name(),
        height = proto.height(),
        package = proto.board_ref().package()
    )
} 

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut config = PlacementConfig::default();
    let config_text = file::read_to_string(&args.config_path).await?;
    protobuf::text::parse_text_proto(&config_text, &mut config)?;

    let temp_dir = file::temp::TempDir::create()?;

    let export = KicadPCBExport::generate(&args.board_path, temp_dir.path())?;

    let bbox = {
        let data = gerber::read(
            &export.edge_cuts,
            gerber::CommandsProcessorOptions {
                min_feature_size: 0.001,
            },
        )
        .await?;

        let mut bbox_builder = BoundingBoxBuilder::new();

        for obj in data {
            // TODO: Support curves
            if let Some((start, end)) = obj.line {
                bbox_builder.update(&start);
                bbox_builder.update(&end);
            }
        }

        bbox_builder.build()
    };

    let board_width = bbox.max.x() - bbox.min.x();
    let board_height = bbox.max.y() - bbox.min.y();

    println!("{:?}", bbox);

    println!("Board Size: {} wide x {} high", board_width, board_height);


    let mut components = ComponentPosition::read_csv(&export.pos_file).await?;

    for component in &mut components {
        component.pos_x -= bbox.min.x() as f64;
        component.pos_y -= bbox.min.y() as f64;

    } 

    // println!("{:#?}", components);

    /*
    For all kicad packages, I will create an OpenPNP package with the same name.
    */
    let mut packages_xml = String::new();
    let mut packages_defined = HashSet::new();

    let mut parts_xml = String::new();
    let mut parts_defined = HashSet::new();

    let mut placements_xml = String::new();

    // TODO: Make sure that parts are using "BVS_Default"

    for component in components {

        let mut selected_part = None;
        for part in config.parts() {
            if part.board_ref().package() == component.package &&
                part.board_ref().value() == component.value {
                selected_part = Some(part);
                break;
            }
        }

        let part = match selected_part {
            Some(v) => v,
            None => continue
        };

        if packages_defined.insert(component.package.to_string()) {
            packages_xml.push_str(&make_package(&component.package));
        }

        if parts_defined.insert(part.name()) {
            parts_xml.push_str(&make_part(part));
        }

        let side = match component.side.as_str() {
            "top" => "Top",
            "bottom" => "Bottom",
            _ => return Err(format_err!("Unknown side: {}", component.side))
        };

        let typ = {
            if part.fiducial() {
                "Fiducial"
            } else {
                "Placement"
            }
        };

        placements_xml.push_str(&format!(
            r#"
            <placement version="1.4" side="{side}" id="{id}" part-id="{part_id}" type="{typ}" enabled="true">
                <location units="Millimeters" x="{x}" y="{y}" z="0.0" rotation="{rotation}"/>
                <error-handling>Alert</error-handling>
            </placement>            
            "#,
            x = component.pos_x,
            y = component.pos_y,
            rotation = component.rotation,
            side = side,
            id = component.reference,
            part_id = part.name(),
            typ = typ,
        ));

        // packages_xml.push

        // package: "Fiducial_1mm_Mask2mm"

    }

    println!("{}", packages_xml);

    println!("{}", parts_xml);


    file::write(
        args.output_dir.join("packages.xml"),
        format!(
            r#"
            <openpnp-packages>
                <!-- For LumenPnP Homing -->
                <package version="1.1" id="FIDUCIAL-1X2" pick-vacuum-level="0.0" place-blow-off-level="0.0">
                    <footprint units="Millimeters" body-width="0.0" body-height="0.0" outer-dimension="0.0" inner-dimension="0.0" pad-count="0" pad-pitch="0.0" pad-across="0.0" pad-roundness="0.0">
                        <pad name="1" x="0.0" y="0.0" width="1.0" height="1.0" rotation="0.0" roundness="100.0"/>
                    </footprint>
                    <compatible-nozzle-tip-ids class="java.util.ArrayList"/>
                </package>

                {}
            </openpnp-packages>
            "#,
            packages_xml,
        )
    ).await?;

    file::write(
        args.output_dir.join("parts.xml"),
        format!(
            r#"
            <openpnp-parts>
                <!-- For LumenPnP Homing -->
                <part id="FIDUCIAL-HOME" height-units="Millimeters" height="0.0" package-id="FIDUCIAL-1X2" speed="1.0" pick-retry-count="0"/>

                {}
            </openpnp-parts>
            "#,
            parts_xml,
        )
    ).await?;

    file::write(
        args.output_dir.join("generated.board.xml"),
        format!(
            r#"
            <openpnp-board version="1.1" name="generated.board.xml">
                <dimensions units="Millimeters" x="{width}" y="{height}" z="0.0" rotation="0.0"/>
                <placements>
                    {placements}
                </placements>
                <fiducials/>
                <solder-paste-pads/>
            </openpnp-board>
            "#,
            placements = placements_xml,
            height = board_height,
            width = board_width
        )
    ).await?;

    file::write(
        args.output_dir.join("boards.xml"),
        format!(
            r#"
            <openpnp-boards>
                <board>/home/dennis/.openpnp2/generated.board.xml</board>
            </openpnp-boards>
            "#
        )
    ).await?;

    Ok(())
}


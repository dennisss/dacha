use base_error::*;
use common::line_builder::LineBuilder;
use gerber::GraphicsObject;
use math::geometry::half_edge::HalfEdgeStruct;
use math::geometry::bounding_box::BoundingBox2;
use cam_proto::cnc::*;

use crate::isolation::FaceLabel;

const SVG_HEADER: &'static str = r#"
<?xml version="1.0" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg
    xmlns:svg="http://www.w3.org/2000/svg"
    xmlns="http://www.w3.org/2000/svg"
    xmlns:xlink="http://www.w3.org/1999/xlink"
    version="1.1"
    width="{width}mm" height="{height}mm" viewBox="0.0000 0.0000 {width} {height}"
>
<g style="fill: #000000; fill-opacity: 1; stroke:#000000; stroke-width: 0; stroke-opacity: 1; stroke-linecap: round; stroke-linejoin: round; fill-rule: evenodd;">
"#;

const SVG_PATH: &'static str = r#"
<path d="M {path}Z" />
"#;

const SVG_TRAILER: &'static str = "</g></svg>";

pub struct LaserStencilProcessorOptions {
    pub config: LaserStencilProcessorConfig,
    pub max_error: f32,
}

pub struct LaserStencilProcessor {
    options: LaserStencilProcessorOptions
}


impl LaserStencilProcessor {
    pub fn new(options: LaserStencilProcessorOptions) -> Self {
        Self { options }
    }

    pub fn process(&self, objects: &[GraphicsObject], bbox: &BoundingBox2) -> Result<String> {
        // TODO: Dedup this between the processors.
        let object_edges = {
            let mut half_edges = HalfEdgeStruct::<FaceLabel>::new();

            for obj in objects {
                for path in &obj.paths {
                    if let gerber::FillMode::Dark = path.fill {
                        //
                    } else {
                        // TODO:
                        println!("Non dark");
                        continue;
                    }

                    let (vertices, path_starts) = path.path.linearize(self.options.max_error);
                    for i in 0..(path_starts.len() - 1) {
                        let start_i = path_starts[i];
                        let end_i = path_starts[i + 1];
                        half_edges.add_face(FaceLabel::dark(), vertices[start_i..end_i].iter().cloned());
                    }
                }
            }

            half_edges.repair();
            half_edges.merge_faces();
            half_edges
        };

        let mut num_pads = 0;
        for face in object_edges.faces() {
            if face.label().dark {
                num_pads += 1;
            }
        }

        let mut pass_edges = math::geometry::offsetting::offset_faces(
            &object_edges,
            -self.options.config.laser_diameter() / 2.0,
            self.options.max_error,
        );
        pass_edges.merge_faces();

        let mut out = LineBuilder::new();

        // Note that we assume that we have shifted the zero point to the 'min' coordinate
        let width = bbox.max.x() - bbox.min.x();
        let height = bbox.max.y() - bbox.min.y();
        out.add(SVG_HEADER
            .replace("{width}", &format!("{:.4}", width))
            .replace("{height}", &format!("{:.4}", height))
            .trim()
        );

        let mut num_cut_pads = 0;

        for face in pass_edges.faces() {
            if /* !face.label().inbounds || */ !face.label().dark {
                continue;
            }

            let outer_component = match face.outer_component() {
                Some(v) => v,
                None => continue
            };

            num_cut_pads += 1;

            let mut path = String::new();

            for point in outer_component.points() {
                path.push_str(&format!("{:.4} {:.4} ", point.x(), point.y()));
            }

            out.add(SVG_PATH
                .replace("{path}", &path)
                .trim()
            );
        }

        out.add(SVG_TRAILER);

        if num_pads != num_cut_pads {
            // If this error occurs, then the laser diameter is too large.
            return Err(format_err!("Original gerber had {} pads but could only cut {}", num_pads, num_cut_pads));
        }

        Ok(out.to_string())
    }
}
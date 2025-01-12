use std::f32::consts::PI;

use base_error::*;
use cam_proto::cnc::*;
use common::line_builder::LineBuilder;
use file::LocalPathBuf;
use graphics::{
    canvas::{Path, PathBuilder},
    transforms::{scale2f, translate2f},
};
use math::{
    geometry::bounding_box::BoundingBoxBuilder,
    matrix::{vec2f, Matrix3f},
};

use crate::{
    cutout::{CutOutProcessor, CutOutProcessorOptions},
    drill::{DrillProcessor, DrillProcessorOptions},
    isolation::{IsolationRoutingProcessor, IsolationRoutingProcessorOptions},
};

pub struct PCBProcessorOptions {
    pub config: PCBProcessorConfig,

    // pub front_copper_path: Option<LocalPathBuf>,
    pub back_copper_path: Option<LocalPathBuf>,
    pub back_mask_path: Option<LocalPathBuf>,
    pub drill_path: Option<LocalPathBuf>,
    pub edge_cuts_path: Option<LocalPathBuf>,
    pub back_paste_path: Option<LocalPathBuf>,
    pub min_feature_size: f32,
}

/*
NOTE: All processors assume they are starting with a non-moving spindle and are in absolute positioning mode.
*/

pub async fn process_pcb(options: &PCBProcessorOptions) -> Result<String> {
    let mut edge_cuts = {
        if let Some(path) = &options.edge_cuts_path {
            gerber::read(
                path,
                gerber::CommandsProcessorOptions {
                    min_feature_size: options.min_feature_size,
                },
            )
            .await?
        } else {
            vec![]
        }
    };

    let mut back_copper = {
        if let Some(path) = &options.back_copper_path {
            gerber::read(
                path,
                gerber::CommandsProcessorOptions {
                    min_feature_size: options.min_feature_size,
                },
            )
            .await?
        } else {
            vec![]
        }
    };

    let mut back_mask = {
        if let Some(path) = &options.back_mask_path {
            gerber::read(
                path,
                gerber::CommandsProcessorOptions {
                    min_feature_size: options.min_feature_size,
                },
            )
            .await?
        } else {
            vec![]
        }
    };

    let mut back_paste = {
        if let Some(path) = &options.back_paste_path {
            gerber::read(
                path,
                gerber::CommandsProcessorOptions {
                    min_feature_size: options.min_feature_size,
                },
            )
            .await?
        } else {
            vec![]
        }
    };

    let mut drill_holes = {
        if let Some(path) = &options.drill_path {
            gerber::DrillFile::parse_excellon(&file::read(path).await?)?.holes
        } else {
            vec![]
        }
    };

    // Finding the path bounding box.
    // NOTE: This doesn't factor in the diameter of the cutting tools.
    let mut bbox_builder = BoundingBoxBuilder::new();

    // TODO: Also include the drill holes.
    for obj in edge_cuts
        .iter()
        .chain(back_copper.iter())
        .chain(back_mask.iter())
        .chain(back_paste.iter())
    {
        for path in &obj.paths {
            path.path.bbox_to(&mut bbox_builder);
        }
    }

    let bbox = bbox_builder.build();

    // Invert since cutting the back side.
    let transform = translate2f(vec2f(bbox.max.x() - bbox.min.x(), 0.0))
        * scale2f(&vec2f(-1.0, 1.0))
        * translate2f(bbox.min.clone() * -1.0);

    for obj in edge_cuts
        .iter_mut()
        .chain(back_copper.iter_mut())
        .chain(back_mask.iter_mut())
        .chain(back_paste.iter_mut())
    {
        obj.transform(&transform);
    }
    for hole in &mut drill_holes {
        hole.transform(&transform);
    }

    // Any hole that can't be drilled with one plunge will be drilled out in
    // multiple passes.
    {
        let mut i = 0;
        while i < drill_holes.len() {
            let hole = &drill_holes[i];

            // TODO: Base on the tool diameter
            if hole.diameter <= 0.81 {
                i += 1;
                continue;
            }

            let mut path_builder = PathBuilder::new();
            path_builder.ellipse(
                vec2f(hole.x, hole.y),
                vec2f(hole.diameter, hole.diameter) / 2.0,
                0.0,
                2.0 * PI,
            );
            path_builder.close();

            let obj = gerber::GraphicsObject {
                paths: vec![gerber::GraphicsPath {
                    path: path_builder.build(),
                    fill: gerber::FillMode::Dark,
                }],
                line: None,
            };

            edge_cuts.push(obj);

            drill_holes.swap_remove(i);
        }
    }

    let mut program = LineBuilder::new();

    program.add("G21 G40 G54");
    program.add("G80 G90 G94");

    /*
    {
        let isolation_processor =
            IsolationRoutingProcessor::new(IsolationRoutingProcessorOptions {
                config: options.config.paste_stencil().clone(),
                max_error: options.min_feature_size,
            });

        isolation_processor.process(&back_paste, &mut program)?;
    }

    return Ok(program.to_string());
    */

    {
        let isolation_processor =
            IsolationRoutingProcessor::new(IsolationRoutingProcessorOptions {
                config: options.config.isolation().clone(),
                max_error: options.min_feature_size,
            });

        isolation_processor.process(&back_copper, &mut program)?;
    }

    // Mark and wait for user to resume.
    // TODO: Use an absolute machine position for this.
    // program.add("G00 Y200");
    // TODO: Need to prevent the cnc_monitor bounding box estimator from counting
    // the park space
    program.add("M0");

    {
        let isolation_processor =
            IsolationRoutingProcessor::new(IsolationRoutingProcessorOptions {
                config: options.config.mask_removal().clone(),
                max_error: options.min_feature_size,
            });

        isolation_processor.process(&back_mask, &mut program)?;
    }

    if !drill_holes.is_empty() {
        let drill_processor = DrillProcessor::new(DrillProcessorOptions {
            config: options.config.drill().clone(),
        });

        drill_processor.process(&drill_holes, &mut program)?;
    }

    let cutout_processor = CutOutProcessor::new(CutOutProcessorOptions {
        config: options.config.cutout().clone(),
        max_error: options.min_feature_size,
    });

    cutout_processor.process(&edge_cuts, &mut program)?;

    Ok(program.to_string())
}

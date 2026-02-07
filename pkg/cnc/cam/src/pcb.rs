use std::f32::consts::PI;
use std::collections::{HashMap, HashSet};

use base_error::*;
use cam_proto::cnc::*;
use common::line_builder::LineBuilder;
use file::LocalPathBuf;
use graphics::{
    canvas::{Path, PathBuilder},
    transforms::{scale2f, translate2f, rotate2f},
};
use math::{
    geometry::bounding_box::{BoundingBoxBuilder, BoundingBox2},
    matrix::{vec2f, Matrix3f, MatrixXd},
};
use math::matrix::{Vector2f, Vector3f};

use crate::{
    cutout::*,
    drill::{DrillProcessor, DrillProcessorOptions},
    isolation::{IsolationRoutingProcessor, IsolationRoutingProcessorOptions},
};
use crate::stencil::*;
use crate::edge::*;

pub struct PCBProcessorOptions {
    pub config: PCBProcessorConfig,

    pub layers: Vec<PCBLayer>,

    pub add_alignment_holes: bool,
}

pub struct PCBLayer {
    pub path: LocalPathBuf,
    pub usage: PCBLayerUsage,
    pub side: PCBLayerSide
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PCBLayerUsage {
    Copper,
    Mask,
    EdgeCuts,
    Paste,
    Drill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PCBLayerSide {
    All,
    Back,
    Front,
}

/*
NOTE: All processors assume they are starting with a non-moving spindle and are in absolute positioning mode.
*/


/*

TODO: Kicad will exclude NPTHs from the copper layer unles

TODO: 



KiCad plot settings can be changed with "Do not tent vias" to not tent any vias though this tends to be too strict.

Input:
- List of riveted through hole sizes (ID and OD)

Instead we:

1. Look at all drill holes
2. Identify if it has a circle defined on the copper layer (indication that it is a via)
3. Check whether or not a circle exists on both solder mask layers
4. If tented and we want to rivet it, add more masking

Also dump the list of remaining tented holes which are candidates for alignment points.


No tenting settings:


Rivets are defined here:
- https://www.fortex.co.uk/product/favorit-through-hole-mechanical-plating/
- "0.6mm ID" Rivet 
    - 0.8mm OD / 0.9mm Drill size
    - 1.3mm head size (we will round up to 2mm)


    Safety of unplated through holes
    - copper around them must be connected to the same net but we can't guarantee that both sides will connect.

TODO: Disallow vias within SMT component footprints since we make the vias after placing SMT components (so can't reach below the component)

*/

pub struct PCBProcessor {
    options: PCBProcessorOptions,
    gerber_data: HashMap<(PCBLayerSide, PCBLayerUsage), Vec<gerber::GraphicsObject>>,
    drill_data: HashMap<(PCBLayerSide, PCBLayerUsage), Vec<gerber::DrillHole>>,
    
    /// These are edge/drill objects that should be cut on the first side of a double sided 
    precut_objects: HashSet<((PCBLayerSide, PCBLayerUsage), usize)>,

    bbox: BoundingBox2,
}

struct Circle {
    object_index: usize,
    center: Vector2f,
    diameter: f32
}

#[derive(Debug)]
struct HoleData {
    center: Vector2f,

    /// Diameters of this hole on each connected layer (and the index of the object)
    circles: HashMap<(PCBLayerSide, PCBLayerUsage), (f32, usize)>,
}


impl PCBProcessor {
    pub async fn create(options: PCBProcessorOptions) -> Result<Self> {

        let mut gerber_data = HashMap::new();
        let mut drill_data = HashMap::new();

        for layer in options.layers.iter() {
            let key = (layer.side, layer.usage);

            match layer.usage {
                PCBLayerUsage::Copper | PCBLayerUsage::Mask | PCBLayerUsage::EdgeCuts | PCBLayerUsage::Paste => {
                    let data = gerber::read(
                        &layer.path,
                        gerber::CommandsProcessorOptions {
                            min_feature_size: options.config.min_feature_size(),
                        },
                    )
                    .await?;

                    // TODO: Merge layers.
                    gerber_data.insert(key, data);
                }
                PCBLayerUsage::Drill => {
                    drill_data.insert(key, gerber::DrillFile::parse_excellon(&file::read(&layer.path).await?)?.holes);
                }
            }
        }


        // Finding the path bounding box.
        // NOTE: This doesn't factor in the diameter of the cutting tools.
        let mut bbox_builder = BoundingBoxBuilder::new();

        // TODO: Also include the drill holes.
        for obj in gerber_data.values().map(|v| v.iter()).flatten() {
            for path in &obj.paths {
                path.path.bbox_to(&mut bbox_builder);
            }
        }

        let bbox = bbox_builder.build();

        let mut inst = PCBProcessor {
            options,
            gerber_data,
            drill_data,
            precut_objects: HashSet::new(),
            bbox,
        };

        // TODO: THis doesn't seem to work correctly.
        /*
        let width = inst.bbox.max.x() - inst.bbox.min.x();
        let height = inst.bbox.max.y() - inst.bbox.min.y();
        if height > width {
            println!("Vertical PCB: Rotating 90 degrees...");
            inst.transform_objects(&rotate2f(PI / 2.0)); // 90 degrees.

            // Recompute bbox.
            inst.bbox = {
                let mut bbox_builder = BoundingBoxBuilder::new();

                // TODO: Also include the drill holes.
                for obj in inst.gerber_data.values().map(|v| v.iter()).flatten() {
                    for path in &obj.paths {
                        path.path.bbox_to(&mut bbox_builder);
                    }
                }

                bbox_builder.build()
            };

            println!("BBOX: {:?}", inst.bbox);
        }
        */

        inst.apply_forced_hole_diameter();
        inst.convert_drills_to_edge_cuts()?;
        inst.transform_vias();

        Ok(inst)
    }

    fn apply_forced_hole_diameter(&mut self) {
        if self.options.config.forced_hole_diameter() != 0.0 {
            for hole in self.drill_data.values_mut().map(|v| v.iter_mut()).flatten() {
                hole.diameter = self.options.config.forced_hole_diameter();
            }
        }
    }

    // Any hole that can't be drilled with one plunge will be drilled out in
    // multiple passes.
    fn convert_drills_to_edge_cuts(&mut self) -> Result<()> {
        let mut edge_cuts = self.gerber_data
            .entry((PCBLayerSide::All, PCBLayerUsage::EdgeCuts))
            .or_default();

        let mut drill_holes = self.drill_data
            .entry((PCBLayerSide::All, PCBLayerUsage::Drill))
            .or_default();

        let mut i = 0;
        while i < drill_holes.len() {
            let hole = &drill_holes[i];

            let mut found_drill = false;
            for tool in self.options.config.drill().tools() {
                // TODO: Make this the same check as in the drill processor.
                if (tool.tool_diameter() - hole.diameter).abs() <= 0.01 {
                    found_drill = true;
                    break;
                }
            }

            if found_drill {
                println!("Drill: {}", hole.diameter);
                i += 1;
                continue;
            }

            println!("Edge Cut: {}", hole.diameter);

            // offset must be much larger than the min feature size to get a reliable cutout.
            if hole.diameter < self.options.config.cutout().tool_diameter() + 0.1 {
                return Err(format_err!("Can't cut hole of diameter: {}", hole.diameter));
            }

            /*
            // TODO: What does this mean.
            if hole.diameter <= (self.options.config.drill().tool_diameter() + 0.01) {
                if (hole.diameter - self.options.config.drill().tool_diameter()).abs() > 0.05 {
                    println!("No ideal drill for hole diameter: {}", hole.diameter);
                }

                i += 1;
                continue;
            }
            */

            // TODO: It would probably be ideal to just 
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
                attributes: HashMap::new(),
            };

            edge_cuts.push(obj);

            drill_holes.swap_remove(i);
        }

        Ok(())
    }

    // Via analysis
    //
    // - We define a via 
    fn transform_vias(&mut self) {

        let mut holes = vec![];

        for (key, data) in self.drill_data.iter() {
            for (i, hole) in data.iter().enumerate() {
                let mut circles = HashMap::new();
                circles.insert(key.clone(), (hole.diameter, i));
                
                holes.push(HoleData {
                    center: vec2f(hole.x, hole.y),
                    circles
                });
            }
        }

        for (key, data) in self.gerber_data.iter() {
            let circles = Self::find_circles_on_layer(data);

            for circle in circles {

                let hole = match holes.iter_mut()
                    .find(|hole| (&hole.center - &circle.center).norm_squared() < 0.001) {
                    Some(v) => v,
                    None => {
                        holes.push(HoleData {
                            center: circle.center.clone(),
                            circles: HashMap::new()
                        });
                        holes.last_mut().unwrap()
                    }
                };

                hole.circles.insert(key.clone(), (circle.diameter, circle.object_index));
            }
        }

        let mut candidate_alignment_holes = vec![];

        // TODO: Make this more customizable.
        let via_sizes: Vec<(f32, f32)> = vec![
            (1.1, 2.0),
            (0.9, 1.8),
            (0.9, 2.0),

            // For freestanding vias only
            // (0.9, 2)
        ];

        // println!("{:#?}", holes);

        for hole in &holes {

            let front_copper_diameter = match hole.circles.get(&(PCBLayerSide::Front, PCBLayerUsage::Copper)) {
                Some(v) => v.0,
                None => continue
            };

            let back_copper_diameter = match hole.circles.get(&(PCBLayerSide::Back, PCBLayerUsage::Copper)) {
                Some(v) => v.0,
                None => continue
            };

            // TODO: Verify the hole isn't on both layers.
            let (drill_diameter, drill_key) = match hole.circles.get(&(PCBLayerSide::All, PCBLayerUsage::Drill)) {
                Some(v) => (v.0, ((PCBLayerSide::All, PCBLayerUsage::Drill), v.1)),
                None => {
                    match hole.circles.get(&(PCBLayerSide::All, PCBLayerUsage::EdgeCuts)) {
                        Some(v) => (v.0, ((PCBLayerSide::All, PCBLayerUsage::EdgeCuts), v.1)),
                        None => continue
                    }    
                }
            };

            if (back_copper_diameter - front_copper_diameter).abs() > 0.001 {
                eprintln!("Front and back copper diameters of via are inconsistent");
            }

            if hole.circles.contains_key(&(PCBLayerSide::Front, PCBLayerUsage::Mask)) ||
                hole.circles.contains_key(&(PCBLayerSide::Back, PCBLayerUsage::Mask)) {
                continue;
            }



            println!("Tented Via: {}, {} | {} by {}", hole.center.x(), hole.center.y(), drill_diameter, front_copper_diameter);
            
            let mut should_untent = false;
            for size in via_sizes.iter().cloned() {
                if (drill_diameter - size.0).abs() <= 0.001 &&
                   (front_copper_diameter - size.1).abs() <= 0.001 {
                    should_untent = true;
                    break;
                }
            }

            if should_untent {
                println!("=> Untenting");

                let mut path_builder = PathBuilder::new();
                path_builder.ellipse(
                    hole.center.clone(),
                    vec2f(front_copper_diameter, front_copper_diameter) / 2.0,
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
                    attributes: HashMap::new(),
                };

                self.gerber_data
                    .entry((PCBLayerSide::Front, PCBLayerUsage::Mask))
                    .or_default()
                    .push(obj.clone());

                self.gerber_data
                    .entry((PCBLayerSide::Back, PCBLayerUsage::Mask))
                    .or_default()
                    .push(obj);

            } else {
                candidate_alignment_holes.push(drill_key);

                // TODO: Verify that the dirll size is smaller than hole size in this case.
                // (we also don't want to double drill these to avoid hole alignment issues)

                //

                // These will be drilled first as alignment points (will need to )
            }

            // We will auto-untent 
        }


        if self.options.add_alignment_holes {
            if candidate_alignment_holes.len() >= 4 {
                println!("Using existing holes in the board as alignment holes");
                self.precut_objects.extend(candidate_alignment_holes.into_iter());
            } else {
                self.add_alignment_holes();
            }
        }
    }

    fn add_alignment_holes(&mut self) {
        println!("Making new alignment holes");

        // TODO: Make long/short size dynamic based on which side is smallest (also allow the user to specify a PCB blank size that we can evaluate which side to put it on is easiest). Currently we just assume that the x direction edge is the long size.

        // TODO: Support holes using edge cuts if 'hole_size' is not the same as the drill size.

        // TODO: Verify that the board is long enough to accomadate the alignment holes without them colliding with each other.

        let config = self.options.config.alignment_holes();
        let points = vec![
            ("Top Left", self.bbox.min.x() + config.short_edge_offset(), self.bbox.max.y() + config.long_edge_offset()),
            ("Bottom Left", self.bbox.min.x() + config.short_edge_offset(), self.bbox.min.y() - config.long_edge_offset()),
            ("Top Right", self.bbox.max.x() - config.short_edge_offset(), self.bbox.max.y() + config.long_edge_offset()),
            ("Bottom Right", self.bbox.max.x() - config.short_edge_asymetric_offset(), self.bbox.min.y() - config.long_edge_offset()),
        ];
        

        let mut drill_holes = self.drill_data
            .entry((PCBLayerSide::All, PCBLayerUsage::Drill))
            .or_default();

        for (name, x, y) in points {

            println!("Alignment hole: {} : x: {}, {}", name, x, y);

            let i = drill_holes.len();

            drill_holes.push(gerber::DrillHole {
                x, y, diameter: config.hole_size(),
            });

            self.precut_objects.insert(((PCBLayerSide::All, PCBLayerUsage::Drill), i));
        }
    }

    fn find_circles_on_layer(data: &[gerber::GraphicsObject]) -> Vec<Circle> {
        let mut out = vec![];

        for (i, cmd) in data.iter().enumerate() {
            // Skip over traces
            if cmd.line.is_some() {
                continue;
            }

            // We expect vias to drawn with separate dark circles.
            // TODO: Also check fill mode is 'dark'.
            if cmd.paths.len() != 1 ||
                cmd.paths[0].path.sub_paths().len() != 1 ||
                cmd.paths[0].path.sub_paths()[0].segments.len() != 1 {
                continue;
            }

            let ellipse = match &cmd.paths[0].path.sub_paths()[0].segments[0] {
                graphics::canvas::PathSegment::Ellipse(e) => e,
                _ => continue
            };

            // TODO: Use approximate comparison.
            if ellipse.x_axis.y() != 0.0 || ellipse.y_axis.x() != 0.0 || ellipse.x_axis.x() != ellipse.y_axis.y() {
                continue;
            }

            out.push(Circle {
                center: ellipse.center.clone(),
                diameter: ellipse.x_axis.x() * 2.0,
                object_index: i,
            });
        }

        out
    }

    fn find_gerber_layer(&self, side: PCBLayerSide, usage: PCBLayerUsage) -> Option<&[gerber::GraphicsObject]> {
        if let Some(data) = self.gerber_data.get(&(side, usage)) {
            if data.is_empty() {
                return None;
            }

            return Some(data);
        }

        None
    }

    fn find_drill_layer(&self) -> Option<&[gerber::DrillHole]> {
        if let Some(data) = self.drill_data.get(&(PCBLayerSide::All, PCBLayerUsage::Drill)) {
            if data.is_empty() {
                return None;
            }

            return Some(data);
        }

        None
    }

    fn single_side_transform(&mut self, side: PCBLayerSide) -> Result<()> {
        let transform = {
            match side {
                PCBLayerSide::All => {
                    return Err(err_msg("All is not a single side"));
                }
                PCBLayerSide::Front => {
                    translate2f(vec2f(-self.bbox.min.x(), -self.bbox.min.y()))
                }
                PCBLayerSide::Back => {
                    // Invert since cutting the back side.
                    translate2f(vec2f(self.bbox.max.x() - self.bbox.min.x(), 0.0))
                    * scale2f(&vec2f(-1.0, 1.0))
                    * translate2f(self.bbox.min.clone() * -1.0)
                }
            }
        };

        self.transform_objects(&transform);

        Ok(())
    }

    fn transform_objects(&mut self, transform: &Matrix3f) {
        for layer in self.gerber_data.values_mut() {
            for obj in layer.iter_mut() {
                obj.transform(transform);
            }
        }

        for layer in self.drill_data.values_mut() {
            for hole in layer.iter_mut() {
                hole.transform(transform);
            }
        }
    }

    pub fn build_single_side_program(mut self, side: PCBLayerSide) -> Result<String> {
        self.single_side_transform(side)?;
        self.single_side_program_impl(side, false)
    }

    /*
    double sided front:
    - Copper Isolation
    - Mask Isolation
    - Through Cut Drill / Cut Out
        - On the second side we don't need to go all the way.
    - Partial Cut Drill / Cut Out

    */


    fn single_side_program_impl(mut self, side: PCBLayerSide, part_of_double_sided: bool) -> Result<String> {

        let edge_layer = self.find_gerber_layer(PCBLayerSide::All, PCBLayerUsage::EdgeCuts)
            .ok_or_else(|| err_msg("Board is missing edge cuts"))?;

        let edge_metadata = EdgeCutMetadata::create(&edge_layer)?;

        let mut program = LineBuilder::new();

        program.add("G21 G40 G54");
        program.add("G80 G90 G94");

        if let Some(layer) = self.find_gerber_layer(side, PCBLayerUsage::Copper) {
            println!("Copper...");

            program.nl();
            program.add("; Copper Isolation Routing");

            let isolation_processor =
                IsolationRoutingProcessor::new(IsolationRoutingProcessorOptions {
                    config: self.options.config.isolation().clone(),
                    arc_config: self.options.config.arc_builder().clone(),
                    max_error: self.options.config.min_feature_size(),
                    mark_edge: true
                });

            isolation_processor.process(layer, &edge_metadata, &mut program)?;
        }

        if let Some(layer) = self.find_gerber_layer(side, PCBLayerUsage::Mask) {
            println!("Mask...");
            
            program.nl();
            program.add("; Mask Isolation Routing");

            // Mark and wait for user to resume.
            // TODO: Use an absolute machine position for this.
            // program.add("G00 Y200");
            // TODO: Need to prevent the cnc_monitor bounding box estimator from counting
            // the park space
            program.add("M0");

            let isolation_processor =
                IsolationRoutingProcessor::new(IsolationRoutingProcessorOptions {
                    config: self.options.config.mask_removal().clone(),
                    arc_config: self.options.config.arc_builder().clone(),
                    max_error: self.options.config.min_feature_size(),
                    mark_edge: false,
                });

            isolation_processor.process(layer, &edge_metadata, &mut program)?;

            // User check that mask is well removed.
            program.add("M0");
        }

        if let Some(layer) = self.find_drill_layer() {
            println!("Drill...");

            if part_of_double_sided {

                let mut through_indexes = HashSet::new();
                 for (key, index) in &self.precut_objects {
                    if *key == (PCBLayerSide::All, PCBLayerUsage::Drill) {
                        through_indexes.insert(*index);
                    }
                }

                let mut through_objects: Vec<gerber::DrillHole> = vec![];
                let mut partial_objects: Vec<gerber::DrillHole> = vec![];
                for (i, obj) in layer.iter().enumerate() {
                    if through_indexes.contains(&i) {
                        through_objects.push(obj.clone());
                    } else {
                        partial_objects.push(obj.clone());
                    }
                }

                let through_depth = match side {
                    PCBLayerSide::Front => self.options.config.double_sided().front_through_cut_depth(),
                    PCBLayerSide::Back => self.options.config.double_sided().back_through_cut_depth(),
                    _ => todo!()
                };
                let partial_depth = match side {
                    PCBLayerSide::Front => self.options.config.double_sided().front_partial_cut_depth(),
                    PCBLayerSide::Back => self.options.config.double_sided().back_partial_cut_depth(),
                    _ => todo!()
                };

                if through_depth != 0.0 {
                    program.nl();
                    program.add("; Drill Through");

                    let mut config = self.options.config.drill().clone();
                    config.set_drill_z(-through_depth);
                    let drill_processor = DrillProcessor::new(DrillProcessorOptions { config });

                    drill_processor.process(&through_objects[..], &mut program)?;
                }
                if partial_depth != 0.0 {
                    program.nl();
                    program.add("; Drill Partial");

                    let mut config = self.options.config.drill().clone();
                    config.set_drill_z(-partial_depth);
                    let drill_processor = DrillProcessor::new(DrillProcessorOptions { config });

                    drill_processor.process(&partial_objects[..], &mut program)?;
                }

            } else {
                program.nl();
                program.add("; Drill");

                // Drill everything
                let drill_processor = DrillProcessor::new(DrillProcessorOptions {
                    config: self.options.config.drill().clone(),
                });
                drill_processor.process(layer, &mut program)?;
            }
        }

        {
            println!("Cutout...");

            if part_of_double_sided {
                // Note that we can sand down precuts on the second side but this is more annoying to do for other types of cuts.
                
                let mut through_indexes = HashSet::new();
                 for (key, index) in &self.precut_objects {
                    if *key == (PCBLayerSide::All, PCBLayerUsage::EdgeCuts) {
                        through_indexes.insert(*index);
                    }
                }

                let mut through_edge_meta = EdgeCutMetadata {
                    outer_edge_path: edge_metadata.outer_edge_path.clone(),
                    outer_edge_objects: HashSet::new(),
                    inner_edge_objects: HashSet::new()
                };

                let mut partial_edge_meta = EdgeCutMetadata {
                    outer_edge_path: edge_metadata.outer_edge_path.clone(),
                    outer_edge_objects: edge_metadata.outer_edge_objects.clone(),
                    inner_edge_objects: HashSet::new()
                };

                for index in &edge_metadata.inner_edge_objects {
                    if through_indexes.contains(index) {
                        through_edge_meta.inner_edge_objects.insert(*index);
                    } else {
                        partial_edge_meta.inner_edge_objects.insert(*index);
                    }
                }


                let through_depth = match side {
                    PCBLayerSide::Front => self.options.config.double_sided().front_through_cut_depth(),
                    PCBLayerSide::Back => self.options.config.double_sided().back_through_cut_depth(),
                    _ => todo!()
                };
                let partial_depth = match side {
                    PCBLayerSide::Front => self.options.config.double_sided().front_partial_cut_depth(),
                    PCBLayerSide::Back => self.options.config.double_sided().back_partial_cut_depth(),
                    _ => todo!()
                };

                if through_depth != 0.0 {
                    program.nl();
                    program.add("; Edge Cut Through");

                   let mut config = self.options.config.cutout().clone();
                   config.set_cut_depth_z(through_depth);
                  
                    let cutout_processor = CutOutProcessor::new(CutOutProcessorOptions {
                        config: config,
                        max_error: self.options.config.min_feature_size(),
                        arc_config: self.options.config.arc_builder().clone(),
                    });

                    cutout_processor.process(edge_layer, &through_edge_meta, &mut program)?;
                }
                if partial_depth != 0.0 {
                    program.nl();
                    program.add("; Edge Cut Partial");

                    let mut config = self.options.config.cutout().clone();
                   config.set_cut_depth_z(partial_depth);
                  
                    let cutout_processor = CutOutProcessor::new(CutOutProcessorOptions {
                        config: config,
                        max_error: self.options.config.min_feature_size(),
                        arc_config: self.options.config.arc_builder().clone(),
                    });

                    cutout_processor.process(edge_layer, &partial_edge_meta, &mut program)?;
                }


            } else {
                // Cut everything

                program.nl();
                program.add("; Edge Cut");

                let cutout_processor = CutOutProcessor::new(CutOutProcessorOptions {
                    config: self.options.config.cutout().clone(),
                    max_error: self.options.config.min_feature_size(),
                    arc_config: self.options.config.arc_builder().clone(),
                });

                cutout_processor.process(edge_layer, &edge_metadata, &mut program)?;
            }
        }

        Ok(program.to_string())
    }

    pub fn build_stencil_program(mut self, side: PCBLayerSide) -> Result<String> {
        self.single_side_transform(side)?;

        let mut program = LineBuilder::new();

        program.add("G21 G40 G54");
        program.add("G80 G90 G94");


        let edge_layer = self.find_gerber_layer(PCBLayerSide::All, PCBLayerUsage::EdgeCuts)
            .ok_or_else(|| err_msg("Board is missing edge cuts"))?;

        // TODO: Get rid of the requirement to pass this in.
        let edge_metadata = EdgeCutMetadata::create(&edge_layer)?;

        let layer = self.find_gerber_layer(side, PCBLayerUsage::Paste)
            .ok_or_else(|| err_msg("Missing stencil layer"))?;


        {
            let isolation_processor =
                IsolationRoutingProcessor::new(IsolationRoutingProcessorOptions {
                    config: self.options.config.paste_stencil().clone(),
                    arc_config: self.options.config.arc_builder().clone(),
                    max_error: self.options.config.min_feature_size(),
                    mark_edge: false,
                });

            isolation_processor.process(layer, &edge_metadata, &mut program)?;
        }


        Ok(program.to_string())

    }

    pub fn build_double_sided_front_program(mut self) -> Result<String> {
        self.single_side_transform(PCBLayerSide::Front)?;
        self.single_side_program_impl(PCBLayerSide::Front, true)
    }

    pub fn build_double_sided_back_program(mut self, alignment_data: &SideAlignmentData) -> Result<String> {        
        let transform = {
            let n = alignment_data.mappings().len();
            if n < 3 {
                return Err(err_msg("Must have at least 3 aligned points to calibrate front and back sides"));
            }

            // Solving 'A x = y' where:
            // - A is the 2x3 transform matrix.
            // - 'x' has the x/y/1 coordinates of points in the old coordinate space
            // - 'y' has the x/y coordinates of points in the new coordinate space

            let mut x = MatrixXd::zero_with_shape(3, n);
            let mut y = MatrixXd::zero_with_shape(2, n);

            // front_measurement

            for (i, p) in alignment_data.mappings().iter().enumerate() {
                if p.front_measurement().len() != 2 || p.back_measurement().len() != 2 {
                    return Err(err_msg("Expected 2d points for both sided measurements"));
                }

                // TODO: Complain about redundant points

                // TODO: First do check alignment of p.point to p.front_measurement (detect weird warps and warn if there is unexpected skew)

                // TODO: Verify that the p.points correspond to actual points that we drilled or cut out 

                x[(0, i)] = (p.point()[0] - self.bbox.min.x()) as f64; // p.front_measurement()[0] as f64;
                x[(1, i)] = (p.point()[1] - self.bbox.min.y()) as f64; // p.front_measurement()[1] as f64;
                x[(2, i)] = 1.0;

                y[(0, i)] = x[(0, i)] + (p.back_measurement()[0] as f64) - (p.front_measurement()[0] as f64);
                y[(1, i)] = x[(1, i)] + (p.back_measurement()[1] as f64) - (p.front_measurement()[1] as f64);
                // y[(1, i)] = p.back_measurement()[1] as f64;
            }

            // TODO: Must check for invertability and verify we get a resonable mapping 
            let pinv_x = math::matrix::pinv(&x);

            // This will be a 2x3 matrix.
            let a = y * pinv_x;

            println!("Transform:\n{:?}", a);

            let a = a.cast::<f32>();

            let mut out = Matrix3f::identity();
            out.block_with_shape_mut(0, 0, 2, 3).copy_from(&a);
            out
        };

        self.single_side_transform(PCBLayerSide::Front)?;
        self.transform_objects(&transform);

        self.single_side_program_impl(PCBLayerSide::Back, true)
    }

    pub fn build_laser_stencil_program(mut self, side: PCBLayerSide) -> Result<String> {
        self.single_side_transform(side)?;

        let layer = self.find_gerber_layer(side, PCBLayerUsage::Paste)
            .ok_or_else(|| err_msg("Missing stencil layer"))?;

        let processor = LaserStencilProcessor::new(LaserStencilProcessorOptions {
            config: self.options.config.laser_stencil().clone(),
            max_error: self.options.config.min_feature_size(),
        });

        processor.process(&layer, &self.bbox)
    }
}

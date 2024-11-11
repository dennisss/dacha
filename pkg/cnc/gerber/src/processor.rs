use std::{collections::HashMap, f32::consts::PI};

use base_error::*;
use graphics::{canvas::PathBuilder, raster::stroke::stroke_poly};
use math::{geometry::line::Line2, matrix::Vector2f};

use crate::{
    expression::ExpressionEvaluator,
    graphics::{FillMode, GraphicsObject},
    syntax::*,
};

/// Gerber file prefix that is processed before any user commands are processed.
/// This is used to define standard built-in templates.
///
/// - 'C' macro: Params are diameter and hole_diameter for a circle.
/// - 'R' macro: Params are x_size, y_size, hole_diamater for a rectangle.
/// - 'O' macro: Params are x_size, y_size, hole_diameter
///     - MUST be called with 'y_size >= x_size'
/// - 'OX'
///     - MUST be called with 'x_size >= y_ssize'
/// - 'L' macro: Params are width, start_x, start_y, end_x, end_y
const PREAMBLE: &'static str = "
%AMC*
0 Main*
1,1,$1,0,0*
0 Hole*
1,0,$2,0,0*
%

%AMR*
0 Main*
21,1,$1,$2,0,0,0*
0 Hole*
1,0,$3,0,0*
%

%AMO*
0 Start circle*
1,1,$1,0,-($2-$1)/2*
0 End circle*
1,1,$1,0,($2-$1)/2*
0 Main Body*
21,1,$1,$2-$1,0,0,0*
0 Hole*
1,0,$3,0,0*
%

%AMOX*
0 Start circle*
1,1,$2,-($1-$2)/2,0*
0 End circle*
1,1,$2,($1-$2)/2,0*
0 Main Body*
21,1,$1-$2,$2,0,0,0*
0 Hole*
1,0,$3,0,0*
%

%AML*
20,1,$1,$2,$3,$4,$5,0*
0 Start circle*
1,1,$1,$2,$3,0*
0 End circle*
1,1,$1,$4,$5,0*
%
";

pub struct CommandsProcessorOptions {
    /// Minimum width or height of a drawn object.
    pub min_feature_size: f32,
}

pub struct CommandsProcessor {
    options: CommandsProcessorOptions,
    graphics_state: GraphicsState,
    aperture_templates: HashMap<String, ApertureMacro>,
    aperture_definition: HashMap<String, ApertureDefinition>,
}

#[derive(Default)]
struct GraphicsState {
    coordinate_scale: Option<f64>,
    unit: Option<Mode>,
    current_point_x: Option<f64>,
    current_point_y: Option<f64>,
    current_aperture: Option<String>,
    plot_state: Option<PlotState>,
    polarity: Polarity,
    mirroring: Mirroring,
    rotation: Rotation,
    scaling: Scaling,
}

impl CommandsProcessor {
    pub fn create(options: CommandsProcessorOptions) -> Result<Self> {
        let mut inst = Self {
            options,
            graphics_state: GraphicsState::default(),
            aperture_definition: HashMap::default(),
            aperture_templates: HashMap::default(),
        };

        let preamble = File::parse(PREAMBLE.as_bytes())?;

        let mut tmp = vec![];
        for cmd in preamble.commands {
            inst.process(&cmd, &mut tmp)?;
        }

        Ok(inst)
    }

    pub fn process(&mut self, command: &Command, out: &mut Vec<GraphicsObject>) -> Result<()> {
        match command {
            Command::FormatSpecifier(v) => {
                if self.graphics_state.coordinate_scale.is_some() {
                    return Err(err_msg("FS specified multiple times"));
                }

                if v.x_digits != v.y_digits {
                    return Err(err_msg("X and Y format specifier must be the same"));
                }

                let num_decimal_digits = v.x_digits % 10;
                let scale = 1.0 / 10.0f64.powi(num_decimal_digits as i32);

                self.graphics_state.coordinate_scale = Some(scale);
            }
            Command::Mode(v) => {
                if self.graphics_state.unit.is_some() {
                    return Err(err_msg("Unit specified multiple times"));
                }

                self.graphics_state.unit = Some(v.clone());
            }
            Command::SetPlotState(v) => {
                self.graphics_state.plot_state = Some(v.clone());
            }
            Command::LoadPolarity(v) => {
                self.graphics_state.polarity = v.clone();
            }
            Command::LoadMirroring(v) => {
                return Err(err_msg("Mirroring not supported"));

                self.graphics_state.mirroring = v.clone();
            }
            Command::LoadRotation(v) => {
                return Err(err_msg("Rotation not supported"));

                self.graphics_state.rotation = v.clone();
            }
            Command::LoadScaling(v) => {
                return Err(err_msg("Scaling not supported"));

                self.graphics_state.scaling = v.clone();
            }

            Command::ApertureMacro(v) => {
                if self.aperture_templates.contains_key(&v.name) {
                    return Err(format_err!("Duplicate macro defined with name: {}", v.name));
                }

                self.aperture_templates.insert(v.name.clone(), v.clone());
            }

            Command::ApertureDefinition(v) => {
                // TODO: Check no duplicates.
                self.aperture_definition.insert(v.id.clone(), v.clone());
            }

            Command::SetCurrentAperture(v) => {
                self.graphics_state.current_aperture = Some(v.clone());
            }

            Command::Move(v) => {
                let scale = self
                    .graphics_state
                    .coordinate_scale
                    .ok_or_else(|| err_msg("No format specifier defined yet"))?;

                if let Some(x) = v.x {
                    self.graphics_state.current_point_x = Some((x as f64) * scale);
                }

                if let Some(y) = v.y {
                    self.graphics_state.current_point_y = Some((y as f64) * scale);
                }
            }

            Command::Plot(cmd) => {
                let start_x = self
                    .graphics_state
                    .current_point_x
                    .ok_or_else(|| err_msg("Current X undefined"))?;
                let start_y = self
                    .graphics_state
                    .current_point_y
                    .ok_or_else(|| err_msg("Current Y undefined"))?;

                let scale = self
                    .graphics_state
                    .coordinate_scale
                    .ok_or_else(|| err_msg("No format specifier defined yet"))?;

                let end_x = match cmd.x {
                    Some(x) => (x as f64) * scale,
                    None => start_x,
                };

                let end_y = match cmd.y {
                    Some(y) => (y as f64) * scale,
                    None => start_y,
                };

                let current_aperture = self.get_current_aperture()?;

                let circle = match &current_aperture.shape {
                    ApertureShape::Circle(c) => c,
                    _ => return Err(err_msg("Only circle apertures supported in plot")),
                };

                if circle.hole_diameter.is_some() {
                    return Err(err_msg("Can not plot a circle with a hole"));
                }

                if self.graphics_state.plot_state != Some(PlotState::Linear) {
                    return Err(err_msg("Only lines are supported"));
                }

                // TODO: Implement arc support.

                // NOTE: We don't need any pre-processing for translating this macro.
                self.draw_aperture(
                    &ApertureDefinition {
                        id: "".to_string(),
                        shape: ApertureShape::TemplateCall(TemplateCall {
                            name: "L".to_string(),
                            params: vec![circle.diameter, start_x, start_y, end_x, end_y],
                        }),
                    },
                    out,
                )?;

                self.graphics_state.current_point_x = Some(end_x);
                self.graphics_state.current_point_y = Some(end_y);
            }

            Command::Flash(cmd) => {
                let current_aperture = self.get_current_aperture()?;

                let scale = self
                    .graphics_state
                    .coordinate_scale
                    .ok_or_else(|| err_msg("No format specifier defined yet"))?;

                let x = match cmd.x {
                    Some(v) => (v as f64) * scale,
                    None => self
                        .graphics_state
                        .current_point_x
                        .ok_or_else(|| err_msg("Current X undefined"))?,
                };

                let y = match cmd.y {
                    Some(v) => (v as f64) * scale,
                    None => self
                        .graphics_state
                        .current_point_y
                        .ok_or_else(|| err_msg("Current Y undefined"))?,
                };

                let start_i = out.len();

                // TODO: Need to transform all the generated objects (position and polarity).
                self.draw_aperture(current_aperture, out)?;

                let offset = Vector2f::from_slice(&[x as f32, y as f32]);
                for obj in &mut out[start_i..] {
                    match obj {
                        GraphicsObject::FillPath(path, fill) => {
                            path.translate(offset.clone());

                            *fill = match *fill {
                                FillMode::Dark | FillMode::Clear => {
                                    self.graphics_state.polarity.into()
                                }
                                FillMode::Unset => FillMode::Unset,
                            };
                        }
                        GraphicsObject::EndOfLayer => {}
                    }
                }

                // TODO: Add end of layer markers everywhere.

                self.graphics_state.current_point_x = Some(x);
                self.graphics_state.current_point_y = Some(y);
            }

            Command::Comment(_)
            | Command::EnableArcs
            | Command::SetAttribute(_)
            | Command::DeleteAttribute(_)
            | Command::DeleteAttribute(_) => {}

            Command::EndOfProgram => {
                // TODO: Verify we hit the end.
                // println!("END");
            }
        }

        Ok(())
    }

    fn get_current_aperture(&self) -> Result<&ApertureDefinition> {
        let current_aperture_name = self
            .graphics_state
            .current_aperture
            .as_ref()
            .ok_or_else(|| err_msg("No aperture selected"))?;

        self.aperture_definition
            .get(current_aperture_name)
            .ok_or_else(|| format_err!("No aperture defined with name: {}", current_aperture_name))
    }

    /// Draws the non-transformed aperture.
    fn draw_aperture(
        &self,
        aperture: &ApertureDefinition,
        out: &mut Vec<GraphicsObject>,
    ) -> Result<()> {
        let call = match &aperture.shape {
            ApertureShape::Circle(circle) => TemplateCall {
                name: "C".to_string(),
                params: vec![circle.diameter, circle.hole_diameter.unwrap_or(0.0)],
            },
            ApertureShape::Rectangle(rect) => TemplateCall {
                name: "R".to_string(),
                params: vec![rect.x_size, rect.y_size, rect.hole_diameter.unwrap_or(0.0)],
            },
            ApertureShape::Obround(shape) => {
                // Depending on the size, either round the left/right or top/bottom edges.
                let name = {
                    if shape.x_size > shape.y_size {
                        "OX"
                    } else {
                        "O"
                    }
                }
                .to_string();

                // TODO: Maybe optimize into a single outer path to avoid drawing full circles
                // that overlap with the main body (same for D01 code).
                TemplateCall {
                    name,
                    params: vec![
                        shape.x_size,
                        shape.y_size,
                        shape.hole_diameter.unwrap_or(0.0),
                    ],
                }
            }
            ApertureShape::TemplateCall(call) => call.clone(),
        };

        let tmpl = self.aperture_templates.get(&call.name).ok_or_else(|| {
            format_err!(
                "No aperture macro/template defined with name: {}",
                call.name
            )
        })?;

        draw_template_call(tmpl, &call.params, &self.options, out)?;

        out.push(GraphicsObject::EndOfLayer);

        Ok(())
    }
}

/// NOTE: The generated graphical objects are all drawn with an origin of (0,0)
/// and a polarity of 'Clear'.
fn draw_template_call(
    tmpl: &ApertureMacro,
    params: &[f64],
    options: &CommandsProcessorOptions,
    out: &mut Vec<GraphicsObject>,
) -> Result<()> {
    let mut evaluator = ExpressionEvaluator::default();
    evaluator.add_call_params(params);

    for item in &tmpl.body {
        match item {
            ApertureMacroItem::Primitive(prim) => {
                match prim {
                    ApertureMacroPrimitive::Comment(_) => {}
                    ApertureMacroPrimitive::Circle(circle) => {
                        check_rotation(&circle.rotation, &evaluator)?;

                        let exposure = get_exposure(&circle.exposure, &evaluator)?;

                        let center_x = evaluator.evaluate(&circle.center_x)?;
                        let center_y = evaluator.evaluate(&circle.center_y)?;
                        let diameter = evaluator.evaluate(&circle.diameter)?;

                        // Skip zero diameter circles.
                        if diameter as f32 <= options.min_feature_size {
                            continue;
                        }

                        let r = (diameter / 2.0) as f32;

                        let mut path_builder = PathBuilder::new();
                        path_builder.ellipse(
                            Vector2f::from_slice(&[center_x as f32, center_y as f32]),
                            Vector2f::from_slice(&[r, r]),
                            0.0,
                            2.0 * PI,
                        );

                        out.push(GraphicsObject::FillPath(path_builder.build(), exposure));
                    }
                    ApertureMacroPrimitive::VectorLine(line) => {
                        check_rotation(&line.rotation, &evaluator)?;

                        let exposure = get_exposure(&line.exposure, &evaluator)?;

                        let width = evaluator.evaluate(&line.width)? as f32;
                        let start_x = evaluator.evaluate(&line.start_x)? as f32;
                        let start_y = evaluator.evaluate(&line.start_y)? as f32;
                        let end_x = evaluator.evaluate(&line.end_x)? as f32;
                        let end_y = evaluator.evaluate(&line.end_y)? as f32;

                        let start = Vector2f::from_slice(&[start_x, start_y]);
                        let end = Vector2f::from_slice(&[end_x, end_y]);

                        if (&start - &end).norm() <= 0.05 || width <= 0.05 {
                            continue;
                        }

                        // TODO: Deduplicate this with the line stroking code.

                        let line = Line2::from_points(&start, &end);

                        let dir = line.perp().normalized();

                        let p1 = &start + dir.clone() * (width / 2.0);
                        let p2 = &start + dir.clone() * (-width / 2.0);
                        let p3 = &end + dir.clone() * (-width / 2.0);
                        let p4 = &end + dir.clone() * (width / 2.0);

                        let mut path_builder = PathBuilder::new();
                        path_builder.move_to(p1);
                        path_builder.line_to(p2);
                        path_builder.line_to(p3);
                        path_builder.line_to(p4);
                        path_builder.close();

                        out.push(GraphicsObject::FillPath(path_builder.build(), exposure));
                    }
                    ApertureMacroPrimitive::Outline(line) => {
                        check_rotation(&line.rotation, &evaluator)?;

                        let exposure = get_exposure(&line.exposure, &evaluator)?;

                        // Minimum shape will be a triangle with the first point repeated to close
                        // the shape.
                        if line.points.len() < 4 {
                            return Err(err_msg("Too few points to form a shape"));
                        }

                        let mut path_builder = PathBuilder::new();

                        // TODO: Verify that the last point must equal the first point (must be
                        // closed).

                        // TODO: This path may not be counterclockwise.

                        let mut first = true;
                        for pt in &line.points {
                            let x = evaluator.evaluate(&pt.x)? as f32;
                            let y = evaluator.evaluate(&pt.y)? as f32;
                            let p = Vector2f::from_slice(&[x, y]);

                            if first {
                                path_builder.move_to(p);
                            } else {
                                path_builder.line_to(p);
                            }

                            first = false;
                        }

                        out.push(GraphicsObject::FillPath(path_builder.build(), exposure));
                    }
                    ApertureMacroPrimitive::CenterLine(line) => {
                        check_rotation(&line.rotation, &evaluator)?;

                        let exposure = get_exposure(&line.exposure, &evaluator)?;

                        let width = evaluator.evaluate(&line.width)? as f32;
                        let height = evaluator.evaluate(&line.height)? as f32;
                        let center_x = evaluator.evaluate(&line.center_x)? as f32;
                        let center_y = evaluator.evaluate(&line.center_y)? as f32;

                        if width <= options.min_feature_size || height <= options.min_feature_size {
                            continue;
                        }

                        let mut path_builder = PathBuilder::new();

                        // TODO: Deduplicate with line stroking code.
                        let mut pts = vec![];
                        pts.push((center_x - (width / 2.0), center_y - (height / 2.0)));
                        pts.push((center_x + (width / 2.0), center_y - (height / 2.0)));
                        pts.push((center_x + (width / 2.0), center_y + (height / 2.0)));
                        pts.push((center_x - (width / 2.0), center_y + (height / 2.0)));

                        path_builder.move_to(Vector2f::from_slice(&[pts[0].0, pts[0].1]));
                        for pt in &pts[1..] {
                            path_builder.line_to(Vector2f::from_slice(&[pt.0, pt.1]));
                        }

                        path_builder.close();

                        out.push(GraphicsObject::FillPath(path_builder.build(), exposure));
                    }

                    ApertureMacroPrimitive::Polygon => todo!(),
                    ApertureMacroPrimitive::Thermal => todo!(),
                }
            }
            ApertureMacroItem::VariableDefinition(def) => {
                evaluator.define_variable(&def.name, &def.value)?;
            }
        }
    }

    Ok(())
}

fn check_rotation(expr: &Expression, eval: &ExpressionEvaluator) -> Result<()> {
    let rotation = eval.evaluate(expr)?;
    if rotation != 0.0 {
        return Err(err_msg("Rotation not supported"));
    }

    Ok(())
}

fn get_exposure(expr: &Expression, eval: &ExpressionEvaluator) -> Result<FillMode> {
    let exposure = match eval.evaluate(expr)? {
        1.0 => true,
        0.0 => false,
        v @ _ => return Err(format_err!("Invalid exposure: {}", v)),
    };

    Ok(if exposure {
        FillMode::Dark
    } else {
        FillMode::Unset
    })
}

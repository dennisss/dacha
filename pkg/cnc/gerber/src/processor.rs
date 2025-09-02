use std::{collections::HashMap, f32::consts::PI};

use base_error::*;
use graphics::raster::stroke::stroke_poly;
use graphics::canvas::{Path, PathBuilder};
use math::{
    geometry::line::Line2,
    matrix::{vec2f, Vector2f},
};

use math::geometry::ellipse::Ellipse;
use math::geometry::curve::Curve2;

use crate::{expression::ExpressionEvaluator, graphics::*, syntax::*};

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
/// - 'LL' macro: Params are width, start_x, start_y, end_x, end_y
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

%AMLL*
20,1,$1,$2,$3,$4,$5,0*
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
    aperture_definition: HashMap<String, ApertureDefinitionWithAttributes>,
    attributes: HashMap<String, Attribute>
}

struct ApertureDefinitionWithAttributes {
    def: ApertureDefinition,
    attrs: HashMap<String, Attribute>,
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

    // If non-empty, we are processing a command after a BeginRegion and before an EndRegion.
    region_state: Option<RegionState>, 
}

#[derive(Default)]
struct RegionState {
    // Note that each contour in a region will be emitted as a separate path.
    past_contours: Vec<Path>,
    current_contour: Option<CurrentContour>,
}

struct CurrentContour {
    start_point: Vector2f,
    last_point: Vector2f,
    path_builder: PathBuilder,
}

impl RegionState {
    fn finish_current_contour(&mut self) -> Result<()> {
        let mut c = match self.current_contour.take() {
            Some(v) => v,
            None => return Ok(())
        };

        if c.start_point != c.last_point {
            return Err(err_msg("Contour is not closed"));
        }

        self.past_contours.push(c.path_builder.build());

        Ok(())
    }
}


impl CommandsProcessor {
    pub fn create(options: CommandsProcessorOptions) -> Result<Self> {
        let mut inst = Self {
            options,
            graphics_state: GraphicsState::default(),
            aperture_definition: HashMap::default(),
            aperture_templates: HashMap::default(),
            attributes: HashMap::default(),
        };

        let preamble = File::parse(PREAMBLE.as_bytes())?;

        let mut tmp = vec![];
        for cmd in preamble.commands {
            inst.process(&cmd, &mut tmp)?;
        }

        Ok(inst)
    }

    pub fn process(&mut self, command: &Command, out: &mut Vec<GraphicsObject>) -> Result<()> {

        // Allowlist the commands allowed inside of a region statement.
        if self.graphics_state.region_state.is_some() {
            let allowed = match command {
                Command::Move(_) | Command::SetPlotState(_) | Command::Plot(_) | Command::EndRegion => true,
                _ => false
            };

            if !allowed {
                return Err(format_err!("Command not allowed inside of a filled region: {:?}", command));
            }
        }

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
                let mut attrs = self.attributes.clone();
                attrs.retain(|_, attr| attr.typ == AttributeType::Aperture);

                // TODO: Check no duplicates.
                self.aperture_definition.insert(v.id.clone(), ApertureDefinitionWithAttributes {
                    def: v.clone(),
                    attrs,
                });
            }

            Command::SetCurrentAperture(v) => {
                self.graphics_state.current_aperture = Some(v.clone());
            }

            Command::BeginRegion => {
                self.graphics_state.region_state = Some(RegionState::default());
            }
            Command::EndRegion => {
                let mut region = self.graphics_state.region_state.take()
                    .ok_or_else(|| err_msg("EndRegion command not allowed before a BeginRegion command"))?;
                region.finish_current_contour()?;
                
                let fill = self.graphics_state.polarity.into();

                out.push(GraphicsObject {
                    paths: region.past_contours.into_iter().map(|path| {
                        GraphicsPath {
                            path,
                            fill,
                        }
                    }).collect(),
                    line: None,
                    attributes: HashMap::new(),
                });
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

                if let Some(region) = &mut self.graphics_state.region_state {
                    let x = self.graphics_state.current_point_x.ok_or_else(|| err_msg("Missing x pos in region"))?;
                    let y = self.graphics_state.current_point_y.ok_or_else(|| err_msg("Missing y pos in region"))?;

                    region.finish_current_contour()?;
                    let p = vec2f(x as f32, y as f32);

                    let mut path_builder = PathBuilder::new();
                    path_builder.move_to(p.clone());

                    region.current_contour = Some(CurrentContour {
                        start_point: p.clone(),
                        last_point: p.clone(),
                        path_builder
                    });
                }
            }

            Command::Plot(cmd) => {
                // TODO: Region support.

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

                let circle_diameter = {
                    let current_aperture = self.get_current_aperture()?;

                    let circle = match &current_aperture.def.shape {
                        ApertureShape::Circle(c) => c,
                        _ => return Err(err_msg("Only circle apertures supported in plot")),
                    };

                    if circle.hole_diameter.is_some() {
                        return Err(err_msg("Can not plot a circle with a hole"));
                    }

                    circle.diameter
                };

                if self.graphics_state.unit != Some(Mode::Millimeter) {
                    return Err(err_msg("Units must be set to millimeters before drawing"));
                }

                let mut current_contour = match &mut self.graphics_state.region_state {
                    Some(region) => {
                        Some(region.current_contour.as_mut().ok_or_else(|| err_msg("Move command required before first plot in a region"))?)
                    }
                    None => None
                };

                let plot_state = self.graphics_state.plot_state.ok_or_else(|| err_msg("No plot state set yet"))?;

                if plot_state == PlotState::Linear {

                    if let Some(current_contour) = self.get_current_contour()? {
                        let p = vec2f(end_x as f32, end_y as f32);
                        current_contour.path_builder.line_to(p.clone());
                        current_contour.last_point = p;
                    } else {
                        // NOTE: We don't need any pre-processing for translating this macro.
                        let mut paths = vec![];
                        self.draw_aperture(
                            &ApertureDefinition {
                                id: "".to_string(),
                                shape: ApertureShape::TemplateCall(TemplateCall {
                                    name: "L".to_string(),
                                    params: vec![circle_diameter, start_x, start_y, end_x, end_y],
                                }),
                            },
                            &mut paths,
                        )?;

                        out.push(GraphicsObject {
                            paths,
                            line: Some((
                                vec2f(start_x as f32, start_y as f32),
                                vec2f(end_x as f32, end_y as f32),
                            )),
                            attributes: HashMap::new(),
                        });
                    }


                } else {
                    let (i, j) = cmd.ij.clone()
                        .ok_or_else(|| err_msg("Expected arc plotting to specify an I and J offset"))?;

                    let center_x = start_x + (i as f64) * scale;
                    let center_y = start_y + (j as f64) * scale;

                    // TODO: Do everything in f64
                    let center = vec2f(center_x as f32, center_y as f32);
                    let start_vec = vec2f(start_x as f32, start_y as f32) - &center;
                    let end_vec = vec2f(end_x as f32, end_y as f32) - &center;

                    let radius = start_vec.norm();
                    let radius2 = end_vec.norm();
                    if (radius - radius2).abs() >= 0.001 {
                        return Err(err_msg("Expected arc to be of constant radius"));
                    }

                    let start_angle = start_vec.y().atan2(start_vec.x());
                    let end_angle = end_vec.y().atan2(end_vec.x());

                    // TODO: Ignore very small angles.
                    let mut delta_angle = end_angle - start_angle;

                    {
                        if delta_angle >= 2.0 * PI {
                            delta_angle -= 2.0 * PI;
                        }
                        if delta_angle <= -2.0 * PI {
                            delta_angle += 2.0 * PI;
                        }

                        // If true, then the delta angle should be positive.
                        let increasing_angle = match plot_state {
                            PlotState::Linear => todo!(),
                            PlotState::ClockwiseCircular => false,
                            PlotState::CounterClockwiseCircular => true,
                        };

                        if increasing_angle != (delta_angle > 0.0) {
                            if delta_angle >= 0.0 {
                                delta_angle -= 2.0 * PI;
                            } else {
                                delta_angle += 2.0 * PI;
                            }
                        }
                    }

                    let ellipse = Ellipse {
                        center: center.clone(),
                        x_axis: vec2f(radius, 0.0),
                        y_axis: vec2f(0.0, radius),
                        start_angle,
                        delta_angle,
                    };

                    let mut points = vec![];
                    ellipse.linearize(self.options.min_feature_size, &mut points);

                    for point_i in 0..(points.len() - 1) {
                        if let Some(current_contour) = self.get_current_contour()? {
                            current_contour.path_builder.line_to(points[point_i + 1].clone());
                            current_contour.last_point = points[point_i + 1].clone();
                            continue;
                        }

                        let mut paths = vec![];

                        if point_i == 0 {
                            // TODO: Add start circle.
                        }

                        if point_i == points.len() - 1 {
                            // TODO: Add end circle
                        }

                        // Drawing the main line.
                        self.draw_aperture(
                            &ApertureDefinition {
                                id: "".to_string(),
                                shape: ApertureShape::TemplateCall(TemplateCall {
                                    name: "LL".to_string(),
                                    params: vec![
                                        circle_diameter,
                                        points[point_i].x() as f64, points[point_i].y() as f64,
                                        points[point_i + 1].x() as f64, points[point_i + 1].y() as f64
                                    ],
                                }),
                            },
                            &mut paths,
                        )?;

                        out.push(GraphicsObject {
                            paths,
                            line: Some((
                                points[point_i].clone(),
                                points[point_i + 1].clone()
                            )),
                            attributes: HashMap::new(),
                        });
                    }
                }

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

                // TODO: Need to transform all the generated objects (position and polarity).
                let mut paths = vec![];
                self.draw_aperture(&current_aperture.def, &mut paths)?;

                let offset = Vector2f::from_slice(&[x as f32, y as f32]);
                for path in &mut paths {
                    path.path.translate(offset.clone());

                    path.fill = match path.fill {
                        FillMode::Dark | FillMode::Clear => self.graphics_state.polarity.into(),
                        FillMode::Unset => FillMode::Unset,
                    };
                }

                out.push(GraphicsObject { paths, line: None, attributes: HashMap::new(), });

                self.graphics_state.current_point_x = Some(x);
                self.graphics_state.current_point_y = Some(y);
            }

            Command::Comment(_)
            | Command::EnableArcs => {}
            Command::SetAttribute(attr) => {
                self.attributes.insert(attr.name.clone(), attr.clone());
            }
            | Command::DeleteAttribute(name) => {
                if let Some(name) = name {
                    self.attributes.remove(name);
                } else {
                    self.attributes.retain(|_, attr| {
                        attr.typ == AttributeType::File
                    });
                }

            }

            Command::EndOfProgram => {
                // TODO: Verify we hit the end.
                // println!("END");
            }
        }

        Ok(())
    }

    /// Assuming we are able to perform a plot command, gets the current contour if any.
    fn get_current_contour(&mut self) -> Result<Option<&mut CurrentContour>> {
        Ok(match &mut self.graphics_state.region_state {
            Some(region) => {
                Some(region.current_contour.as_mut().ok_or_else(|| err_msg("Move command required before first plot in a region"))?)
            }
            None => None
        })
    }

    fn get_current_aperture(&self) -> Result<&ApertureDefinitionWithAttributes> {
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
        out: &mut Vec<GraphicsPath>,
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

        Ok(())
    }
}

/// NOTE: The generated graphical objects are all drawn with an origin of (0,0)
/// and a polarity of 'Clear'.
fn draw_template_call(
    tmpl: &ApertureMacro,
    params: &[f64],
    options: &CommandsProcessorOptions,
    out: &mut Vec<GraphicsPath>,
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
                        if (diameter as f32) < options.min_feature_size {
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

                        out.push(GraphicsPath {
                            path: path_builder.build(),
                            fill: exposure,
                        });
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

                        if (&start - &end).norm() < options.min_feature_size
                            || width < options.min_feature_size
                        {
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

                        out.push(GraphicsPath {
                            path: path_builder.build(),
                            fill: exposure,
                        });
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

                        out.push(GraphicsPath {
                            path: path_builder.build(),
                            fill: exposure,
                        });
                    }
                    ApertureMacroPrimitive::CenterLine(line) => {
                        check_rotation(&line.rotation, &evaluator)?;

                        let exposure = get_exposure(&line.exposure, &evaluator)?;

                        let width = evaluator.evaluate(&line.width)? as f32;
                        let height = evaluator.evaluate(&line.height)? as f32;
                        let center_x = evaluator.evaluate(&line.center_x)? as f32;
                        let center_y = evaluator.evaluate(&line.center_y)? as f32;

                        if width < options.min_feature_size || height < options.min_feature_size {
                            continue;
                        }

                        // TODO: Ideally just convert to a VectorLine and render that instead.

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

                        out.push(GraphicsPath {
                            path: path_builder.build(),
                            fill: exposure,
                        });
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

use base_error::*;
use parsing::*;

macro_rules! regexp_parser {
    ($name:ident, $s:expr) => {
        fn $name(input: &[u8]) -> ParseResult<&str, &[u8]> {
            regexp!(PATTERN => $s);

            let m = PATTERN.exec(input).ok_or_else(|| err_msg("No match"))?;
            let v = m.group_str(0).unwrap()?;
            let rest = &input[m.last_index()..];
            Ok((v, rest))
        }
    };
}

#[derive(Debug, Clone)]
pub struct File {
    pub commands: Vec<Command>,
}

impl File {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let data = std::str::from_utf8(data)?.replace('\n', "");

        let mut commands = vec![];
        let mut rest = data.as_str();
        while !rest.is_empty() {
            let (v, r) = match Command::parse(rest.as_bytes()) {
                Ok(v) => v,
                Err(e) => {
                    // TODO: Print data.
                    println!("REMAINING UNPARSED: {}", rest);
                    return Err(e);
                }
            };

            commands.push(v);

            let n = rest.len() - r.len();
            rest = &rest[n..];
        }

        Ok(Self { commands })
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    Comment(String),
    Mode(Mode),
    FormatSpecifier(FormatSpecifier),
    SetPlotState(PlotState),
    EnableArcs,
    ApertureDefinition(ApertureDefinition),
    ApertureMacro(ApertureMacro),
    SetCurrentAperture(String),
    Plot(PlotCommand),
    Move(MoveCommand),
    Flash(FlashCommand),
    LoadPolarity(Polarity),
    LoadMirroring(Mirroring),
    LoadRotation(Rotation),
    LoadScaling(Scaling),
    EndOfProgram,
    SetAttribute(Attribute),
    DeleteAttribute(Option<String>),
    BeginRegion,
    EndRegion,
}

impl Command {
    parser!(parse<&[u8], Self> => alt!(
        map(parse_g04, |v| Self::Comment(v)),
        map(parse_mo, |v| Self::Mode(v)),
        map(parse_fs, |v| Self::FormatSpecifier(v)),
        map(tag("G01*"), |_| Self::SetPlotState(PlotState::Linear)),
        map(tag("G02*"), |_| Self::SetPlotState(PlotState::ClockwiseCircular)),
        map(tag("G03*"), |_| Self::SetPlotState(PlotState::CounterClockwiseCircular)),
        map(tag("G75*"), |_| Self::EnableArcs),
        map(tag("G36*"), |_| Self::BeginRegion),
        map(tag("G37*"), |_| Self::EndRegion),
        map(ApertureDefinition::parse, |v| Self::ApertureDefinition(v)),
        map(ApertureMacro::parse, |v| Self::ApertureMacro(v)),
        map(parse_dnn, |v| Self::SetCurrentAperture(v)),
        map(PlotCommand::parse, |v| Self::Plot(v)),
        map(MoveCommand::parse, |v| Self::Move(v)),
        map(FlashCommand::parse, |v| Self::Flash(v)),
        map(parse_lp, |v| Self::LoadPolarity(v)),
        map(parse_lm, |v| Self::LoadMirroring(v)),
        map(parse_lr, |v| Self::LoadRotation(v)),
        map(parse_ls, |v| Self::LoadScaling(v)),
        map(tag("M02*"), |_| Self::EndOfProgram),
        map(Attribute::parse, |v| Self::SetAttribute(v)),
        map(parse_td, |v| Self::DeleteAttribute(v))
    ));
}

// G04 = 'G04' string '*';
parser!(parse_g04<&[u8], String> => seq!(c => {
    c.next(tag("G04"))?;
    let v = c.next(parse_string)?.to_string();
    c.next(tag("*"))?;
    Ok(v)
}));

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Millimeter,
    Inch,
}

// MO = '%MO' ('MM'|'IN') '*%';
parser!(parse_mo<&[u8], Mode> => seq!(c => {
    c.next(tag("%MO"))?;
    let v = c.next(alt!(
        map(tag("MM"), |_| Mode::Millimeter),
        map(tag("IN"), |_| Mode::Inch)
    ))?;
    c.next(tag("*%"))?;
    Ok(v)
}));

#[derive(Debug, Clone)]
pub struct FormatSpecifier {
    pub x_digits: usize,
    pub y_digits: usize,
}

// FS = '%FS' 'LA' 'X' coordinate_digits 'Y' coordinate_digits '*%';
// coordinate_digits = /[1-6][6]/;
parser!(parse_fs<&[u8], FormatSpecifier> => seq!(c => {
    c.next(tag("%FSLAX"))?;
    let x_digits = c.next(parse_coordinate_digits)?.parse()?;

    c.next(tag("Y"))?;
    let y_digits = c.next(parse_coordinate_digits)?.parse()?;

    c.next(tag("*%"))?;
    Ok(FormatSpecifier {
        x_digits,
        y_digits
    })
}));

regexp_parser!(parse_coordinate_digits, "^[1-6][6]");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotState {
    Linear,
    ClockwiseCircular,
    CounterClockwiseCircular,
}

/*
AD = '%AD'
 aperture_identifier
 (
 | 'C' ',' ~ decimal ['X' decimal]
 | 'R' ',' ~ decimal 'X' decimal ['X' decimal]
 | 'O' ',' ~ decimal 'X' decimal ['X' decimal]
 | 'P' ',' ~ decimal 'X' decimal ['X' decimal ['X' decimal]]
 | name [',' decimal {'X' decimal}*]
 )
 '*%';
*/
#[derive(Debug, Clone)]
pub struct ApertureDefinition {
    pub id: String,
    pub shape: ApertureShape,
}

#[derive(Debug, Clone)]
pub enum ApertureShape {
    Circle(CircleApertureDefinition),
    Rectangle(RectangleApertureDefinition),
    Obround(ObroundApertureDefinition),
    TemplateCall(TemplateCall),
}

impl ApertureDefinition {
    parser!(parse<&[u8], Self> => seq!(c => {
        c.next(tag("%AD"))?;

        let id = c.next(parse_aperture_identifier)?.to_string();

        let shape = c.next(alt!(
            map(CircleApertureDefinition::parse, |v| ApertureShape::Circle(v)),
            map(RectangleApertureDefinition::parse, |v| ApertureShape::Rectangle(v)),
            map(ObroundApertureDefinition::parse, |v| ApertureShape::Obround(v)),
            map(TemplateCall::parse, |v| ApertureShape::TemplateCall(v))
        ))?;

        c.next(tag("*%"))?;

        Ok(Self {
            id,
            shape
        })
    }));
}

#[derive(Debug, Clone)]
pub struct CircleApertureDefinition {
    pub diameter: f64,
    pub hole_diameter: Option<f64>,
}

impl CircleApertureDefinition {
    // 'C' ',' ~ decimal ['X' decimal]
    parser!(parse<&[u8], Self> => seq!(c => {
        c.next(tag("C,"))?;

        let diameter = c.next(parse_decimal)?.parse()?;

        let hole_diameter = c.next(opt(seq!(c => {
            c.next(tag("X"))?;
            let v = c.next(parse_decimal)?.parse()?;
            Ok(v)
        })))?;

        Ok(Self {
            diameter,
            hole_diameter
        })
    }));
}

#[derive(Debug, Clone)]
pub struct RectangleApertureDefinition {
    pub x_size: f64,
    pub y_size: f64,
    pub hole_diameter: Option<f64>,
}

impl RectangleApertureDefinition {
    // 'R' ',' ~ decimal 'X' decimal ['X' decimal]
    parser!(parse<&[u8], Self> => seq!(c => {
        c.next(tag("R,"))?;

        let x_size = c.next(parse_decimal)?.parse()?;

        c.next(tag("X"))?;

        let y_size = c.next(parse_decimal)?.parse()?;

        let hole_diameter = c.next(opt(seq!(c => {
            c.next(tag("X"))?;
            let v = c.next(parse_decimal)?.parse()?;
            Ok(v)
        })))?;

        Ok(Self {
            x_size,
            y_size,
            hole_diameter
        })
    }));
}

#[derive(Debug, Clone)]
pub struct ObroundApertureDefinition {
    pub x_size: f64,
    pub y_size: f64,
    pub hole_diameter: Option<f64>,
}

impl ObroundApertureDefinition {
    // 'O' ',' ~ decimal 'X' decimal ['X' decimal]
    //
    // TODO: Dedup with RectangleApertureDefinition
    parser!(parse<&[u8], Self> => seq!(c => {
        c.next(tag("O,"))?;

        let x_size = c.next(parse_decimal)?.parse()?;

        c.next(tag("X"))?;

        let y_size = c.next(parse_decimal)?.parse()?;

        let hole_diameter = c.next(opt(seq!(c => {
            c.next(tag("X"))?;
            let v = c.next(parse_decimal)?.parse()?;
            Ok(v)
        })))?;

        Ok(Self {
            x_size,
            y_size,
            hole_diameter
        })
    }));
}

#[derive(Debug, Clone)]
pub struct TemplateCall {
    pub name: String,
    pub params: Vec<f64>,
}

impl TemplateCall {
    // name [',' decimal {'X' decimal}*]
    parser!(parse<&[u8], Self> => seq!(c => {
        let name = c.next(parse_name)?.to_string();

        let params = c.next(opt(seq!(c => {
            c.next(tag(","))?;
            let mut out = vec![];
            out.push(c.next(parse_decimal)?.parse()?);

            while let Some(v) = c.next(opt(seq!(c => { c.next(tag("X"))?; c.next(parse_decimal) })))? {
                out.push(v.parse()?);
            }

            Ok(out)
        })))?.unwrap_or_default();

        Ok(Self {
            name, params
        })
    }));
}

// AM = '%AM' name '*' macro_body '%';
// macro_body = { primitive | variable_definition }+;
#[derive(Debug, Clone)]
pub struct ApertureMacro {
    pub name: String,
    pub body: Vec<ApertureMacroItem>,
}

impl ApertureMacro {
    parser!(parse<&[u8], Self> => seq!(c => {
        c.next(tag("%AM"))?;
        let name = c.next(parse_name)?.to_string();
        c.next(tag("*"))?;

        let body = c.next(many1(alt!(
            map(ApertureMacroPrimitive::parse, |v| ApertureMacroItem::Primitive(v)),
            map(VariableDefinition::parse, |v| ApertureMacroItem::VariableDefinition(v))
        )))?;

        c.next(tag("%"))?;

        Ok(Self {
            name,
            body
        })
    }));
}

#[derive(Debug, Clone)]
pub enum ApertureMacroItem {
    Primitive(ApertureMacroPrimitive),
    VariableDefinition(VariableDefinition),
}

#[derive(Debug, Clone)]
pub enum ApertureMacroPrimitive {
    Comment(String),
    Circle(CirclePrimitive),
    VectorLine(VectorLinePrimitive),
    CenterLine(CenterLinePrimitive),
    Outline(OutlinePrimitive),
    Polygon,
    Thermal,
}

#[derive(Debug, Clone)]
pub struct CirclePrimitive {
    pub exposure: Expression,
    pub diameter: Expression,
    pub center_x: Expression,
    pub center_y: Expression,
    pub rotation: Expression,
}

#[derive(Debug, Clone)]
pub struct VectorLinePrimitive {
    pub exposure: Expression,
    pub width: Expression,
    pub start_x: Expression,
    pub start_y: Expression,
    pub end_x: Expression,
    pub end_y: Expression,
    pub rotation: Expression,
}

#[derive(Debug, Clone)]
pub struct OutlinePrimitive {
    pub exposure: Expression,
    pub points: Vec<PointExpression>,
    pub rotation: Expression,
}

#[derive(Debug, Clone)]
pub struct PointExpression {
    pub x: Expression,
    pub y: Expression,
}

#[derive(Debug, Clone)]
pub struct CenterLinePrimitive {
    pub exposure: Expression,
    pub width: Expression,
    pub height: Expression,
    pub center_x: Expression,
    pub center_y: Expression,
    pub rotation: Expression,
}

impl ApertureMacroPrimitive {
    // primitive =
    //     | '0' string '*'
    //     | '1' ',' expr ',' expr ',' expr ',' expr [',' expr] '*'
    //     | '20' ',' expr ',' expr ',' expr ',' expr ',' expr ','
    //                expr ‘,’ expr '*'
    //     | '21' ',' expr ',' expr ',' expr ',' expr ',' expr ',' expr '*'
    //     | '4' ',' expr ',' expr ',' expr ',' expr {',' expr ',' expr}+ ','
    //               expr'*'
    //     | '5' ',' expr ',' expr ',' expr ',' expr ',' expr ',' expr '*'
    //     | '7' ',' expr ',' expr ',' expr ',' expr ',' expr ',' expr '*'
    //     ;
    parser!(parse<&[u8], Self> => seq!(c => {

        let v = c.next(alt!(
            seq!(c => {
                c.next(tag("0"))?;
                let v = c.next(parse_string)?.to_string();
                Ok(Self::Comment(v))
            }),
            seq!(c => {
                c.next(tag("1"))?;

                let exposure = c.next(Self::parse_next_param)?;
                let diameter = c.next(Self::parse_next_param)?;
                let center_x = c.next(Self::parse_next_param)?;
                let center_y = c.next(Self::parse_next_param)?;
                let rotation = match c.next(opt(Self::parse_next_param))? {
                    Some(v) => v,
                    None => Expression::Number(0.0)
                };

                Ok(Self::Circle(CirclePrimitive {
                    exposure,
                    diameter,
                    center_x,
                    center_y,
                    rotation
                }))
            }),
            seq!(c => {
                c.next(tag("20"))?;

                let exposure = c.next(Self::parse_next_param)?;
                let width = c.next(Self::parse_next_param)?;
                let start_x = c.next(Self::parse_next_param)?;
                let start_y = c.next(Self::parse_next_param)?;
                let end_x = c.next(Self::parse_next_param)?;
                let end_y = c.next(Self::parse_next_param)?;
                let rotation = c.next(Self::parse_next_param)?;

                Ok(Self::VectorLine(VectorLinePrimitive {
                    exposure,
                    width,
                    start_x,
                    start_y,
                    end_x,
                    end_y,
                    rotation
                }))
            }),
            seq!(c => {
                c.next(tag("21"))?;

                let exposure = c.next(Self::parse_next_param)?;
                let width = c.next(Self::parse_next_param)?;
                let height = c.next(Self::parse_next_param)?;
                let center_x = c.next(Self::parse_next_param)?;
                let center_y = c.next(Self::parse_next_param)?;
                let rotation = c.next(Self::parse_next_param)?;

                Ok(Self::CenterLine(CenterLinePrimitive {
                    exposure,
                    width,
                    height,
                    center_x,
                    center_y,
                    rotation,
                }))
            }),
            seq!(c => {
                c.next(tag("4"))?;

                let exposure = c.next(Self::parse_next_param)?;
                let num_vertices = match c.next(Self::parse_next_param)? {
                    Expression::Number(v) => v as usize,
                    _ => return Err(err_msg("Num vertices must be statically defined"))
                };

                let mut points = vec![];
                for _ in 0..(num_vertices + 1) {
                    let x = c.next(Self::parse_next_param)?;
                    let y = c.next(Self::parse_next_param)?;
                    points.push(PointExpression { x, y });
                }

                let rotation = c.next(Self::parse_next_param)?;

                Ok(Self::Outline(OutlinePrimitive {
                    exposure, points, rotation
                }))
            })
        ))?;

        c.next(tag("*"))?;

        Ok(v)
    }));

    parser!(parse_next_param<&[u8], Expression> => seq!(c => {
        c.next(tag(","))?;
        c.next(Expression::parse)
    }));
}

#[derive(Debug, Clone)]
pub struct VariableDefinition {
    pub name: String,
    pub value: Expression,
}

impl VariableDefinition {
    // variable_definition = macro_variable '=' expr '*';
    parser!(parse<&[u8], Self> => seq!(c => {
        let name = c.next(parse_macro_variable)?.to_string();
        c.next(tag("="))?;
        let value = c.next(Expression::parse)?;
        c.next(tag("*"))?;
        Ok(Self {
            name, value
        })
    }));
}

#[derive(Debug, Clone)]
pub enum Expression {
    Number(f64),
    Variable(String),
    BinaryOp(BinaryOp, Box<Expression>, Box<Expression>),
}

#[derive(Debug, Clone)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Expression {
    // expr =
    //     |{/[+-]/ term}+
    //     |expr /[+-]/ term
    //     |term
    //     ;
    parser!(parse<&[u8], Self> => seq!(c => {
        let sign = c.next(opt(alt!(
            map(tag("+"), |_| 1.0),
            map(tag("-"), |_| -1.0)
        )))?;

        let mut expr = c.next(Self::parse_term)?;

        if let Some(sign) = sign {
            expr = Self::BinaryOp(BinaryOp::Multiply, Box::new(Expression::Number(sign)), Box::new(expr));
        }

        loop {
            let v = c.next(opt(seq!(c => {
                let op = c.next(alt!(
                    map(tag("+"), |_| BinaryOp::Add),
                    map(tag("-"), |_| BinaryOp::Subtract)
                ))?;

                let f = c.next(Self::parse_term)?;
                Ok((op, f))
            })))?;


            if let Some((op, term)) = v {
                expr = Expression::BinaryOp(op, Box::new(expr), Box::new(term));
            } else {
                break;
            }
        }

        Ok(expr)
    }));

    // term =
    //     |term /[x\/]/ factor
    //     |factor
    //     ;
    parser!(parse_term<&[u8], Self> => seq!(c => {
        let mut expr = c.next(Self::parse_factor)?;

        loop {
            let v = c.next(opt(seq!(c => {
                let op = c.next(alt!(
                    map(tag("x"), |_| BinaryOp::Multiply),
                    map(tag("/"), |_| BinaryOp::Divide)
                ))?;

                let f = c.next(Self::parse_factor)?;
                Ok((op, f))
            })))?;


            if let Some((op, factor)) = v {
                expr = Expression::BinaryOp(op, Box::new(expr), Box::new(factor));
            } else {
                break;
            }
        }

        Ok(expr)
    }));

    // factor =
    //     | '(' ~ expr ')'
    //     |macro_variable
    //     |unsigned_decimal
    //     ;
    parser!(parse_factor<&[u8], Self> => alt!(
        seq!(c => {
            c.next(tag("("))?;
            let e = c.next(Self::parse)?;
            c.next(tag(")"))?;
            Ok(e)
        }),
        map(parse_macro_variable, |v| Self::Variable(v.to_string())),
        seq!(c => {
            let v = c.next(parse_unsigned_decimal)?.parse()?;
            Ok(Self::Number(v))
        })
    ));
}

regexp_parser!(parse_macro_variable, "^\\$[0-9]*[1-9][0-9]*");

// Dnn = aperture_identifier '*';
parser!(parse_dnn<&[u8], String> => seq!(c => {
    let v = c.next(parse_aperture_identifier)?.to_string();
    c.next(tag("*"))?;
    Ok(v)
}));

#[derive(Debug, Clone)]
pub struct PlotCommand {
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub ij: Option<(i64, i64)>,
}

impl PlotCommand {
    // D01 = ['X' integer] ['Y' integer] ['I' integer 'J' integer] 'D01*';
    parser!(parse<&[u8], Self> => seq!(c => {
        let x = c.next(opt(seq!(c => {
            c.next(tag("X"))?;
            let v = c.next(parse_integer)?.parse()?;
            Ok(v)
        })))?;

        let y = c.next(opt(seq!(c => {
            c.next(tag("Y"))?;
            let v = c.next(parse_integer)?.parse()?;
            Ok(v)
        })))?;

        let ij = c.next(opt(seq!(c => {
            c.next(tag("I"))?;
            let i = c.next(parse_integer)?.parse()?;

            c.next(tag("J"))?;
            let j = c.next(parse_integer)?.parse()?;

            Ok((i, j))
        })))?;

        c.next(tag("D01*"))?;

        Ok(Self {
            x, y, ij
        })
    }));
}

#[derive(Debug, Clone)]
pub struct MoveCommand {
    pub x: Option<i64>,
    pub y: Option<i64>,
}

impl MoveCommand {
    // D02 = ['X' integer] ['Y' integer] 'D02*';
    parser!(parse<&[u8], Self> => seq!(c => {
        let x = c.next(opt(seq!(c => {
            c.next(tag("X"))?;
            let v = c.next(parse_integer)?.parse()?;
            Ok(v)
        })))?;

        let y = c.next(opt(seq!(c => {
            c.next(tag("Y"))?;
            let v = c.next(parse_integer)?.parse()?;
            Ok(v)
        })))?;

        c.next(tag("D02*"))?;

        Ok(Self {
            x, y
        })
    }));
}

#[derive(Debug, Clone)]
pub struct FlashCommand {
    pub x: Option<i64>,
    pub y: Option<i64>,
}

impl FlashCommand {
    // D03 = ['X' integer] ['Y' integer] 'D03*';
    parser!(parse<&[u8], Self> => seq!(c => {
        let x = c.next(opt(seq!(c => {
            c.next(tag("X"))?;
            let v = c.next(parse_integer)?.parse()?;
            Ok(v)
        })))?;

        let y = c.next(opt(seq!(c => {
            c.next(tag("Y"))?;
            let v = c.next(parse_integer)?.parse()?;
            Ok(v)
        })))?;

        c.next(tag("D03*"))?;

        Ok(Self {
            x, y
        })
    }));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Clear,
    Dark,
}

impl Default for Polarity {
    fn default() -> Self {
        Self::Dark
    }
}

// LP = '%LP' ('C'|'D') '*%';
parser!(parse_lp<&[u8], Polarity> => seq!(c => {
    c.next(tag("%LP"))?;

    let p = c.next(alt!(
        map(tag("C"), |_| Polarity::Clear),
        map(tag("D"), |_| Polarity::Dark)
    ))?;

    c.next(tag("*%"))?;

    Ok(p)
}));

#[derive(Debug, Clone)]
pub enum Mirroring {
    None,
    X,
    Y,
    XY,
}

impl Default for Mirroring {
    fn default() -> Self {
        Self::None
    }
}

// LM = '%LM' ('N'|'XY'|'Y'|'X') '*%';
parser!(parse_lm<&[u8], Mirroring> => seq!(c => {
    c.next(tag("%LM"))?;

    let p = c.next(alt!(
        map(tag("N"), |_| Mirroring::None),
        map(tag("XY"), |_| Mirroring::XY),
        map(tag("Y"), |_| Mirroring::Y),
        map(tag("X"), |_| Mirroring::X)
    ))?;

    c.next(tag("*%"))?;

    Ok(p)
}));

#[derive(Debug, Clone, Default)]
pub struct Rotation {
    /// Counterclockwise degrees.
    pub degrees: f64,
}

// LR = '%LR' decimal '*%';
parser!(parse_lr<&[u8], Rotation> => seq!(c => {
    c.next(tag("%LR"))?;

    let v = c.next(parse_decimal)?.parse()?;

    c.next(tag("*%"))?;

    Ok(Rotation { degrees: v })
}));

#[derive(Debug, Clone)]
pub struct Scaling {
    pub scale: f64,
}

impl Default for Scaling {
    fn default() -> Self {
        Self { scale: 0.0 }
    }
}

// LS = '%LS' decimal '*%';
parser!(parse_ls<&[u8], Scaling> => seq!(c => {
    c.next(tag("%LS"))?;

    let v = c.next(parse_decimal)?.parse()?;

    c.next(tag("*%"))?;

    Ok(Scaling { scale: v })
}));

/*
TODO:

AB_statement = AB_open block AB_close;
AB_open = '%AB' aperture_identifier '*%';
AB_close = '%AB' '*%';

SR_statement = SR_open block SR_close;
SR_open = '%SR' 'X' positive_integer 'Y' positive_integer
 'I' decimal 'J' decimal '*%';
SR_close = '%SR' '*%';


*/

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeType {
    File,
    Aperture,
    Object,
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub typ: AttributeType,
    pub name: String,
    pub fields: Vec<String>,
}

impl Attribute {
    parser!(parse<&[u8], Self> => seq!(c => {
        let typ = c.next(alt!(
            map(tag("%TF"), |_| AttributeType::File),
            map(tag("%TA"), |_| AttributeType::Aperture),
            map(tag("%TO"), |_| AttributeType::Object)
        ))?;

        let name = c.next(parse_name)?.to_string();

        let fields = c.next(many(seq!(c => {
            c.next(tag(","))?;
            Ok(c.next(parse_field)?.to_string())
        })))?;

        c.next(tag("*%"))?;

        Ok(Self {
            typ, name, fields
        })
    }));
}

parser!(parse_td<&[u8], Option<String>> => seq!(c => {
    c.next(tag("%TD"))?;

    let name = c.next(opt(parse_name))?.map(|v| v.to_string());

    c.next(tag("*%"))?;

    Ok(name)
}));

regexp_parser!(parse_integer, "^[+-]?[0-9]+");

regexp_parser!(
    parse_unsigned_decimal,
    "^((([0-9]+)(\\.[0-9]*)?)|(\\.[0-9]+))"
);

regexp_parser!(parse_decimal, "^[+-]?((([0-9]+)(\\.[0-9]*)?)|(\\.[0-9]+))");

regexp_parser!(parse_aperture_identifier, "^D[0]*[1-9][0-9]+");

regexp_parser!(parse_name, "^[._a-zA-Z$][._a-zA-Z0-9]*");

regexp_parser!(parse_field, "^[^%*,]*");

// TODO: Must interprate escaped characters.
// Can contain escape sequences like '\u00A9' or '\U000000A9'
// Must escape ',' (if not at the end of a word) '\', '%', '*'
regexp_parser!(parse_string, "^[^%*]*");

regexp_parser!(parse_user_name, "^[_a-zA-Z$][._a-zA-Z0-9]*");

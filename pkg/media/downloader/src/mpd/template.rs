use common::errors::*;

regexp!(FORMAT_PATTERN => "^([A-Za-z]*)(%0[0-9]+d)?$");

#[derive(Default)]
pub struct SegmentTemplateInputs<'a> {
    pub representation_id: Option<&'a str>,
    pub number: Option<usize>,
    pub bandwidth: Option<usize>,
    pub time: Option<usize>,
    pub sub_number: Option<usize>,
}

pub struct SegmentTemplateStr<'a> {
    parts: Vec<Part<'a>>,
}

#[derive(Debug, PartialEq, Eq)]
enum Part<'a> {
    String(&'a str),
    RepresentationId,
    Number(usize),
    Bandwidth(usize),
    Time(usize),
    SubNumber(usize),
}

impl<'a> SegmentTemplateStr<'a> {
    pub fn parse_from(s: &'a str) -> Result<Self> {
        let mut rest = s;
        let mut parts = vec![];
        let mut last_index = 0;

        let mut in_sequence = false;
        while !rest.is_empty() {
            let mut i = 0;
            while i < rest.len() {
                if rest.as_bytes()[i] == b'$' {
                    break;
                }

                i += 1;
            }

            let (span, r) = rest.split_at(i);
            let mut hit_eos = false;
            if r.is_empty() {
                hit_eos = true;
                rest = r;
            } else {
                rest = r.split_at(1).1; // Remove the '$'
            }

            if !in_sequence {
                parts.push(Part::String(span));
                if !hit_eos {
                    in_sequence = true;
                }
                continue;
            }

            if hit_eos {
                return Err(format_err!(
                    "Unterminated format sequence in template: {}",
                    s
                ));
            }

            let m = FORMAT_PATTERN
                .exec(span)
                .ok_or_else(|| format_err!("Unknown format pattern: {}", span))?;

            let ident = m.group_str(1).unwrap()?;

            let mut width = match m.group_str(2) {
                Some(s) => Some(s?.parse::<usize>()?),
                None => None,
            };

            if ident == "" {
                parts.push(Part::String("$"));
            } else if ident == "RepresentationID" {
                parts.push(Part::RepresentationId)
            } else if ident == "Number" {
                parts.push(Part::Number(width.take().unwrap_or(1)));
            } else if ident == "Bandwidth" {
                parts.push(Part::Bandwidth(width.take().unwrap_or(1)));
            } else if ident == "Time" {
                parts.push(Part::Time(width.take().unwrap_or(1)));
            } else if ident == "SubNumber" {
                parts.push(Part::SubNumber(width.take().unwrap_or(1)));
            } else {
                return Err(format_err!("Unknown identifier: {}", ident));
            }

            if width.is_some() {
                return Err(format_err!("Unused width specifier in: {}", span));
            }

            in_sequence = false;
        }

        Ok(Self { parts })
    }

    pub fn format(&self, inputs: &SegmentTemplateInputs) -> Result<String> {
        let mut out = String::new();

        for part in &self.parts {
            match part {
                Part::String(s) => out.push_str(*s),
                Part::RepresentationId => {
                    let v = inputs
                        .representation_id
                        .clone()
                        .ok_or_else(|| err_msg("Missing RepresentationId"))?;
                    out.push_str(v);
                }
                Part::Number(width) => {
                    let v = inputs
                        .number
                        .clone()
                        .ok_or_else(|| err_msg("Missing Number"))?;

                    out.push_str(&format!("{:0width$}", v, width = *width));
                }
                Part::Bandwidth(width) => {
                    let v = inputs
                        .bandwidth
                        .clone()
                        .ok_or_else(|| err_msg("Missing Bandwidth"))?;

                    out.push_str(&format!("{:0width$}", v, width = *width));
                }
                Part::Time(width) => {
                    let v = inputs.time.clone().ok_or_else(|| err_msg("Missing Time"))?;

                    out.push_str(&format!("{:0width$}", v, width = *width));
                }
                Part::SubNumber(width) => {
                    let v = inputs
                        .sub_number
                        .clone()
                        .ok_or_else(|| err_msg("Missing SubNumber"))?;

                    out.push_str(&format!("{:0width$}", v, width = *width));
                }
            }
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_template_parsing() -> Result<()> {
        let tmpl = SegmentTemplateStr::parse_from("hello $Number$ world")?;
        assert_eq!(
            &tmpl.parts,
            &[
                Part::String("hello "),
                Part::Number(1),
                Part::String(" world")
            ]
        );

        Ok(())
    }
}

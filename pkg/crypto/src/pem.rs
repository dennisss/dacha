use bytes::Bytes;
use common::errors::*;
use parsing::ascii::*;
use parsing::*;

// NOTE: PEM files should only ever be ASCII.

pub const PEM_CERTIFICATE_LABEL: &'static str = "CERTIFICATE";
pub const PEM_CERTIFICATE_REQUEST_LABEL: &'static str = "CERTIFICATE REQUEST";
pub const PEM_PRIVATE_KEY_LABEL: &'static str = "PRIVATE KEY";
pub const PEM_RSA_PRIVATE_KEY_LABEL: &'static str = "RSA PRIVATE KEY";
pub const PEM_X509_CRL_LABEL: &'static str = "X509 CRL";

#[derive(Debug)]
pub struct PEM {
    pub entries: Vec<PEMEntry>,
}

impl PEM {
    pub fn parse(input: Bytes) -> Result<Self> {
        // TODO: Eventually wrap in 'complete()'
        let (raw_entries, _) = many(alt!(
            map(PEMEntry::parse, |e| Some(e)),
            map(other_line, |_| None)
        ))(input)?;

        let entries = raw_entries.into_iter().filter_map(|x| x).collect::<_>();
        Ok(Self { entries })
    }
}

#[derive(Debug)]
pub struct PEMEntry {
    /// Type name associated with this entry.
    pub label: AsciiString,
    /// Value of this entry as present in the file. (Not parsed)
    pub value: AsciiString,
}

impl PEMEntry {
    parser!(parse<Self> => seq!(c => {
        let label = c.next(begin_line)?;

        // TODO: It should be a fatal error if we got this far but didn't
        // complete it.
        let value = c.next(take_until(map(end_line, |lbl| {
            if lbl != label { return Err(err_msg("Wrong ending")); }
            Ok(())
        })))?;
        c.next(end_line)?;

        Ok(Self { label, value: AsciiString::from(value).unwrap() })
    }));

    /// Parses this entry's value as base64 encoded
    pub fn to_binary(&self) -> Result<Vec<u8>> {
        // Filter out whitespace.
        let mut input = vec![];
        for b in &self.value.data {
            if !(*b as char).is_ascii_whitespace() {
                input.push(*b);
            }
        }

        let out = base64::decode(&input)?;
        println!("IN {} {}", input.len(), out.len());
        Ok(out)
    }
}

// TODO: Also support hitting the end of the file without a line ending.
// TODO: Change this to a regular expression.
// Parses '\n', '\r', or '\r\n'.
parser!(strict_line_ending<()> => seq!(c => {
    let cr = c.next(opt(one_of(b"\r")))?;
    let nl = c.next(opt(one_of(b"\n")))?;
    if cr.is_none() && nl.is_none() {
        return Err(err_msg("Invalid line ending"))
    }

    Ok(())
}));

parser!(line_ending<()> => alt!(
    strict_line_ending,
    // Allow hitting the end of the file.
    |input: Bytes| -> ParseResult<()> {
        if input.len() > 0 { Err(err_msg("Not at end")) } else { Ok(((), input)) }
    }
));

// '-----BEGIN CERTIFICATE-----'
parser!(begin_line<AsciiString> => seq!(c => {
    c.next(tag(b"-----BEGIN "))?;
    let lbl = c.next(label)?;
    c.next(tag(b"-----"))?;
    c.next(line_ending)?;
    Ok(lbl)
}));

// '-----END CERTIFICATE-----'
parser!(end_line<AsciiString> => seq!(c => {
    c.next(tag(b"-----END "))?;
    let lbl = c.next(label)?;
    c.next(tag(b"-----"))?;
    c.next(line_ending)?;
    Ok(lbl)
}));

parser!(label<AsciiString> => {
    map(
        slice(take_while(|b| {
            let c = b as char;
            // TODO: Only allow uppercase alphabetic.
            c.is_ascii_alphanumeric() || c == ' '
        })),
        |v| AsciiString::from(v).unwrap()
    )
});

parser!(other_line<Bytes> => seq!(c => {
    let data = c.next(take_while(|b| {
        let c = b as char;
        c != '\r' && c != '\n' && c.is_ascii()
    }))?;
    c.next(strict_line_ending)?;
    Ok(data)
}));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_test() {
        let test = b"# this is stuff before the certificate\n-----BEGIN CERTIFICATE-----\nthis is some\r\ndata\n-----END CERTIFICATE-----\nand this is after";

        let file = PEM::parse(Bytes::from_static(test)).unwrap();
        assert_eq!(file.entries.len(), 1);
        assert_eq!(file.entries[0].label.as_ref(), "CERTIFICATE");
        assert_eq!(file.entries[0].value.as_ref(), "this is some\r\ndata\n");

        // TODO: Test binary parsing.
    }
}

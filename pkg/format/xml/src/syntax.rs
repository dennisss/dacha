// See https://cs.lmu.edu/~ray/notes/xmlgrammar/
// Also https://www.w3.org/TR/REC-xml

use std::collections::HashMap;

use common::errors::*;
use parsing::*;

use crate::spec::*;

/*
document  ::=  prolog element Misc*
*/
parser!(pub parse_document<&str, Document> => seq!(c => {
    let (encoding, standalone) = c.next(parse_prolog)?;
    let root_element = c.next(parse_element)?;
    c.next(many(parse_misc))?;

    Ok(Document {
        encoding, standalone, root_element
    })
}));

/*
Char  ::=  #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]
*/
fn is_char(c: char) -> bool {
    let i = c as u32;
    i == 0x9 || i == 0xA || i == 0xD || i >= 0x20
}

/*
S  ::=  (#x20 | #x9 | #xD | #xA)+
*/
parser!(parse_s<&str, ()> => map(many1(one_of("\x20\x09\x0D\x0A")), |_| ()));

/*
NameChar  ::=  Letter | Digit
            |  '.' | '-' | '_' | ':'
            |  CombiningChar | Extender
*/
fn is_name_char(c: char) -> bool {
    is_letter(c)
        || is_digit(c)
        || c == '.'
        || c == '-'
        || c == '_'
        || c == ':'
        || is_combining_char(c)
        || is_extender(c)
}

/*
Name      ::=  (Letter | '_' | ':') (NameChar)*
*/
parser!(parse_name<&str, &str> => slice(seq!(c => {
    c.next(like(|c| is_letter(c) || c == '_' || c == ':'))?;
    c.next(many(like(is_name_char)))?;
    Ok(())
})));

/*
Names     ::=  Name (#x20 Name)*
Nmtoken   ::=  (NameChar)+
Nmtokens  ::=  Nmtoken (#x20 Nmtoken)*

Literals

EntityValue    ::=  '"' ([^%&"] | PEReference | Reference)* '"'
                 |  "'" ([^%&'] | PEReference | Reference)* "'"
*/

/*
AttValue       ::=  '"' ([^<&"] | Reference)* '"'
                 |  "'" ([^<&'] | Reference)* "'"
*/
parser!(parse_att_value<&str, String> => seq!(c => {
    let quote: char = c.next(one_of("\"'"))?;

    let chars = c.next(many(alt!(
        like(|c| c != '<' && c != '&' && c != quote),
        parse_reference
    )))?;

    c.next(atom(quote))?;

    let mut out = String::new();
    out.reserve(chars.len());
    for c in chars {
        out.push(c);
    }

    Ok(out)
}));

/*
SystemLiteral  ::=  ('"' [^"]* '"') | ("'" [^']* "'")
PubidLiteral   ::=  '"' PubidChar* '"' | "'" (PubidChar - "'")* "'"
PubidChar      ::=  #x20 | #xD | #xA | [a-zA-Z0-9]
                 |  [-'()+,./:=?;!*#@$_%]

Character Data
*/

/*
CharData  ::=  [^<&]* - ([^<&]* ']]>' [^<&]*)
*/
parser!(parse_char_data<&str, &str> => seq!(c => {
    let data: &str = c.next(slice(seq!(c => {
        loop {
            if let Some(_) = c.next(opt(peek(tag("]]>"))))? {
                break;
            }

            if let Some(_) = c.next(opt(not_one_of("<&")))? {
                // Continue
            } else {
                break;
            }
        }

        Ok(())
    })))?;

    if data.len() == 0 {
        return Err(err_msg("Empty CharData"));
    }

    Ok(data)
}));

/*
Comment  ::=  '<!--' ((Char - '-') | ('-' (Char - '-')))* '-->'
*/
parser!(parse_comment<&str, &str> => seq!(c => {
    c.next(tag("<!--"))?;
    let inner = c.next(slice(many(seq!(c => {
        let ch = c.next(like(is_char))?;
        if ch == '-' {
            c.next(like(|ch| is_char(ch) && ch != '-'))?;
        }

        Ok(())
    }))))?;
    c.next(tag("-->"))?;
    Ok(inner)
}));

/*
Processing Instructions

PI        ::=  '<?' PITarget (S (Char* - (Char* '?>' Char*)))? '?>'
PITarget  ::=  Name - (('X' | 'x') ('M' | 'm') ('L' | 'l'))

CDATA Sections

CDSect   ::=  CDStart CData CDEnd
CDStart  ::=  '<![CDATA['
CData    ::=  (Char* - (Char* ']]>' Char*))
CDEnd    ::=  ']]>'

*/

/*
prolog       ::=  XMLDecl? Misc* (doctypedecl Misc*)?
*/
parser!(parse_prolog<&str, (String, bool)> => seq!(c => {
    let value = c.next(opt(parse_xml_decl))?.unwrap_or((String::new(), false));
    c.next(many(parse_misc))?;

    // TODO: '(doctypedecl Misc*)?'

    Ok(value)
}));

/*
XMLDecl      ::=  '<?xml' VersionInfo EncodingDecl? SDDecl? S? '?>'
*/
parser!(parse_xml_decl<&str, (String, bool)> => seq!(c => {
    c.next(tag("<?xml"))?;
    c.next(parse_version_info)?;
    let encoding = c.next(opt(parse_encoding_decl))?.unwrap_or("").to_string();
    let standalone = c.next(opt(parse_sddecl))?.unwrap_or(false);
    c.next(opt(parse_s))?;
    c.next(tag("?>"))?;
    Ok((encoding, standalone))
}));

/*
VersionInfo  ::=  S 'version' Eq ("'" VersionNum "'" | '"' VersionNum '"')
*/
parser!(parse_version_info<&str, ()> => seq!(c => {
    c.next(parse_s)?;
    c.next(tag("version"))?;
    c.next(parse_eq)?;

    let quote = c.next(one_of("\"'"))?;
    c.next(parse_version_num)?;
    c.next(atom(quote))?;
    Ok(())
}));

/*
Eq           ::=  S? '=' S?
*/
parser!(parse_eq<&str, ()> => seq!(c => {
    c.next(opt(parse_s))?;
    c.next(tag("="))?;
    c.next(opt(parse_s))?;
    Ok(())
}));

/*
VersionNum   ::=  '1.0'
*/
parser!(parse_version_num<&str, ()> => tag("1.0"));

/*
Misc         ::=  Comment | PI | S
*/
parser!(parse_misc<&str, ()> => seq!(c => {
    c.next(alt!(
        map(parse_comment, |_| ()),
        parse_s
    ))?;

    Ok(())
}));

/*
Document Type Definition

doctypedecl    ::=  '<!DOCTYPE' S Name (S ExternalID)? S? ('[' intSubset ']' S?)? '>'
DeclSep        ::=  PEReference | S
intSubset      ::=  (markupdecl | DeclSep)*
markupdecl     ::=  elementdecl | AttlistDecl | EntityDecl | NotationDecl
                 |  PI | Comment
extSubset      ::=  TextDecl? extSubsetDecl
extSubsetDecl  ::=  ( markupdecl | conditionalSect | DeclSep)*
*/

/*
SDDecl  ::=  S 'standalone' Eq
             (("'" ('yes' | 'no') "'") | ('"' ('yes' | 'no') '"'))
*/
parser!(parse_sddecl<&str, bool> => seq!(c => {
    c.next(parse_s)?;
    c.next(tag("standalone"))?;
    c.next(parse_eq)?;

    let quote = c.next(one_of("\"'"))?;
    let value = c.next(alt!(
        map(tag("yes"), |_| true),
        map(tag("no"), |_| true)
    ))?;
    c.next(atom(quote))?;
    Ok(value)
}));

/*
element       ::=  EmptyElemTag  | STag content ETag
*/
parser!(parse_element<&str, Element> => alt!(
    parse_empty_elem_tag,
    seq!(c => {
        let mut el = c.next(parse_stag)?;
        el.content = c.next(parse_content)?;
        let etag = c.next(parse_etag)?;
        if etag != el.name {
            panic!("MISTMATCH");
            return Err(err_msg("Mismatching start/end tag name"));
        }

        Ok(el)
    })
));

// `(S Attribute)* S?`
parser!(parse_attributes<&str, HashMap<String, String>> => seq!(c => {
    let attrs: Vec<(String, String)> = c.next(many(seq!(c => {
        c.next(parse_s)?;
        c.next(parse_attribute)
    })))?;

    c.next(opt(parse_s))?;

    let attributes = attrs.into_iter().collect::<std::collections::HashMap<String, String>>();
    Ok(attributes)
}));

/*
STag          ::=  '<' Name (S Attribute)* S? '>'
*/
parser!(parse_stag<&str, Element> => seq!(c => {
    c.next(tag("<"))?;
    let name = c.next(parse_name)?.to_string();
    let attributes = c.next(parse_attributes)?;
    c.next(tag(">"))?;
    Ok(Element {
        name, attributes, content: vec![]
    })
}));

/*
Attribute     ::=  Name Eq AttValue
*/
parser!(parse_attribute<&str, (String, String)> => seq!(c => {
    let name = c.next(parse_name)?.to_string();
    c.next(parse_eq)?;
    let value = c.next(parse_att_value)?;
    Ok((name, value))
}));

/*
ETag          ::=  '</' Name S? '>'
*/
parser!(parse_etag<&str, &str> => seq!(c => {
    c.next(tag("</"))?;
    let name = c.next(parse_name)?;
    c.next(opt(parse_s))?;
    c.next(tag(">"))?;
    Ok(name)
}));

/*
content       ::=  CharData?
                   ((element | Reference | CDSect | PI | Comment) CharData?)*
*/
parser!(parse_content<&str, Vec<Node>> => seq!(c => {
    enum NodeFragment<'a> {
        Element(Element),
        Reference(char),
        Comment(&'a str),
        CharData(&'a str)
    }

    let mut nodes: Vec<Node> = vec![];

    let mut text = String::new();
    loop {
        let maybe_fragment = c.next(opt(alt!(
            map(parse_element, |v| NodeFragment::Element(v)),
            map(parse_reference, |v| NodeFragment::Reference(v)),
            map(parse_comment, |v| NodeFragment::Comment(v)),
            // NOTE: Not technically in the grammar, but should still work.
            map(parse_char_data, |v| NodeFragment::CharData(v))
        )))?;

        let fragment = if let Some(f) = maybe_fragment { f } else { break };

        // Handle changes to text
        match fragment {
            NodeFragment::Reference(v) => {
                text.push(v);
            }
            NodeFragment::CharData(v) => {
                text.push_str(v);
            }
            // Whenever a non-text element is seen, split off the current text node.
            _ => {
                if text.len() > 0 {
                    nodes.push(Node::Text(text.split_off(0)));
                }
            }
        }

        // Handle actual nodes.
        match fragment {
            NodeFragment::Element(v) => {
                nodes.push(Node::Element(v));
            }
            NodeFragment::Comment(v) => {
                nodes.push(Node::Comment(v.to_string()));
            }
            NodeFragment::Reference(v) => {}
            NodeFragment::CharData(v) => {}
        }
    }

    if text.len() > 0 {
        nodes.push(Node::Text(text.split_off(0)));
    }

    Ok(nodes)
}));

/*
EmptyElemTag  ::=  '<' Name (S Attribute)* S? '/>'
*/
parser!(parse_empty_elem_tag<&str, Element> => seq!(c => {
    c.next(tag("<"))?;
    let name = c.next(parse_name)?.to_string();
    let attributes = c.next(parse_attributes)?;
    c.next(tag("/>"))?;

    Ok(Element {
        name,
        attributes,
        content: vec![]
    })
}));

/*

Elements in the DTD

elementdecl  ::=  '<!ELEMENT' S Name S contentspec S? '>'
contentspec  ::=  'EMPTY' | 'ANY' | Mixed | children
children     ::=  (choice | seq) ('?' | '*' | '+')?
cp           ::=  (Name | choice | seq) ('?' | '*' | '+')?
choice       ::=  '(' S? cp ( S? '|' S? cp )+ S? ')'
seq          ::=  '(' S? cp ( S? ',' S? cp )* S? ')'
Mixed        ::=  '(' S? '#PCDATA' (S? '|' S? Name)* S? ')*'
               |  '(' S? '#PCDATA' S? ')'

Attributes in the DTD

AttlistDecl       ::=  '<!ATTLIST' S Name AttDef* S? '>'
AttDef            ::=  S Name S AttType S DefaultDecl
AttType           ::=  StringType | TokenizedType | EnumeratedType
StringType        ::=  'CDATA'
TokenizedType     ::=  'ID' | 'IDREF' | 'IDREFS' | 'ENTITY'
                    |  'ENTITIES' | 'NMTOKEN' | 'NMTOKENS'
EnumeratedType    ::=  NotationType | Enumeration
NotationType      ::=  'NOTATION' S '(' S? Name (S? '|' S? Name)* S? ')'
Enumeration       ::=  '(' S? Nmtoken (S? '|' S? Nmtoken)* S? ')'
DefaultDecl       ::=  '#REQUIRED' | '#IMPLIED' | (('#FIXED' S)? AttValue)

Conditional Section

conditionalSect     ::=  includeSect | ignoreSect
includeSect         ::=  '<![' S? 'INCLUDE' S? '[' extSubsetDecl ']]>'
ignoreSect          ::=  '<![' S? 'IGNORE' S? '[' ignoreSectContents* ']]>'
ignoreSectContents  ::= Ignore ('<![' ignoreSectContents ']]>' Ignore)*
Ignore              ::=  Char* - (Char* ('<![' | ']]>') Char*)

Character and Entity References
*/

/*
CharRef      ::=  '&#' [0-9]+ ';' | '&#x' [0-9a-fA-F]+ ';'
*/
parser!(parse_char_ref<&str, char> => alt!(
    seq!(c => {
        c.next(tag("&#"))?;
        let digits: &str = c.next(slice(many1(like(|ch: char| ch.is_ascii_digit()))))?;
        c.next(tag(";"))?;

        let code = digits.parse::<u32>()?;
        let c = std::char::from_u32(code).ok_or(err_msg("Invalid character"))?;
        Ok(c)
    }),
    seq!(c => {
        c.next(tag("&#x"))?;
        let digits: &str = c.next(slice(many1(like(|ch: char| ch.is_digit(16)))))?;
        c.next(tag(";"))?;

        let code = u32::from_str_radix(digits, 16)?;
        let ch = std::char::from_u32(code).ok_or(err_msg("Invalid character"))?;
        Ok(ch)
    })
));

/*
Reference    ::=  EntityRef | CharRef
*/
parser!(parse_reference<&str, char> => alt!(
    parse_entity_ref, parse_char_ref
));

/*
EntityRef    ::=  '&' Name ';'
*/
parser!(parse_entity_ref<&str, char> => seq!(c => {
    c.next(tag("&"))?;
    let name = c.next(parse_name)?;
    c.next(tag(";"))?;

    Ok(match name {
        "amp" => '&',
        "gt" => '>',
        "lt" => '<',
        _ => {
            return Err(format_err!("Unknown named entity: {}", name));
        }
    })
}));

/*
PEReference  ::=  '%' Name ';'
*/

/*
Entity Declarations

EntityDecl        ::=  GEDecl | PEDecl
GEDecl            ::=  '<!ENTITY' S Name S EntityDef S? '>'
PEDecl            ::=  '<!ENTITY' S '%' S Name S PEDef S? '>'
EntityDef         ::=  EntityValue | (ExternalID NDataDecl?)
PEDef             ::=  EntityValue | ExternalID
ExternalID        ::=  'SYSTEM' S SystemLiteral
                    |  'PUBLIC' S PubidLiteral S SystemLiteral
NDataDecl         ::=  S 'NDATA' S Name

Parsed Entities

TextDecl      ::=  '<?xml' VersionInfo? EncodingDecl S? '?>'
extParsedEnt  ::=  TextDecl? content
*/

/*
EncodingDecl  ::=  S 'encoding' Eq ('"' EncName '"' | "'" EncName "'" )
*/
parser!(parse_encoding_decl<&str, &str> => seq!(c => {
    c.next(parse_s)?;
    c.next(tag("encoding"))?;
    c.next(parse_eq)?;
    let quote = c.next(one_of("\"'"))?;
    let name = c.next(parse_enc_name)?;
    c.next(atom(quote))?;
    Ok(name)
}));

/*
EncName       ::=  [A-Za-z] ([A-Za-z0-9._] | '-')*
*/
parser!(parse_enc_name<&str, &str> => slice(seq!(c => {
    c.next(like(|c: char| c.is_ascii_alphabetic()))?;
    c.next(many(like(|c: char| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')))?;
    Ok(())
})));

/*
NotationDecl  ::=  '<!NOTATION' S Name S (ExternalID | PublicID) S? '>'
PublicID      ::=  'PUBLIC' S PubidLiteral
*/

/*
Letter         ::=  BaseChar | Ideographic
*/
fn is_letter(c: char) -> bool {
    is_base_char(c) || is_ideographic(c)
}

/*
BaseChar       ::=  [#x41-#x5A] | [#x61-#x7A] | [#xC0-#xD6]
                 |  [#xD8-#xF6] | [#xF8-#xFF] | [#x100-#x131]
                 |  [#x134-#x13E] | [#x141-#x148] | [#x14A-#x17E]
                 |  [#x180-#x1C3] | [#x1CD-#x1F0] | [#x1F4-#x1F5]
                 |  [#x1FA-#x217] | [#x250-#x2A8] | [#x2BB-#x2C1]
                 |  #x386 | [#x388-#x38A] | #x38C | [#x38E-#x3A1]
                 |  [#x3A3-#x3CE] | [#x3D0-#x3D6] | #x3DA | #x3DC
                 |  #x3DE | #x3E0 | [#x3E2-#x3F3] | [#x401-#x40C]
                 |  [#x40E-#x44F] | [#x451-#x45C] | [#x45E-#x481]
                 |  [#x490-#x4C4] | [#x4C7-#x4C8] | [#x4CB-#x4CC]
                 |  [#x4D0-#x4EB] | [#x4EE-#x4F5] | [#x4F8-#x4F9]
                 |  [#x531-#x556] | #x559 | [#x561-#x586]
                 |  [#x5D0-#x5EA] | [#x5F0-#x5F2] | [#x621-#x63A]
                 |  [#x641-#x64A] | [#x671-#x6B7] | [#x6BA-#x6BE]
                 |  [#x6C0-#x6CE] | [#x6D0-#x6D3] | #x6D5 | [#x6E5-#x6E6]
                 |  [#x905-#x939] | #x93D | [#x958-#x961] | [#x985-#x98C]
                 |  [#x98F-#x990] | [#x993-#x9A8] | [#x9AA-#x9B0]
                 |  #x9B2 | [#x9B6-#x9B9] | [#x9DC-#x9DD] | [#x9DF-#x9E1]
                 |  [#x9F0-#x9F1] | [#xA05-#xA0A] | [#xA0F-#xA10]
                 |  [#xA13-#xA28] | [#xA2A-#xA30] | [#xA32-#xA33]
                 |  [#xA35-#xA36] | [#xA38-#xA39] | [#xA59-#xA5C]
                 |  #xA5E | [#xA72-#xA74] | [#xA85-#xA8B] | #xA8D
                 |  [#xA8F-#xA91] | [#xA93-#xAA8] | [#xAAA-#xAB0]
                 |  [#xAB2-#xAB3] | [#xAB5-#xAB9] | #xABD | #xAE0
                 |  [#xB05-#xB0C] | [#xB0F-#xB10] | [#xB13-#xB28]
                 |  [#xB2A-#xB30] | [#xB32-#xB33] | [#xB36-#xB39]
                 |  #xB3D | [#xB5C-#xB5D] | [#xB5F-#xB61]
                 |  [#xB85-#xB8A] | [#xB8E-#xB90] | [#xB92-#xB95]
                 |  [#xB99-#xB9A] | #xB9C | [#xB9E-#xB9F]
                 |  [#xBA3-#xBA4] | [#xBA8-#xBAA] | [#xBAE-#xBB5]
                 |  [#xBB7-#xBB9] | [#xC05-#xC0C] | [#xC0E-#xC10]
                 |  [#xC12-#xC28] | [#xC2A-#xC33] | [#xC35-#xC39]
                 |  [#xC60-#xC61] | [#xC85-#xC8C] | [#xC8E-#xC90]
                 |  [#xC92-#xCA8] | [#xCAA-#xCB3] | [#xCB5-#xCB9]
                 |  #xCDE | [#xCE0-#xCE1] | [#xD05-#xD0C] | [#xD0E-#xD10]
                 |  [#xD12-#xD28] | [#xD2A-#xD39] | [#xD60-#xD61]
                 |  [#xE01-#xE2E] | #xE30 | [#xE32-#xE33] | [#xE40-#xE45]
                 |  [#xE81-#xE82] | #xE84 | [#xE87-#xE88] | #xE8A
                 |  #xE8D | [#xE94-#xE97] | [#xE99-#xE9F] | [#xEA1-#xEA3]
                 |  #xEA5 | #xEA7 | [#xEAA-#xEAB] | [#xEAD-#xEAE] | #xEB0
                 |  [#xEB2-#xEB3] | #xEBD | [#xEC0-#xEC4] | [#xF40-#xF47]
                 |  [#xF49-#xF69] | [#x10A0-#x10C5] | [#x10D0-#x10F6] | #x1100
                 |  [#x1102-#x1103] | [#x1105-#x1107] | #x1109 | [#x110B-#x110C]
                 |  [#x110E-#x1112] | #x113C | #x113E | #x1140 | #x114C | #x114E
                 |  #x1150 | [#x1154-#x1155] | #x1159 | [#x115F-#x1161] | #x1163
                 |  #x1165 | #x1167 | #x1169 | [#x116D-#x116E] | [#x1172-#x1173]
                 |  #x1175 | #x119E | #x11A8 | #x11AB | [#x11AE-#x11AF]
                 |  [#x11B7-#x11B8] | #x11BA | [#x11BC-#x11C2] | #x11EB | #x11F0
                 |  #x11F9 | [#x1E00-#x1E9B] | [#x1EA0-#x1EF9] | [#x1F00-#x1F15]
                 |  [#x1F18-#x1F1D] | [#x1F20-#x1F45] | [#x1F48-#x1F4D]
                 |  [#x1F50-#x1F57] | #x1F59 | #x1F5B | #x1F5D | [#x1F5F-#x1F7D]
                 |  [#x1F80-#x1FB4] | [#x1FB6-#x1FBC] | #x1FBE | [#x1FC2-#x1FC4]
                 |  [#x1FC6-#x1FCC] | [#x1FD0-#x1FD3] | [#x1FD6-#x1FDB]
                 |  [#x1FE0-#x1FEC] | [#x1FF2-#x1FF4] | [#x1FF6-#x1FFC] | #x2126
                 |  [#x212A-#x212B] | #x212E | [#x2180-#x2182] | [#x3041-#x3094]
                 |  [#x30A1-#x30FA] | [#x3105-#x312C] | [#xAC00-#xD7A3]
*/
fn is_base_char(c: char) -> bool {
    c.is_ascii_alphabetic()
}

/*
Ideographic    ::=  [#x4E00-#x9FA5] | #x3007 | [#x3021-#x3029]
*/
fn is_ideographic(c: char) -> bool {
    false
}

/*
CombiningChar  ::=  [#x300-#x345] | [#x360-#x361] | [#x483-#x486]
                 |  [#x591-#x5A1] | [#x5A3-#x5B9] | [#x5BB-#x5BD] | #x5BF
                 |  [#x5C1-#x5C2] | #x5C4 | [#x64B-#x652] | #x670
                 |  [#x6D6-#x6DC] | [#x6DD-#x6DF] | [#x6E0-#x6E4]
                 |  [#x6E7-#x6E8] | [#x6EA-#x6ED] | [#x901-#x903]
                 |  #x93C | [#x93E-#x94C] | #x94D | [#x951-#x954]
                 |  [#x962-#x963] | [#x981-#x983] | #x9BC | #x9BE
                 |  #x9BF | [#x9C0-#x9C4] | [#x9C7-#x9C8] | [#x9CB-#x9CD]
                 |  #x9D7 | [#x9E2-#x9E3] | #xA02 | #xA3C | #xA3E | #xA3F
                 |  [#xA40-#xA42] | [#xA47-#xA48] | [#xA4B-#xA4D]
                 |  [#xA70-#xA71] | [#xA81-#xA83] | #xABC | [#xABE-#xAC5]
                 |  [#xAC7-#xAC9] | [#xACB-#xACD] | [#xB01-#xB03] | #xB3C
                 |  [#xB3E-#xB43] | [#xB47-#xB48] | [#xB4B-#xB4D]
                 |  [#xB56-#xB57] | [#xB82-#xB83] | [#xBBE-#xBC2]
                 |  [#xBC6-#xBC8] | [#xBCA-#xBCD] | #xBD7 | [#xC01-#xC03]
                 |  [#xC3E-#xC44] | [#xC46-#xC48] | [#xC4A-#xC4D]
                 |  [#xC55-#xC56] | [#xC82-#xC83] | [#xCBE-#xCC4]
                 |  [#xCC6-#xCC8] | [#xCCA-#xCCD] | [#xCD5-#xCD6]
                 |  [#xD02-#xD03] | [#xD3E-#xD43] | [#xD46-#xD48]
                 |  [#xD4A-#xD4D] | #xD57 | #xE31 | [#xE34-#xE3A]
                 |  [#xE47-#xE4E] | #xEB1 | [#xEB4-#xEB9] | [#xEBB-#xEBC]
                 |  [#xEC8-#xECD] | [#xF18-#xF19] | #xF35 | #xF37 | #xF39
                 |  #xF3E | #xF3F | [#xF71-#xF84] | [#xF86-#xF8B]
                 |  [#xF90-#xF95] | #xF97 | [#xF99-#xFAD] | [#xFB1-#xFB7]
                 |  #xFB9 | [#x20D0-#x20DC] | #x20E1 | [#x302A-#x302F]
                 |  #x3099 | #x309A
*/
fn is_combining_char(c: char) -> bool {
    false
}

/*
Digit          ::=  [#x30-#x39] | [#x660-#x669] | [#x6F0-#x6F9]
                 |  [#x966-#x96F] | [#x9E6-#x9EF] | [#xA66-#xA6F]
                 |  [#xAE6-#xAEF] | [#xB66-#xB6F] | [#xBE7-#xBEF]
                 |  [#xC66-#xC6F] | [#xCE6-#xCEF] | [#xD66-#xD6F]
                 |  [#xE50-#xE59] | [#xED0-#xED9] | [#xF20-#xF29]
*/
fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/*
Extender       ::=  #xB7 | #x2D0 | #x2D1 | #x387 | #x640 | #xE46
                 |  #xEC6 | #x3005 | [#x3031-#x3035] | [#x309D-#x309E]
                 |  [#x30FC-#x30FE]
*/
fn is_extender(c: char) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_test() {
        let input = "<hello style=\"lalala\" >world</hello>";
        let (output, _) = complete(parse_document)(input).unwrap();
        println!("{:#?}", output);
    }

    #[test]
    fn document2_test() {
        let input = r#"<?xml version="1.0"?>
            <div style="lalala" >
                <p>world</p>
                <!-- I am a comment! -->
                apples
                <br />            
            </div>"#;
        let (output, _) = complete(parse_document)(input).unwrap();
        println!("{:#?}", output);
    }

    #[test]
    fn prolog_test() {
        let input = "<?xml version=\"1.0\"?>";
        let (output, _) = complete(parse_prolog)(input).unwrap();
        println!("{:#?}", output);
    }

    #[test]
    fn char_data_test() {
        let input = "world";
        let (output, _) = complete(parse_char_data)(input).unwrap();
        println!("{:#?}", output);
    }

    #[test]
    fn stag_test() {
        let input = "<hello>";
        let (output, _) = complete(parse_stag)(input).unwrap();
        println!("{:#?}", output);
    }
}

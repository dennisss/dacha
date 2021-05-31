// This file contains the utilities for parsing a regular expression string.
/*
    General grammar is:
        Regexp -> Alternation
        Alternation -> Expr | ( Expr '|' Alternation )
        Expr -> Element Expr | <empty>
        Element -> '^' | '$' | Quantified
        Quantified -> (Group | CharacterClass | EscapedLiteral | Literal) Repetitions
        Repetitions -> '*' | '?' | '+' | <empty>

        Group -> '(' Regexp ')'

*/

use common::errors::*;
use parsing::*;

use crate::regexp::node::*;

pub fn parse_root_expression(input: &str) -> Result<RegExpNodePtr> {
    let (node, _) = complete(alternation)(input)?;
    Ok(node)
}

parser!(alternation<&str, RegExpNodePtr> => {
    // TODO: Simplify if length == 1
    map(delimited(expr, tag("|")), |alts| Box::new(RegExpNode::Alt(alts)))
});

parser!(expr<&str, RegExpNodePtr> => {
    map(many(element), |els| Box::new(RegExpNode::Expr(els)))
});

parser!(element<&str, RegExpNodePtr> => alt!(
    map(tag("^"), |_| Box::new(RegExpNode::Start)),
    map(tag("$"), |_| Box::new(RegExpNode::End)),
    quantified
));

// Quantified -> Atom Quantifier | Atom
parser!(quantified<&str, RegExpNodePtr> => {
    seq!(c => {
        let a = c.next(atom)?;
        if let Some(q) = c.next(opt(quantifier))? {
            return Ok(Box::new(RegExpNode::Quantified(a, q)));
        }

        Ok(a)
    })
});

parser!(quantifier<&str, Quantifier> => alt!(
    map(tag("?"), |_| Quantifier::ZeroOrOne),
    map(tag("*"), |_| Quantifier::ZeroOrMore),
    map(tag("+"), |_| Quantifier::OneOrMore),
    seq!(c => {
        c.next(tag("{"))?;
        let lower_num = c.next(number)?;

        let upper_num: Option<Option<usize>> = c.next(opt(seq!(c => {
            c.next(tag(","))?;
            c.next(opt(number))
        })))?;

        c.next(tag("}"))?;

        Ok(match upper_num {
            Some(Some(upper_num)) => {
                if lower_num > upper_num {
                    return Err(err_msg("Invalid quantifier lower > higher"));
                }

                Quantifier::Between(lower_num, upper_num)
            },
            Some(None) => Quantifier::NOrMore(lower_num),
            None => Quantifier::ExactlyN(lower_num)
        })
    })
));

parser!(atom<&str, RegExpNodePtr> => alt!(
    map(shared_atom, |c| Box::new(RegExpNode::Literal(c))),
    map(tag("."), |_| Box::new(RegExpNode::Literal(Char::Wildcard))), // TODO: Check this
    literal,
    character_class,
    capture
));

// TODO: Strategy for implementing character classes
// If there are other overlapping symbols,

// TODO: Test what the pattern "[.]" matches.

// TODO: In PCRE, '[]]' would parse as a character class matching the character
// ']' but for simplity we will require that that ']' be escaped in a character
// class
parser!(character_class<&str, RegExpNodePtr> => seq!(c => {
    c.next(tag("["))?;
    let invert = c.next(opt(tag("^")))?;
    let inner = c.next(many(character_class_atom))?; // NOTE: We allow this to be empty.
    c.next(tag("]"))?;

    return Ok(Box::new(RegExpNode::Class { chars: inner, inverted: invert.is_some() }));
}));

parser!(capture<&str, RegExpNodePtr> => seq!(c => {
    c.next(tag("("))?;
    let (capturing, name) = c.next(opt(capture_flags))?.unwrap_or((true, String::new()));
    let inner = c.next(alternation)?;
    c.next(tag(")"))?;

    Ok(Box::new(RegExpNode::Capture { inner, capturing, name } ))
}));

parser!(capture_flags<&str, (bool, String)> => seq!(c => {
    c.next(tag("?"))?;

    c.next(alt!(
        map(tag(":"), |_| (false, String::new())),
        seq!(c => {
            c.next(tag("<"))?;
            let name: &str = c.next(take_while1(|c: char| c != '>'))?;
            c.next(tag(">"))?;
            Ok((true, name.to_owned()))
        })
    ))
}));

parser!(character_class_atom<&str, Char> => alt!(
    seq!(c => {
        let start = c.next(character_class_literal)?;
        c.next(tag("-"))?;
        let end = c.next(character_class_literal)?;

        // TODO: Return this as an error, but don't allow trying to parse
        // other alt! cases.
        assert!(end >= start);

        // In this case,
        Ok(Char::Range(start, end))
    }),

    shared_atom,
    map(character_class_literal, |c| Char::Value(c))
));

// TODO: Ensure that we support \t, \f \a, etc. See:
// https://github.com/google/re2/wiki/Syntax (search 'Escape Sequences')

// TODO: It seems like it could be better to combine this with the
// shared_literal class
parser!(shared_atom<&str, Char> => alt!(
    map(tag("\\w"), |_| Char::Word),
    map(tag("\\d"), |_| Char::Digit),
    map(tag("\\s"), |_| Char::Whitespace),
    map(tag("\\W"), |_| Char::NotWord),
    map(tag("\\D"), |_| Char::NotDigit),
    map(tag("\\S"), |_| Char::NotWhiteSpace)
));

// A single plain character that must be exactly matched.
// This rule does not apply to anything inside a character class.
// e.g. the regexp 'ab' contains 2 literals.
parser!(literal<&str, RegExpNodePtr> => {
    map(alt!(shared_literal,
             map(tag("]"), |_| ']')),
        |c| Box::new(RegExpNode::Literal(Char::Value(c))))
});

// TODO: Check this
parser!(character_class_literal<&str, char> => {
    //shared_literal |
    map(not_one_of("]"), |c| c as char)
});

// Single characters which need to be matched exactly
// (excluding symbols which may have a different meaning depending on context)
parser!(shared_literal<&str, char> => alt!(
    map(not_one_of("[]\\^$.|?*+()"), |v| v as char),
    escaped_literal
));

// TODO: Verify that '01' is a valid number
parser!(number<&str, usize> => and_then(
    take_while1(|c: char| c.is_digit(10)),
    // NOTE: We don't unwrap as it could be out of range.
    |s: &str| { let n = s.parse::<usize>()?; Ok(n) }
));

// Matches '\' followed by the character being escaped.
parser!(escaped_literal<&str, char> => {
    seq!(c => {
        c.next(tag("\\"))?;
        let v = c.next::<&str, _>(take_exact(1))?.chars().next().unwrap() as char;
        if v.is_alphanumeric() {
            return Err(err_msg("Expected non alphanumeric character"));
        }

        Ok(v)
    })
});
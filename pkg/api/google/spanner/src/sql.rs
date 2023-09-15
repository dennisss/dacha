// Basic parser for Google SQL expressions.
//
// Lexical reference: https://cloud.google.com/spanner/docs/reference/standard-sql/lexical
// DDL reference: https://cloud.google.com/spanner/docs/reference/standard-sql/data-definition-language

use common::errors::*;
use parsing::*;
use protobuf::tokenizer::whitespace;

type ParserInput<'a> = &'a str;

const RESERVED_WORDS: &'static [&'static str] = &[
    "ALL",
    "AND",
    "ANY",
    "ARRAY",
    "AS",
    "ASC",
    "ASSERT_ROWS_MODIFIED",
    "AT",
    "BETWEEN",
    "BY",
    "CASE",
    "CAST",
    "COLLATE",
    "CONTAINS",
    "CREATE",
    "CROSS",
    "CUBE",
    "CURRENT",
    "DEFAULT",
    "DEFINE",
    "DESC",
    "DISTINCT",
    "ELSE",
    "END",
    "ENUM",
    "ESCAPE",
    "EXCEPT",
    "EXCLUDE",
    "EXISTS",
    "EXTRACT",
    "FALSE",
    "FETCH",
    "FOLLOWING",
    "FOR",
    "FROM",
    "FULL",
    "GROUP",
    "GROUPING",
    "GROUPS",
    "HASH",
    "HAVING",
    "IF",
    "IGNORE",
    "IN",
    "INNER",
    "INTERSECT",
    "INTERVAL",
    "INTO",
    "IS",
    "JOIN",
    "LATERAL",
    "LEFT",
    "LIKE",
    "LIMIT",
    "LOOKUP",
    "MERGE",
    "NATURAL",
    "NEW",
    "NO",
    "NOT",
    "NULL",
    "NULLS",
    "OF",
    "ON",
    "OR",
    "ORDER",
    "OUTER",
    "OVER",
    "PARTITION",
    "PRECEDING",
    "PROTO",
    "RANGE",
    "RECURSIVE",
    "RESPECT",
    "RIGHT",
    "ROLLUP",
    "ROWS",
    "SELECT",
    "SET",
    "SOME",
    "STRUCT",
    "TABLESAMPLE",
    "THEN",
    "TO",
    "TREAT",
    "TRUE",
    "UNBOUNDED",
    "UNION",
    "UNNEST",
    "USING",
    "WHEN",
    "WHERE",
    "WINDOW",
    "WITH",
    "WITHIN",
];

#[derive(Clone, Debug, PartialEq)]
pub enum DdlStatement {
    CreateTable(CreateTableStatement),
    CreateIndex(CreateIndexStatement),
}

impl DdlStatement {
    pub fn parse(input: &str) -> Result<Self> {
        let (v, _) = complete(alt!(
            map(CreateTableStatement::parse, |v| Self::CreateTable(v)),
            map(CreateIndexStatement::parse, |v| Self::CreateIndex(v))
        ))(input)?;

        Ok(v)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateTableStatement {
    pub table_name: String,
    pub columns: Vec<ColumnDefinition>,
    pub primary_key: Vec<KeyPart>,
}

impl CreateTableStatement {
    parser!(parse<Self> => seq!(c => {
        c.next(reserved("CREATE"))?;
        c.next(reserved("TABLE"))?;

        let table_name = c.next(ident)?;
        c.next(is(symbol, '('))?;

        let columns = c.next(delimited(ColumnDefinition::parse, is(symbol, ',')))?;
        c.next(is(symbol, ','))?;

        c.next(is(symbol, ')'))?;

        c.next(reserved("PRIMARY"))?;
        c.next(reserved("KEY"))?;

        c.next(is(symbol, '('))?;

        let primary_key = c.next(delimited(KeyPart::parse, is(symbol, ',')))?;
        c.next(opt(is(symbol, ',')))?;

        c.next(is(symbol, ')'))?;

        Ok(Self {
            table_name, columns, primary_key
        })
    }));

    pub fn to_string(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "CREATE TABLE {} ({}) PRIMARY KEY ({})",
            self.table_name,
            self.columns
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.primary_key
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ));

        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnDefinition {
    pub column_name: String,
    pub typ: DataType,
    pub not_null: bool,
}

impl ColumnDefinition {
    parser!(parse<Self> => seq!(c => {
        let column_name = c.next(ident)?;
        let typ = c.next(DataType::parse)?;
        let not_null = c.next(opt(seq!(c => {
            c.next(reserved("NOT"))?;
            c.next(reserved("NULL"))?;
            Ok(())
        })))?.is_some();

        Ok(Self {
            column_name, typ, not_null
        })
    }));

    pub fn to_string(&self) -> String {
        let mut out = format!("{} {}", self.column_name, self.typ.to_string());
        if self.not_null {
            out.push_str(" NOT NULL");
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataType {
    pub is_array: bool,
    pub scalar_type: ScalarType,
}

impl DataType {
    parser!(parse<Self> => alt!(
        seq!(c => {
            c.next(reserved("ARRAY"))?;
            c.next(is(symbol, '<'))?;
            let inner = c.next(ScalarType::parse)?;
            c.next(is(symbol, '>'))?;
            Ok(Self { is_array: true, scalar_type: inner })
        }),
        seq!(c => {
            let inner = c.next(ScalarType::parse)?;
            Ok(Self { is_array: false, scalar_type: inner })
        })
    ));

    pub fn to_string(&self) -> String {
        let mut out = self.scalar_type.to_string();
        if self.is_array {
            out = format!("ARRAY<{}>", out);
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScalarType {
    Bool,
    Int64,
    Numeric,
    String(MaxLength),
    Json,
    Bytes(MaxLength),
    Date,
    Timestamp,
}

impl ScalarType {
    parser!(parse<Self> => alt!(
        map(reserved("BOOL"), |_| Self::Bool),
        map(reserved("INT64"), |_| Self::Int64),
        map(reserved("NUMERIC"), |_| Self::Numeric),
        seq!(c => {
            c.next(reserved("STRING"))?;
            c.next(is(symbol, '('))?;
            let length = c.next(MaxLength::parse)?;
            c.next(is(symbol, ')'))?;
            Ok(Self::String(length))
        }),
        map(reserved("JSON"), |_| Self::Json),
        seq!(c => {
            c.next(reserved("BYTES"))?;
            c.next(is(symbol, '('))?;
            let length = c.next(MaxLength::parse)?;
            c.next(is(symbol, ')'))?;
            Ok(Self::Bytes(length))
        }),
        map(reserved("DATE"), |_| Self::Date),
        map(reserved("TIMESTAMP"), |_| Self::Timestamp)
    ));

    pub fn to_string(&self) -> String {
        match self {
            ScalarType::Bool => "BOOL".to_string(),
            ScalarType::Int64 => "INT64".to_string(),
            ScalarType::Numeric => "NUMERIC".to_string(),
            ScalarType::String(len) => format!("STRING({})", len.to_string()),
            ScalarType::Json => "JSON".to_string(),
            ScalarType::Bytes(len) => format!("BYTES({})", len.to_string()),
            ScalarType::Date => "DATE".to_string(),
            ScalarType::Timestamp => "TIMESTAMP".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyPart {
    pub column_name: String,
    pub direction: Option<Direction>,
}

impl KeyPart {
    parser!(parse<Self> => seq!(c => {
        let column_name = c.next(ident)?;
        let direction = c.next(opt(Direction::parse))?;
        Ok(Self {
            column_name, direction
        })
    }));

    pub fn to_string(&self) -> String {
        let mut out = self.column_name.clone();
        if let Some(dir) = &self.direction {
            out.push(' ');
            out.push_str(&dir.to_string());
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Direction {
    Ascending,
    Descending,
}

impl Direction {
    parser!(parse<Self> => alt!(
        map(reserved("ASC"), |_| Self::Ascending),
        map(reserved("DESC"), |_| Self::Descending)
    ));

    pub fn to_string(&self) -> String {
        match self {
            Direction::Ascending => "ASC",
            Direction::Descending => "DESC",
        }
        .to_string()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MaxLength {
    Constant(i64),
    IntMax,
}

impl MaxLength {
    parser!(parse<Self> => alt!(
        map(number, |v| Self::Constant(v)),
        map(reserved("MAX"), |_| Self::IntMax)
    ));

    pub fn to_string(&self) -> String {
        match self {
            MaxLength::Constant(v) => v.to_string(),
            MaxLength::IntMax => "MAX".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateIndexStatement {
    pub unique: bool,
    pub index_name: String,
    pub table_name: String,
    pub key_parts: Vec<KeyPart>,
}

impl CreateIndexStatement {
    parser!(parse<Self> => seq!(c => {
        c.next(reserved("CREATE"))?;
        let unique = c.next(opt(reserved("UNIQUE")))?.is_some();
        c.next(reserved("INDEX"))?;

        let index_name = c.next(ident)?;
        c.next(reserved("ON"))?;
        let table_name = c.next(ident)?;
        c.next(is(symbol, '('))?;

        let key_parts = c.next(delimited(KeyPart::parse, is(symbol, ',')))?;
        c.next(opt(is(symbol, ',')))?;

        c.next(is(symbol, ')'))?;

        Ok(CreateIndexStatement { unique, index_name, table_name, key_parts })
    }));

    pub fn to_string(&self) -> String {
        let mut out = String::new();
        out.push_str("CREATE ");
        if self.unique {
            out.push_str("UNIQUE ");
        }
        out.push_str("INDEX ");

        out.push_str(&self.index_name);
        out.push_str(" ON ");
        out.push_str(&self.table_name);

        out.push('(');

        out.push_str(
            &self
                .key_parts
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        );

        out.push(')');
        out
    }
}

///////////////////////////////////////////////////////////////////////////////
// Tokenization stuff.
///////////////////////////////////////////////////////////////////////////////

parser!(skip_to<()> => {
    map(many(whitespace), |_| ())
});

parser!(symbol<char> => seq!(c => {
    c.next(skip_to)?;
    c.next(raw_symbol)
}));

parser!(raw_symbol<char> => one_of("(),<>"));

parser!(ident<String> => seq!(c => {
    // TODO: Disallow reserved words to be matched as identifiers
    c.next(skip_to)?;
    c.next(raw_identifier)
}));

parser!(raw_identifier<String> => {
    alt!(
        protobuf::tokenizer::ident,
        seq!(c => {
            // TODO: Support unescaping escaped characters.
            c.next(tag("`"))?;
            let v: &str = c.next(slice(take_while1(|c: char| c != '`' && !c.is_ascii_whitespace())))?;
            c.next(tag("`"))?;

            Ok(v.to_string())
        })
    )
});

fn reserved<'a>(word: &'static str) -> impl Fn(&'a str) -> Result<((), &'a str), Error> {
    move |input: &str| {
        let (v, rest) = is(ident, word)(input)?;
        Ok(((), rest))
    }
}

parser!(number<i64> => seq!(c => {
    c.next(skip_to)?;
    c.next(raw_number)
}));

parser!(raw_number<i64> => seq!(c => {
    let is_negative = c.next(opt(is(raw_symbol, '-')))?.is_some();
    let num = (c.next(protobuf::tokenizer::int_lit)? as i64) * if is_negative { -1 } else { 1 };
    Ok(num)
}));

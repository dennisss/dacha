// Old implementation of the RESP protocol using nom
// Only really good if we already have the whole packet in memory


#[derive(Debug, PartialEq)]
pub enum RESPObject<'a> {
	// XXX: Still better to just take ownership of each of the types here
	SimpleString(&'a [u8]),
	Error(&'a [u8]),
	BulkString(&'a [u8]),
	Integer(i64),
	Array(Vec<RESPObject<'a>>),
	Nil
}


named!(nl_string<&[u8], &[u8]>,
	do_parse!(
		value: take_while!(|c| { c != ('\n' as u8) && c != ('\r' as u8) }) >>
		tag!("\r\n") >>
		(value)
	)
);

fn parse_int(val: &[u8]) -> Result<i64> {
	let s = str::from_utf8(val)?;
	let num = s.parse()?;
	Ok(num)
}

// Issue being that this is fine but does not handle incomplete packets
named!(resp_object<&[u8], RESPObject>, 
	switch!(take!(1),
		b"+" => do_parse!(
			val: nl_string >> (RESPObject::SimpleString(val))
		) |
		b"-" => do_parse!(
			val: nl_string >> (RESPObject::Error(val))
		) |
		b":" => do_parse!(
			val: map_res!(nl_string, parse_int) >> (RESPObject::Integer(val))
		) |
		b"$" => do_parse!(
			len: map_res!(nl_string, parse_int) >>
			val: cond!(len >= 0,
				do_parse!( s: take!(len) >> tag!("\r\n") >> (s) )
			) >> (
				if let Some(v) = val {
					RESPObject::BulkString(v)
				}
				else {
					RESPObject::Nil
				}
			)
		) |
		b"*" => do_parse!(
			len: map_res!(nl_string, parse_int) >>
			items: cond!(len >= 0, do_parse!(
				arr: many0!(resp_object) >> (arr)
			)) >> (
				if let Some(arr) = items {
					RESPObject::Array(arr)
				}
				else {
					RESPObject::Nil
				}
			)
		)
	)
);
use common::errors::*;
use super::syntax::*;
use parsing::*;
use bytes::Bytes;
use super::encoding::TagClass;

pub struct LineBuilder {
	lines: Vec<String>
}

impl LineBuilder {
	pub fn new() -> Self {
		Self { lines: vec![] }
	}
	
	pub fn add<T: std::convert::Into<String>>(&mut self, line: T) {
		self.lines.push(line.into());
	}

	pub fn add_inline<T: std::convert::Into<String> + std::convert::AsRef<str>>(&mut self, line: T) {
		if let Some(last) = self.lines.last_mut() {
			*last += line.as_ref();
		} else {
			self.lines.push(line.into());
		}
	}

	pub fn append(&mut self, mut lines: LineBuilder) {
		self.lines.append(&mut lines.lines);
	}

	/// Similar to append() except the first line is merged with the last line
	/// of the current builder.
	pub fn append_inline(&mut self, mut lines: LineBuilder) {
		if let Some(last) = self.lines.last_mut() {
			if lines.lines.len() > 0 {
				*last += &lines.lines.remove(0);
			}
		}

		self.append(lines);
	}

	pub fn indent(&mut self) {
		for s in self.lines.iter_mut() {
			*s = format!("\t{}", s);
		}
	}

	pub fn indented<T, F: FnOnce(&mut LineBuilder) -> T>(&mut self, mut f: F)
	-> T {
		let mut inner = LineBuilder::new();
		let ret = f(&mut inner);
		inner.indent();
		self.append(inner);
		ret
	}

	pub fn wrap_with(&mut self, first: String, last: String) {
		let mut lines = vec![];
		lines.reserve(self.lines.len() + 2);
		lines.push(first);
		lines.append(&mut self.lines);
		lines.push(last);
		self.lines = lines;
	}

	pub fn empty(&self) -> bool {
		self.lines.len() == 0
	}

	pub fn nl(&mut self) {
		self.lines.push(String::new());
	}

	pub fn wrap_module(&mut self, name: &str) {
		let mut lines = vec![];
		lines.push(format!("pub mod {} {{\n", name));
		for s in self.lines.iter() {
			lines.push(format!("\t{}", s));
		}
		lines.push("}\n".into());
		self.lines = lines;
	}

	pub fn to_string(&self) -> String {
		let mut out = self.lines.join("\n");
		out.push('\n');
		out
	}
}

pub struct Context {
	names: Vec<String>
}

impl Context {
	fn new() -> Self {
		Self { names: vec![] }
	}

	fn inner(&self, module: &str) -> Self {
		let mut names = self.names.clone();
		names.push(module.to_string());
		Self { names }
	}

	fn resolve(&self, name: &str) -> String {
		if self.names.len() == 0 {
			name.to_string()
		} else {
			let mut s = self.names.join("::");
			format!("{}::{}", s, name)
		}
	}
}

fn impl_der_writeable<T, F: FnOnce(&mut LineBuilder) -> T>(
	lines: &mut LineBuilder, name: &str, f: F) -> T {
	lines.add(format!("impl DERWriteable for {} {{", name));

	let ret = lines.indented(|l| {
		l.add("fn write_der(&self, writer: &mut DERWriter) {");
		let ret = l.indented(|l| {
			f(l)
		});
		l.add("}");
		ret
	});
	lines.add("}");
	ret
}

fn impl_der_readable<T, F: FnOnce(&mut LineBuilder) -> T>(
	lines: &mut LineBuilder, name: &str, f: F) -> T {
	lines.add(format!("impl DERReadable for {} {{", name));

	let ret = lines.indented(|l| {
		l.add("fn read_der(&self, r: &mut DERReader) -> Result<Option<Self>> {");
		let ret = l.indented(|l| {
			f(l)
		});
		l.add("}");
		ret
	});
	lines.add("}");
	ret
}


// TODO: Things to validate an a file:
// - Any optional fields can be unambiguously distinguished from other fields.
// - All fields in a set have distinct tags (after applying any automatic tagging if enables)
// - Verify that all typenames and value identifiers can be resolved.

#[derive(PartialEq)]
enum EncodingMode {
	Read, Write
}

pub struct Compiler {
	output: String,
	// TODO: Must also consider automatic tagging.
	default_tagging: TagMode 
}

impl Compiler {

	pub fn new() -> Self {
		Self { output: String::new(), default_tagging: TagMode::Explicit }
	}

	fn value_name(name: &str) -> String {
		name.replace("-", "_").to_ascii_uppercase()
	}

	fn case_name(name: &str) -> String { name.replace("-", "_") }

	fn field_name(name: &str) -> String {
		if name == "type" {
			return "typ".into();
		}

		name.replace("-", "_")
	}

	fn type_name(name: &str) -> String {
		let mut parts = vec![];
		let mut last_part = String::new();
		for c in name.chars() {
			let isdelim = c == '_' || c == '-';
			if last_part.len() > 0 {
				if (c.is_ascii_alphabetic() && c.is_ascii_uppercase()) || isdelim {
					parts.push(last_part);
					last_part = String::new();
				}
			}

			if !isdelim {
				last_part.push(c);
			}
		}

		if last_part.len() > 0 {
			parts.push(last_part);
		}

		for s in parts.iter_mut() {
			let first = s.get_mut(0..1).unwrap();
			first.make_ascii_uppercase();
		}

		parts.join("")
	}

	fn compile_int_enum(&mut self, name: &str, variants: &[NamedNumber])
	-> Result<LineBuilder> {
		let mut lines = LineBuilder::new();
		let mut read_lines = LineBuilder::new();
		let mut write_lines = LineBuilder::new();
		lines.add(format!("pub enum {} {{", name));
		for v in variants {
			let case_name = Self::case_name(v.name.as_ref());
			let val = match &v.value {
				NamedNumberValue::Defined(n) => n.to_string(),
				NamedNumberValue::Immediate(v) => v.to_string()
			};

			read_lines.add(format!("\t{} => Self::{},", val, case_name));
			write_lines.add(format!("\tSelf::{} => {},", case_name, val));

			lines.add(format!("\t{} = {},", case_name, val));
		}
		lines.add("}");

		lines.nl();
		impl_der_readable(&mut lines, name, move |l| {
			l.add("let v = r.read_int()?;");
			l.add("Ok(match v {");
			l.append(read_lines);
			l.add("\t_ => { return Err(\"Invalid case\".into()); }");
			l.add("})");
		});

		impl_der_writeable(&mut lines, name, move |l| {
			l.add("let v = match self {");
			l.append(write_lines);
			l.add("};");
			l.nl();
			l.add("w.write_int(v);");
		});

		Ok(lines)
	}

	fn compile_collection_type(&mut self, ctype: &CollectionType, ctx: &Context)
	-> Result<(String, LineBuilder)> {
		match ctype {
			CollectionType::Named(t) => {
				self.compile_type(t.name.as_ref(), &t.typ, ctx)
			},
			CollectionType::Type(t) => {
				self.compile_type("Item", &t, ctx)
			}
		}
	}

	fn compile_type_prefixes(&mut self, encmode: EncodingMode,
							 prefixes: &[TypePrefix], lines: &mut LineBuilder)
	-> Result<()> {

		for p in prefixes {
			match p {
				TypePrefix::Encoding(_) => {
					return Err("Encoding prefix not supported".into());
				},
				TypePrefix::Tag(t) => {
					if t.tag.encoding_ref.is_some() {
						return Err(
							"Encoding reference in tag not supported".into());
					}

					let mode = t.mode.unwrap_or(self.default_tagging);
					let num = match &t.tag.number {
						ClassNumber::Immediate(n) => n.to_string(),
						ClassNumber::Defined(d) => d.0.to_string()
					};

					let class = match t.tag.class {
						TagClass::Universal => "TagClass::Universal",
						TagClass::Application => "TagClass::Application",
						TagClass::Private => "TagClass::Private",
						TagClass::ContextSpecific => "TagClass::ContextSpecific"
					};

					let verb = match encmode {
						EncodingMode::Read => "r.read",
						EncodingMode::Write => "w.write"
					};

					let adj = match mode {
						TagMode::Explicit => "explicitly",
						TagMode::Implicit => "implicitly"
					};

					let start = format!("w.{}_{}({}, {}, |w| {{",
										verb, adj, class, num);
					let mut end = String::from("})");
					if encmode == EncodingMode::Write {
						end += ";";
					}

					lines.indent();
					lines.wrap_with(start, end);
				}
			}
		}

		Ok(())
	}

	/// Compiles 
	fn compile_struct(&mut self, name: &str, prefixes: &[TypePrefix],
					  types: &[ComponentType], ctx: &Context) -> Result<LineBuilder> {

		let mut fields = vec![];
		for t in types.iter() {
			match t {
				ComponentType::Field(f) => { fields.push(f); },
				ComponentType::ComponentsOf(_) => {
					// TODO: Will need to index all of the types in order to do
					// a named lookup to fetch the fields.
					// (or observe the internal SEQUENCE/SET type)
					//
					// This will require a lot of care as it must maintain the
					// original tagging/encoding properties
					return Err("COMPONENTS OF not supported".into());
				}
			};
		}

		let mut lines = LineBuilder::new();

		// Create a module just for this struct if needed
		let modname = name.to_ascii_lowercase();
		let inner_ctx = ctx.inner(name);

		let mut field_typenames = std::collections::HashMap::<String, String>::new();

		// TODO: If any of these fields creates a new struct that consumes the
		// prefixes, then we shouldn't use any of the prefix information below.
		// let (inner, mut outer) = self.compile_component_fields(
		// 	&fields, &inner_ctx)?;

		let (inner, mut outer) = {
			let mut out = LineBuilder::new();
			let mut outer = LineBuilder::new();
			for f in &fields {
				let name = Self::field_name(f.name.as_ref());
				let (mut typename, l) = self.compile_type(&name, &f.typ, &inner_ctx)?;

				if let ComponentMode::Optional = f.mode {
					typename = format!("Option<{}>", typename);
				}

				field_typenames.insert(name.clone(), typename.clone());

				out.add(format!("\tpub {}: {},", name, typename));
				outer.append(l);
			}

			(out, outer)
		};


		lines.add(format!("pub struct {} {{", &name));
		lines.append(inner);
		lines.add("}");
		lines.nl();
		

		impl_der_readable(&mut lines, name, |l| {
			l.add("reader.read_sequence(|r| {");
			l.indented(|l| {
				let mut ctor = String::from("Ok(Self { "); 
				for f in fields.iter() {
					let mut field_lines = LineBuilder::new();
					let field_name = Self::field_name(f.name.as_ref());

					field_lines.add(format!("{}::read_der(r)",
									field_typenames.get(&field_name).unwrap()));

					// TODO: Must also implement good support for options?

					self.compile_type_prefixes(
						EncodingMode::Read,
						&f.typ.prefixes, &mut field_lines).unwrap();
					
					let mut field_var = LineBuilder::new();
					field_var.add(format!("let {} = ", field_name));
					field_var.append_inline(field_lines);
					field_var.add_inline("?;");
					l.append(field_var);

					ctor += &format!("{}, ", field_name);
				}

				l.nl();

				ctor.pop();
				if ctor.chars().last().unwrap() == ',' {
					ctor.pop();
				}
				ctor += " })";
				l.add(ctor);
			});
			l.add("})");

			self.compile_type_prefixes(EncodingMode::Read, prefixes, l).unwrap();
		});
		lines.nl();

		impl_der_writeable(&mut lines, name, |l| {
			// TODO: Just changing this line will allow writing a SET as
			// well.
			l.add("w.write_sequence(|w| {");
			
			// TODO: If we are compiling a SET, we should pre-sort the 
			// writes by tag so that it is cheaper to read/write.
			l.indented(|l| {
				for f in fields.iter() {
					let mut field_lines = LineBuilder::new();
					let field_name = Self::field_name(f.name.as_ref());

					// TODO: Should also implement any special prefixed for
					// this field or constraints
					field_lines.add(format!("self.{}.write_der(w);",
									field_name));

					self.compile_type_prefixes(
						EncodingMode::Write,
						&f.typ.prefixes, &mut field_lines).unwrap();
					
					l.append(field_lines);
				}
			});
			l.add("});");

			// TODO: REfactor out these unwraps
			self.compile_type_prefixes(EncodingMode::Write, prefixes, l).unwrap();
		});

		// Place the module after the struct
		if !outer.empty() {
			lines.nl();
			outer.wrap_module(&modname);
			lines.append(outer);
		}

		Ok(lines)
	}

	/// Top-level assignments that are not a struct/enum compatible type will
	/// be wrapped in a struct with a single field.
	fn compile_wrapped_type(&mut self, name: &str, typ: &Type)
	-> Result<LineBuilder> {
		let mut l = LineBuilder::new();

		let modname = name.to_ascii_lowercase();
		let inner_ctx = Context::new().inner(name);

		let (typename, mut outer) = self.compile_type(
			"Value", typ, &inner_ctx)?;

		l.add(format!("pub struct {} {{", name));
		l.add(format!("\tvalue: {}", typename));
		l.add("}");
		l.nl();

		l.add(format!("impl Into<{}> for {} {{", typename, name));
		l.add(format!("\tfn into(self) -> {} {{", typename));
		l.add("\t\tself.value");
		l.add("\t}");
		l.add("}");
		l.nl();

		l.add(format!("impl From<{}> for {} {{", typename, name));
		l.add(format!("\tfn from(value: {}) -> Self {{", typename));
		l.add("\t\tSelf { value }");
		l.add("\t}");
		l.add("}");
		l.nl();

		// TODO: Implement Deref and DerefMut and AsRef() and AsMut

		// TODO: Implement write

		// TODO: Implement read

		if !outer.empty() {
			l.nl();
			outer.wrap_module(&modname);
			l.append(outer);
		}

		Ok(l)
	}

	/// NOTE: It is the caller's role to setup the constraints
	fn compile_type(&mut self, name: &str, typ: &Type, ctx: &Context)
	-> Result<(String, LineBuilder)> {

		let mut lines = LineBuilder::new();
		let name = match &typ.desc {
			TypeDesc::Builtin(t) => {
				String::from(match t {
					BuiltinType::Boolean => "bool".to_string(),
					BuiltinType::Integer(t) => {
						if let Some(vals) = &t.values {
							lines = self.compile_int_enum(name, vals)?;
							ctx.resolve(name)
						} else {
							"isize".to_string()
						}

					},
					BuiltinType::ObjectIdentifier => "ObjectIdentifier".to_string(),
					BuiltinType::CharacterString(t) => {
						match t {
							CharacterStringType::Restricted(t) => t.typename().to_string(),
							CharacterStringType::Unrestricted => "Bytes".to_string()
						}
					},
					BuiltinType::OctetString => "OctetString".into(),
					BuiltinType::Sequence(t) | BuiltinType::Set(t) => {
						// TODO: Should compile_struct use the un-converted
						// name to generate the module name.
						let tname = Self::type_name(&name);

						lines.append(
							self.compile_struct(&tname, &typ.prefixes,
												&t.types, ctx)?
						);

						ctx.resolve(&tname)
					},
					BuiltinType::SequenceOf(t) | BuiltinType::SetOf(t) => {
						let modname = name.to_ascii_lowercase();
						let inner_ctx = ctx.inner(&modname);

						let (s, mut l) = self.compile_collection_type(
							t, &inner_ctx)?;

						if !l.empty() {
							l.wrap_module(&modname);
							lines.append(l);
						}

						// TODO: Should I be resolving this?
						format!("SequenceOf<{}>", s)
					},
					BuiltinType::BitString(t) => {
						// TODO: A BitString with named bits will need to be 
						"BitString".into()
					},
					BuiltinType::Any(_) => {
						// TODO: Handle any DEFINED BY constraints.
						"Any".into()
					},
					_ => {
						"TODO2".to_string()
						// return Err(format!("Unsupported built-in {:?}", t).into())
					}
				})
			},
			TypeDesc::Referenced(s) => s.to_string()
		};

		Ok((name, lines))
	}


	fn compile_value_assign(&mut self, assign: &ValueAssignment) -> Result<LineBuilder> {
		if assign.typ.constraints.len() != 0 {
			return Err("Constraints not supported in value assignments".into());
		}

		let name = Self::value_name(assign.name.as_ref());
		let modname = name.to_ascii_lowercase();
		let ctx = Context::new().inner(&modname);
		let (typename, mut l) = self.compile_type(
			&name, &assign.typ, &ctx)?;
		
		let mut lines = LineBuilder::new();

		lines.add(format!("pub const {}: {} = {};",
			name, typename, self.compile_value(&assign.value)?));

		if !l.empty() {
			lines.nl();
			l.wrap_module(&modname);
			lines.append(l);
		}

		Ok(lines)
	}

	fn compile_value(&mut self, value: &Value) -> Result<String> {
		Ok(match value {
			Value::Builtin(v) => {
				match v {
					BuiltinValue::Boolean(v) => {
						String::from(if *v {
							"true"
						} else {
							"false"
						})
					},
					BuiltinValue::Integer(v) => {
						match v {
							IntegerValue::SignedNumber(v) => {
								v.to_string()
							},
							IntegerValue::Identifier(name) => {
								name.to_string()
							}
						}
					},
					BuiltinValue::ObjectIdentifier(v) => {
						let mut s = String::from("ObjectIdentifier::new()");
						for c in &v.components {
							match c {
								// TODO: This is only valid if it is the first
								// component and refers to another absolute oid.
								ObjectIdentifierComponent::Name(n) => {
									s += &format!(".extend({})",
										Self::value_name(n.as_ref()));
								},
								ObjectIdentifierComponent::NameAndNumber(_, v) |
								ObjectIdentifierComponent::Number(v) => {
									s += &format!(".extend(&[{}])", v);
								}
							}
						}

						s
					},
					_ => { return Err(format!("Failed {:?}", v).into()); }
				}
			},
			_ => { return Err("failed 2".into()); }
		})
	}

	// TODO: Also implement enumerated.

	fn compile_choice(&mut self, name: &str, prefixes: &[TypePrefix],
					  choice: &ChoiceType, ctx: &Context)
	-> Result<LineBuilder> {

		let modname = name.to_ascii_lowercase();
		let inner_ctx = ctx.inner(&modname);

		let mut lines = LineBuilder::new();
		let mut outer = LineBuilder::new();

		lines.add(format!("pub enum {} {{", name));

		for t in &choice.types.types {
			let cname = Self::case_name(&t.name.to_string());
			
			let (typ, l) = self.compile_type(&cname, &t.typ, &inner_ctx)?;

			lines.add(format!("\t{}({}),", cname, typ));
			outer.append(l);
		}

		lines.add("}");
		lines.nl();

		impl_der_readable(&mut lines, name, |l| {
			// TODO:
		});
		lines.nl();

		impl_der_writeable(&mut lines, name, |l| {
			l.add("w.write_choice(|w| {");
			l.indented(|l| {
				l.add("match self {");
				l.indented(|l| {
					for t in &choice.types.types {
						l.add(format!("{}::{}(v) => {{",
										name,
										Self::case_name(t.name.as_ref())));

						// Implement the usual type reading stuff here.
						// TODO: Redundant with the compile_struct
						let mut field_lines = LineBuilder::new();

						// TODO: Should also implement any special prefixed for
						// this field or constraints
						field_lines.add("v.write_der(w);");

						self.compile_type_prefixes(
							EncodingMode::Write,
							&t.typ.prefixes, &mut field_lines).unwrap();
						
						field_lines.indent();
						l.append(field_lines);


						l.add("},");
					}
				});
				l.add("};");
			});
			l.add("});");

			self.compile_type_prefixes(EncodingMode::Write, prefixes, l).unwrap();
		});

		if !outer.empty() {
			lines.nl();
			outer.wrap_module(&modname);
			lines.append(outer);
		}

		Ok(lines)
	}

	// fn compile_component_fields(&mut self, list: &[&ComponentField],
	// 	ctx: &Context) -> Result<(LineBuilder, LineBuilder)> {
	// 	let mut out = LineBuilder::new();
	// 	let mut outer = LineBuilder::new();
	// 	for f in list {
	// 		let name = Self::field_name(f.name.as_ref());
	// 		let (mut typename, l) = self.compile_type(&name, &f.typ, ctx)?;

	// 		if let ComponentMode::Optional = f.mode {
	// 			typename = format!("Option<{}>", typename);
	// 		}

	// 		out.add(format!("\tpub {}: {},", name, typename));
	// 		outer.append(l);
	// 	}

	// 	Ok((out, outer))
	// }

	fn compile_type_assign(&mut self, a: &TypeAssignment) -> Result<LineBuilder> {

		let name = a.name.as_ref();
		// let modname = name.to_ascii_lowercase();
		// let ctx = Context::new().inner(&modname);


		if let TypeDesc::Builtin(t) = &a.typ.desc {
			if let BuiltinType::Choice(c) = t {
				return self.compile_choice(
					name, &a.typ.prefixes, &c, &Context::new()
				);
			}
			else if let BuiltinType::Sequence(t) = t {
				return self.compile_struct(
					a.name.as_ref(), &a.typ.prefixes, &t.types,
					&Context::new()
				);
			}
			else if let BuiltinType::Set(t) = t {
				// TODO: Separate flag for SET and SEQUENCE
				return self.compile_struct(
					a.name.as_ref(), &a.typ.prefixes, &t.types,
					&Context::new()
				);
			}
			else if let BuiltinType::Integer(t) = t {
				// TODO: This doesn't support prefixed and constraints.
				if let Some(vals) = &t.values {
					return self.compile_int_enum(name, vals)
				}
			}

			// TODO: Enumerated


			// TODO: SET/SEQUENCE OF should be implemented as wrapped types.
		}

		// Because any other type may have constraints/prefixes (or may have 
		// constraints/prefixes in the future). We avoid doing
		// 'pub type Name = Type' and instead wrap the value in a struct.
		// TODO: Pass in constraints and prefixes if any.
		self.compile_wrapped_type(name, &a.typ)

		// match &a.typ.desc {
		// 	TypeDesc::Referenced(n) => {
		// 		lines.add(format!("pub type {} = {};", name, n.to_string()));
		// 	},
		// 	TypeDesc::Builtin(t) => {
		// 		match t {
		// 			BuiltinType::Choice(c) => {
						
		// 			},
		// 			BuiltinType::Sequence(t) => {						
						
		// 			},
		// 			BuiltinType::CharacterString(t) => {
		// 				// NOTE: compile_type here should not produce any extra
		// 				// lines.
		// 				lines.add(format!("pub type {} = {};", name,
		// 					self.compile_type("", &a.typ, &Context::new())?.0));
		// 			},
		// 			_ => {
		// 				lines.add("TODO");	
		// 			}
		// 		}
		// 	}
		// };

	}

	pub fn compile(&mut self, file: Bytes) -> Result<String> {
		let (module, _) = complete(ModuleDefinition::parse)(file)?;
		let mut lines = LineBuilder::new();

		lines.add("// AUTOGENERATED. DO NOT EDIT DIRECTLY.");
		lines.nl();
		
		// NOTE: None of these symbols will be allowed as typenames.
		lines.add("use std::convert::{From, Into};");
		lines.add("use crate::asn::builtin::*;");
		lines.add("use crate::asn::encoding::*;");
		lines.nl();

		// TODO: Step one should be to handle all imports and builtin imports.
		const skip_assignments: &'static [&'static str] = &[
			"UniversalString", "BMPString", "UTF8String"];

		let body = match module.body {
			Some(v) => v,
			_ => { return Ok(String::new()); }
		};

		for a in body.assignments {
			match a {
				Assignment::Value(value) => {
					lines.append(self.compile_value_assign(&value)?);
				},
				Assignment::Type(typ) => {
					if skip_assignments.iter().find(|v| **v == typ.name.as_ref()).is_some() {
						println!("Skip type assignment for: {}",
								 typ.name.as_ref());
						continue;
					}

					lines.append(self.compile_type_assign(&typ)?);
				}
			};
			lines.nl();
		}

		Ok(lines.to_string())
	}

}






#[cfg(test)]
mod tests {
	use super::*;

	use std::io::{Read, Write};

	#[test]
	fn asn1_compile_test() {
		

		let input_dir = "/home/dennis/workspace/dacha/pkg/crypto/src/x509/asn";

		for dirent in std::fs::read_dir(input_dir).unwrap() {
			let path = dirent.unwrap().path();
			println!("{:?}", path);
			// TODO: Use the extension method.
			if path.extension().unwrap_or(std::ffi::OsStr::new("")).to_str().unwrap() != "asn1" {
				continue;
			}

			println!("Read {:?}", path);
			let mut outpath = path.clone();
			outpath.set_extension("rs");
			println!("Write {:?}", outpath);

			let mut file = std::fs::File::open(path).unwrap();
			let mut data = vec![];
			file.read_to_end(&mut data).unwrap();

			let mut c = Compiler::new();
			let s = c.compile(Bytes::from(data)).unwrap();
			
			let mut outfile = std::fs::File::create(outpath).unwrap();
			outfile.write_all(s.as_bytes()).unwrap();
		}
		
	}
}
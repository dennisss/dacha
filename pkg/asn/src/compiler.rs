use common::errors::*;
use common::line_builder::*;
use parsing::*;
use bytes::Bytes;
use crate::tag::TagClass;
use crate::syntax::*;

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
		l.add("fn write_der(&self, w_: &mut DERWriter) {");
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
		l.add("fn read_der(r_: &mut DERReader) -> Result<Self> {");
		let ret = l.indented(|l| {
			f(l)
		});
		l.add("}");
		ret
	});
	lines.add("}");
	ret
}

fn impl_to_string<T, F: FnOnce(&mut LineBuilder) -> T>(
	lines: &mut LineBuilder, name: &str, f: F) -> T {
	lines.add(format!("impl ::std::string::ToString for {} {{", name));

	let ret = lines.indented(|l| {
		l.add("fn to_string(&self) -> String {");
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


use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;
use std::io::{Read, Write};

/// Multi-file compiler. Currently all files must be added before any individual
/// files get compiled.
pub struct Compiler {
	inner: Rc<RefCell<CompilerInner>>
}

struct CompilerInner {
	// TODO: Eventually we wil need to store absolute file paths for each source
	// in order to know how to handle resolves.
	files: std::collections::HashMap<String, CompilerFileEntry>
}

struct CompilerFileEntry {
	/// Path to the ASN file from which this entry originated.
	source: PathBuf,
	compiler: Rc<FileCompiler>
}

impl Compiler {
	fn new() -> Self {
		Self {
			inner: Rc::new(RefCell::new(CompilerInner {
				files: std::collections::HashMap::new()
			}))
		}
	}

	pub fn add(&mut self, path: PathBuf) -> Result<()> {
		// TODO: Integrate extension check into this?

		let mut file = std::fs::File::open(&path)?;

		let mut data = vec![];
		file.read_to_end(&mut data)?;

		let c = FileCompiler::create(data.into(), self.inner.clone())?;
		let name = c.module.ident.name.to_string();

		let mut inner = self.inner.borrow_mut();
		inner.files.insert(name, CompilerFileEntry {
			source: path,
			compiler: Rc::new(c)
		});

		Ok(())
	}

	/// Compiles all added files saving them back to disk.
	pub fn compile_all(&mut self) -> Result<()> {
		for (_name, entry) in self.inner.borrow().files.iter() {
			println!("Read {:?}", entry.source);
			let mut outpath = entry.source.clone();
			outpath.set_extension("rs");
			let outpath = outpath.to_str().unwrap().replace("-", "_");
			println!("Write {:?}", outpath);

			let compiled = entry.compiler.compile()?;
			
			let mut outfile = std::fs::File::create(outpath)?;
			outfile.write_all(compiled.as_bytes())?;
		}

		Ok(())
	}
}

/// Single file compiler
pub struct FileCompiler {
	module: ModuleDefinition,
	// output: String,
	// TODO: Must also consider automatic tagging.
	default_tagging: TagMode,

	parent: Rc<RefCell<CompilerInner>>
}

impl FileCompiler {
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

	fn compile_int_enum(&self, name: &str, variants: &[NamedNumber],
						is_enumerated: bool)
	-> Result<LineBuilder> {
		let mut lines = LineBuilder::new();
		let mut read_lines = LineBuilder::new();
		let mut write_lines = LineBuilder::new();
		lines.add(format!("#[derive(Debug, Clone, Copy, PartialEq)]"));
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
		let funcname = if is_enumerated { "enumerated" } else { "isize" }; 
		impl_der_readable(&mut lines, name, move |l| {
			l.add(format!("let v = r_.read_{}()?;", funcname));
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
			l.add(format!("w_.write_{}(v);", funcname));
		});

		Ok(lines)
	}

	fn compile_enumerated(&self, name: &str, typ: &EnumeratedType)
	-> Result<LineBuilder> {
		self.compile_int_enum(name, &typ.0.items, true)
	}

	fn compile_collection_type(&self, ctype: &CollectionType, ctx: &Context)
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

	fn compile_type_prefixes(&self, encmode: EncodingMode,
							 prefixes: &[TypePrefix], lines: &mut LineBuilder)
	-> Result<()> {

		for p in prefixes {
			match p {
				TypePrefix::Encoding(_) => {
					return Err(err_msg("Encoding prefix not supported"));
				},
				TypePrefix::Tag(t) => {
					if t.tag.encoding_ref.is_some() {
						return Err(err_msg(
							"Encoding reference in tag not supported"));
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

					let (verb, varname) = match encmode {
						EncodingMode::Read => ("r_.read", "r_"),
						EncodingMode::Write => ("w_.write", "w_")
					};

					let adj = match mode {
						TagMode::Explicit => "explicitly",
						TagMode::Implicit => "implicitly"
					};

					let start = format!("{}_{}(Tag {{ class: {}, number: {} }}, |{}| {{",
										verb, adj, class, num, varname);
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
	fn compile_struct(&self, name: &str, prefixes: &[TypePrefix],
					  types: &[ComponentType], ctx: &Context, is_set: bool)
					  -> Result<LineBuilder> {
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
					return Err(err_msg("COMPONENTS OF not supported"));
				}
			};
		}

		let mut lines = LineBuilder::new();

		// Create a module just for this struct if needed
		let modname = name.to_ascii_lowercase();
		let inner_ctx = ctx.inner(&modname);

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
				field_typenames.insert(name.clone(), typename.clone());

				// TODO: Optionals must be read/written as the outer most
				// constraint.
				if let ComponentMode::Optional = f.mode {
					typename = format!("Option<{}>", typename);
				}

				out.add(format!("\tpub {}: {},", name, typename));
				outer.append(l);
			}

			(out, outer)
		};

		lines.add(format!("#[derive(Debug, Clone)]"));
		lines.add(format!("pub struct {} {{", &name));
		lines.append(inner);
		lines.add("}");
		lines.nl();


		impl_der_readable(&mut lines, name, |l| {
			if is_set {
				l.add("r_.read_set(false, |r_| {");
			} else {
				l.add("r_.read_sequence(false, |r_| {");
			}
			l.indented(|l| {
				let mut ctor = String::from("Ok(Self { ");
				for f in fields.iter() {
					let mut field_lines = LineBuilder::new();
					let field_name = Self::field_name(f.name.as_ref());
					let typename = field_typenames.get(&field_name).unwrap();

					field_lines.add(format!("{}::read_der(r_)",
											typename.replace("<", "::<")));

					// TODO: Must also implement good support for options?

					// TODO: This is still a major problem.
					// TODO: This is problematic for things like structs in structs which should sometimes be handling their own prefixes.
					self.compile_type_prefixes(
						EncodingMode::Read,
						&f.typ.prefixes, &mut field_lines).unwrap();

					if let ComponentMode::Optional = f.mode {
						field_lines.indent();
						field_lines.wrap_with(
							"r_.read_option(|r_| {".into(), "})?;".into());
					} else if let ComponentMode::WithDefault(v) = &f.mode {
						field_lines.indent();
						println!("{:?}", v);

						let builtin_type = match &f.typ.desc {
							TypeDesc::Builtin(v) => v.clone(),
							TypeDesc::Referenced(v) => {
								self.lookup_type(v.as_ref()).unwrap()
							}
						};

						let mut valuec = self.compile_value(v, &typename, &f.typ).unwrap().0;
						if self.is_referenced_value(v, builtin_type.as_ref()) && !valuec.contains("{") {
							valuec = format!("(*{})", valuec);
						}

						// TODO: For all constants, we might as well pre-serialize them in the compiled version?

						field_lines.wrap_with(
							"r_.read_with_default(|r_| {".into(),
							format!("}}, {}.clone().into())?;",
									valuec));

						// format!("}})?.unwrap_or({}.clone().into());",
						//									valuec)

						// TODO: We can't use der_eq because things like 'bool' have different encodings possible (at least if we try to optimize it?
//						field_lines.add(format!("if der_eq(&{}, &{}) {{", field_name, valuec));
//						field_lines.add(format!("\treturn Err(\"DER got default value encoded for '{}'\".into());",
//												field_name));
//						field_lines.add("}");
					} else {
						field_lines.add_inline("?;");
					}

					let mut field_var = LineBuilder::new();
					field_var.add(format!("let {} = ", field_name));
					field_var.append_inline(field_lines);
					// field_var.add_inline("?;");
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
			// XXX: Yes
			if is_set {
				l.add("w_.write_set(|w_| {");
			} else {
				l.add("w_.write_sequence(|w_| {");
			}

			// TODO: If we are compiling a SET, we should pre-sort the
			// writes by tag so that it is cheaper to read/write.
			l.indented(|l| {
				for f in fields.iter() {
					let mut field_lines = LineBuilder::new();
					let field_name = Self::field_name(f.name.as_ref());
					let typename = field_typenames.get(&field_name).unwrap();

					// TODO: For items with default values, we must compare
					// to the default value to see if we should even bother
					// writing it.

					// TODO: Should also implement any special prefixed for
					// this field or constraints


					if let ComponentMode::Optional = &f.mode {
						field_lines.add("v.write_der(w_);");
					} else if let ComponentMode::WithDefault(v) = &f.mode {
						let builtin_type = match &f.typ.desc {
							TypeDesc::Builtin(v) => v.clone(),
							TypeDesc::Referenced(v) => {
								self.lookup_type(v.as_ref()).unwrap()
							}
						};

						let mut valuec = self.compile_value(v, &typename, &f.typ).unwrap().0;
//						if self.is_referenced_value(v, builtin_type.as_ref()) && !valuec.contains("{") {
//							valuec = format!("(*{})", valuec);
//						}

						field_lines.add(format!("if !der_eq(&self.{}, &{}) {{", field_name, valuec));
						// TODO: This line is the same as the default case.
						field_lines.add(format!("\tself.{}.write_der(w_);",
												field_name));
						field_lines.add("}");

					} else {
						field_lines.add(format!("self.{}.write_der(w_);",
												field_name));
					}

					self.compile_type_prefixes(
						EncodingMode::Write,
						&f.typ.prefixes, &mut field_lines).unwrap();

					if let ComponentMode::Optional = &f.mode {
						field_lines.indent();
						field_lines.wrap_with(
							format!("if let Some(v) = &self.{} {{", field_name),
							"}".into());
					}

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
	fn compile_wrapped_type(&self, name: &str, typ: &Type)
	-> Result<LineBuilder> {
		let mut l = LineBuilder::new();

		let modname = name.to_ascii_lowercase();
		let inner_ctx = Context::new().inner(&modname);

		let (typename, mut outer) = self.compile_type(
			"Value", typ, &inner_ctx)?;

		l.add(format!("#[derive(Debug, Clone)]"));
		l.add(format!("pub struct {} {{", name));
		l.add(format!("\tvalue: {}", typename));
		l.add("}");
		l.nl();

		// TODO: Must implement this for inline types as well
		if let TypeDesc::Builtin(t) = &typ.desc {
			if let BuiltinType::BitString(t) = t.as_ref() {
				if t.named_bits.len() > 0 {
					l.add(format!("impl {} {{", name));
					for bit in &t.named_bits {
						let v = match &bit.value {
							NamedBitValue::Immediate(v) => v.to_string(),
							NamedBitValue::Defined(v) =>
								Self::value_name(v.as_ref())
						};

						l.add(format!("\tpub fn {}(&self) -> Option<bool> {{",
									  bit.name.as_ref()));
						l.add(format!("\t\tself.value.get({}).map(|v| {{", v));
						l.add("\t\t\tif v == 1 { true } else { false }");
						l.add("\t\t})");
						l.add("}");
					}
					l.add("}");
					l.nl();
				}
			}
		}

		if self.can_to_string(&typ.desc) {
			impl_to_string(&mut l, name, |l| {
				l.add("self.value.to_string()");
			});
			l.nl();
		}

		l.add(format!("impl ::std::ops::Deref for {} {{", name));
		l.add(format!("\ttype Target = {};", typename));
		l.add("\tfn deref(&self) -> &Self::Target {");
		l.add("\t\t&self.value");
		l.add("\t}");
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

//		l.add(format!("impl ::std::cmp::PartialEq<{}> for {} {{", typename, name));
//		l.add(format!("\tfn eq(&self, other: &{}) -> bool {{", typename));
//		l.add("\t\t&self.value == other");
//		l.add("\t}");
//		l.add("}");
//		l.nl();

		{
			// TODO: Keep unwrapping incase it is multiple layers of nesting.
			// We should also unwrap anything that is a nested struct itself.
			let (reftype, use_ref) = if typename.starts_with("SequenceOf<") {
					(&typename.as_str()[11..(typename.len() - 1)], true)
				} else if typename.starts_with("SetOf<") {
					(&typename.as_str()[6..(typename.len() - 1)], true)
				} else {
					(typename.as_str(), false)
				};

			let reftype = if use_ref { format!("[{}]", reftype) } else { reftype.to_string() };

			l.add(format!("impl AsRef<{}> for {} {{", reftype, name));
			l.add(format!("\tfn as_ref(&self) -> &{} {{",
						  reftype));
			l.add(format!("\t\t&self.value{}",
						  if use_ref { ".as_ref()" } else { "" }));
			l.add("\t}");
			l.add("}");
			l.nl();
		}

		// If the inner

		// TODO: Implement Deref and DerefMut and AsRef() and AsMut

		impl_der_writeable(&mut l, name, |l| {
			l.add("self.value.write_der(w_);");
			self.compile_type_prefixes(EncodingMode::Write, &typ.prefixes, l).unwrap();
		});
		l.nl();

		// TODO: Implement read
		impl_der_readable(&mut l, name, |l| {
			l.add(format!("{}::read_der(r_).map(|value| Self {{ value }})",
				  typename.replace("<", "::<")));
			self.compile_type_prefixes(EncodingMode::Read, &typ.prefixes, l).unwrap();
		});
		l.nl();

		if !outer.empty() {
			l.nl();
			outer.wrap_module(&modname);
			l.append(outer);
		}

		Ok(l)
	}

	fn can_to_string_choice(&self, t: &ChoiceType) -> bool {
		for t in &t.types.types {
			if !self.can_to_string(&t.typ.desc) {
				return false;
			}
		}

		true
	}

	fn can_to_string(&self, desc: &TypeDesc) -> bool {
		// TODO: For referenced types, recursively look up if it can be
		// stringified (especially for CHOICE types).
		if let TypeDesc::Builtin(t) = desc {
			match t.as_ref() {
				BuiltinType::CharacterString(CharacterStringType::Restricted(_)) => {
					return true;
				},
				BuiltinType::Choice(t) => {
					return self.can_to_string_choice(t);
				}
				_ => {}
			}
		}

		return false;
	}

	/// NOTE: It is the caller's role to setup the constraints
	/// TODO: Instead of accepting a Type, accept only a BuiltinType after being resolved.
	fn compile_type(&self, original_name: &str, typ: &Type, ctx: &Context)
	-> Result<(String, LineBuilder)> {

		let tname = Self::type_name(original_name);

		let mut lines = LineBuilder::new();
		let name = match &typ.desc {
			TypeDesc::Builtin(t) => {
				String::from(match t.as_ref() {
					BuiltinType::Boolean => "bool".to_string(),
					BuiltinType::Integer(t) => {
						if let Some(vals) = &t.values {
							lines = self.compile_int_enum(
								&tname, vals, false)?;
							ctx.resolve(&tname)
						} else {
							"BigInt".to_string()
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
					BuiltinType::Sequence(t) => {
						// TODO: Should compile_struct use the un-converted
						// name to generate the module name.
						let tname = Self::type_name(&original_name);

						lines.append(
							self.compile_struct(&tname, &typ.prefixes,
												&t.types, ctx, false)?
						);

						ctx.resolve(&tname)
					},
					BuiltinType::Set(t) => {
						// TODO: Dedup with Sequence case above.
						let tname = Self::type_name(&tname);

						lines.append(
							self.compile_struct(&tname, &typ.prefixes,
												&t.types, ctx, true)?
						);

						ctx.resolve(&tname)
					},
					BuiltinType::SequenceOf(t) => {
						let modname = tname.to_ascii_lowercase();
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
					BuiltinType::SetOf(t) => {
						// TODO: Dedup with SequenceOf code above
						let modname = tname.to_ascii_lowercase();
						let inner_ctx = ctx.inner(&modname);

						let (s, mut l) = self.compile_collection_type(
							t, &inner_ctx)?;

						if !l.empty() {
							l.wrap_module(&modname);
							lines.append(l);
						}

						// TODO: Should I be resolving this?
						format!("SetOf<{}>", s)
					},
					BuiltinType::BitString(t) => {
						// TODO: A BitString with named bits will need to be 
						"BitString".into()
					},
					BuiltinType::Any(_) => {
						// TODO: Handle any DEFINED BY constraints.
						"Any".into()
					},
					BuiltinType::Null => {
						"Null".into()
					},
					// BuiltinType::Enumerated(t) => {
						// TODO:
					// },
					_ => {
						"TODO2".to_string()
						// return Err(format_err!("Unsupported built-in {:?}", t))
					}
				})
			},
			TypeDesc::Referenced(s) => s.to_string().replace("-", "_")
		};

		Ok((name, lines))
	}

	fn is_referenced_value(&self, val: &Value, typ: &BuiltinType) -> bool {
		if let BuiltinType::Integer(t) = typ {
			if t.values.is_some() {
				// In this case, no cloning is required as our enums should
				// always implement Copy and not be wrapping in lazy_static.
				return false;
			}
		}

		match val {
			Value::Referenced(_) => true,
			Value::Builtin(v) => {
				match v.as_ref() {
					BuiltinValue::Integer(IntegerValue::Identifier(_)) => {
						// TODO: This is only because we parse it wrong and even then it
						// only applies if we aren't referencing a specific case.
						true
					},
					_ => false
				}
			},
			_ => false
		}
	}


	fn compile_value_assign(&self, assign: &ValueAssignment) -> Result<LineBuilder> {
		// TODO: We should check thee statically, but no need to do anything
		// dynamically.
		// if assign.typ.constraints.len() != 0 {
		// 	return Err(err_msg("Constraints not supported in value assignments"));
		// }

		let name = Self::value_name(assign.name.as_ref());
		let modname = name.to_ascii_lowercase();
		let ctx = Context::new().inner(&modname);
		let (typename, mut l) = self.compile_type(
			&name, &assign.typ, &ctx)?;
		
		let mut lines = LineBuilder::new();

		// TODO: Into is mainly only needed if it isn't a trivial type.

		let (value, is_complex) = self.compile_value(&assign.value, &typename, &assign.typ)?;

		let wrapped_value = if let TypeDesc::Referenced(r) = &assign.typ.desc {

			let builtin_type = self.lookup_type(r.as_ref())?;

			// TODO: Instead need a consistent function for checking if a type
			// will get wrapped.
			match builtin_type.as_ref() {
				BuiltinType::Sequence(_) | BuiltinType::Set(_) => {
					value
				},
				_ => {
					let mut valuec = value;

					if self.is_referenced_value(&assign.value, builtin_type.as_ref()) {
						valuec = format!("(*{}).clone()", valuec);
					}

					// TODO: This is only applicable when we have simple types that have
					// been wrapped.
					format!("{} {{ value: {} }}", typename, valuec)
				}
			}
		} else {
			value
		};

		// let suffix = if typename.chars().next().unwrap().is_ascii_uppercase() {
		// 	".into()"
		// } else {
		// 	""
		// };


		let assign_line = format!("{}: {} = {};",
			name, typename, wrapped_value);
		if is_complex {
			lines.add("lazy_static! {");
			lines.add(format!("pub static ref {}", assign_line));
			lines.add("}");
		} else {
			lines.add(format!("pub const {}", assign_line));
		}

		if !l.empty() {
			lines.nl();
			l.wrap_module(&modname);
			lines.append(l);
		}

		Ok(lines)
	}

	// Must lookup 

	fn lookup_value_type(&self, name: &str) -> Result<Option<String>> {
		let body = self.module.body.as_ref().unwrap();
		for a in &body.assignments {
			if let Assignment::Value(v) = a {
				if v.name.as_ref() == name {
					match &v.typ.desc {
						TypeDesc::Referenced(v) => {
							return Ok(Some(v.to_string()));
						},
						TypeDesc::Builtin(_) => {
							return Ok(None);
						}
					}
				}
			}
		}

		for import in &body.imports {
			let parent = self.parent.borrow();
			let mc = &parent.files[&import.module.name.to_string()];
			if let Ok(v) = mc.compiler.lookup_value_type(name) {
				return Ok(v);
			}
		}

		Err(format_err!("Unknown value named: {}", name))
	}

	fn lookup_value(&self, name: &str) -> Result<Rc<BuiltinValue>> {
		let body = self.module.body.as_ref().unwrap();
		for a in &body.assignments {
			if let Assignment::Value(v) = a {
				if v.name.as_ref() == name {
					match v.value.as_ref() {
						Value::Referenced(name) => {
							// TODO: Prevent recursion.
							return self.lookup_value((name.0).0.as_ref());
						},
						Value::Builtin(v) => {
							return Ok(v.clone());
						}
					};
				}
			}
		}

		for import in &body.imports {
			let parent = self.parent.borrow();
			let mc = &parent.files[&import.module.name.to_string()];
			if let Ok(v) = mc.compiler.lookup_value(name) {
				return Ok(v);
			}
		}

		Err(format_err!("Unknown value named: {}", name))
	}

	// NOTE: This will look up the inner-most builtin type and will hide any
	// outer prefixes, constraints, etc. so should be used with caution.
	fn lookup_type(&self, name: &str) -> Result<Rc<BuiltinType>> {
		let body = self.module.body.as_ref().unwrap();
		for a in &body.assignments {
			if let Assignment::Type(t) = a {
				if t.name.as_ref() == name {
					match &t.typ.desc {
						TypeDesc::Builtin(t) => {
							return Ok(t.clone());
						},
						TypeDesc::Referenced(name) => {
							return self.lookup_type(name.as_ref());
						}
					}
				}
			}
		}

		// TODO: Must validate in cyclic loops in 

		for import in &body.imports {
			let parent = self.parent.borrow();
			let mc = &parent.files[&import.module.name.to_string()];
			if let Ok(v) = mc.compiler.lookup_type(name) {
				return Ok(v);
			}
		}


		Err(format_err!("Unknown type named: {}", name))
	}

	fn compile_oid_value(&self, v: &ObjectIdentifierValue) -> Result<Vec<usize>> {
		let mut items = vec![];
		for c in &v.components {
			match c {
				// TODO: This is only valid if it is the first
				// component and refers to another absolute oid.
				ObjectIdentifierComponent::Name(n) => {
					let builtin_val = self.lookup_value(n.as_ref())?;
					let val = match builtin_val.as_ref() {
						BuiltinValue::ObjectIdentifier(v) => v,
						_ => { return Err(err_msg("Type incompatible with oid")); }
					};
					// TODO: Prevent infinite recursions
					let inner = self.compile_oid_value(val)?;
					items.extend_from_slice(&inner);
				},
				ObjectIdentifierComponent::NameAndNumber(_, v) |
				ObjectIdentifierComponent::Number(v) => {
					items.push(*v);
				}
			}
		}

		Ok(items)
	}

	// Returns a string of the form 'b"..."' which has type &'static [u8] when
	// used in compiled code
	fn binary_string(data: &[u8]) -> String {
		let mut out = String::new();
		out.reserve(2*data.len());
		out.push_str("b\"");
		for b in data {
			if *b >= 32 && *b <= 126 {
				out.push(*b as char)
			} else {
				out.push_str(&format!("\\x{:02x}", b))
			}
		}
		out.push_str("\"");

		out
	}

	// TODO: Must validate that the value agrees with the type
	fn compile_value(&self, value: &Value, typename: &str, typ: &Type)
	-> Result<(String, bool)> {
		let builtin_type = match &typ.desc {
			TypeDesc::Builtin(v) => v.clone(),
			TypeDesc::Referenced(v) => {
				self.lookup_type(v.as_ref())?
			}
		};

		let is_any =
			if let BuiltinType::Any(_) = builtin_type.as_ref() {
				true
			} else {
				false
			};

		// TODO: When we are a reference to a type which isn't the same as the
		// type itself, then we will need to wrap the value.

		// Lookup current type completely,
		// Then lookup other type completely.
		// - If there is a discrepency, we are probably a wrapped type.

		let mut is_complex = false;

		let mut out = match value {
			Value::Builtin(v) => {
				match v.as_ref() {
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
								// TODO: Use BigInt to do the conversion.

								format!("BigInt::from_le_static(&[{}])", v)
							},
							// TODO: We will never see this as we never 
							// TODO: This depends on the type. If it has a named
							// value list, then we should prioritize those
							IntegerValue::Identifier(name) => {
								if let BuiltinType::Integer(t)
									= builtin_type.as_ref() {
									if t.values.is_some() {
										return Ok((format!("{}::{}", typename, name.to_string()), is_complex));
									}
								}

								let mut vname = Self::value_name(name.as_ref());

								let value_type = self.lookup_value_type(
									name.as_ref())?;

								if let Some(tname) = value_type {
									if &tname != typename && !is_any {
										if self.is_referenced_value(
											value, builtin_type.as_ref()) {
											vname = format!("(*{}).clone()", vname)
										}

										return Ok((format!("{} {{ value: {} }}", typename, vname), is_complex));
									} 
								}

								// TODO: Need to format as a value/constant name
								vname
							}
						}
					},
					BuiltinValue::ObjectIdentifier(v) => {
						let items = self.compile_oid_value(v)?;
						format!("oid![{}]",
								items.into_iter().map(|v| v.to_string())
									.collect::<Vec<_>>().join(","))
					},
					BuiltinValue::OctetString(v) => {
						let val = match v {
							OctetStringValue::Hex(v) => {
								Self::binary_string(v.as_ref())
							},
							OctetStringValue::Bits(v) => {
								Self::binary_string(v.as_ref())
							}
						};

						format!("OctetString::from_static({})", val)
					},
					BuiltinValue::Null => {
						"Null::new()".to_string()
					},
					BuiltinValue::Sequence(SequenceValue(v)) => {
						let mut out = format!("{} {{\n", typename);

						let body = match builtin_type.as_ref() {
							BuiltinType::Set(v) | BuiltinType::Sequence(v) => {
								v
							},
							_ => { return Err(err_msg("Sequence value for non-sequence/set type.")); }
						};

						let mut fields = std::collections::HashMap::new(); 

						// TODO: Must also handle optional types.
						for comp in &body.types {
							match comp {
								ComponentType::Field(f) => {
									fields.insert(f.name.to_string(), f);
								},
								_ => {}
							}
						}


						for named in v {
							let field = fields.get(&named.name.as_ref().to_string())
								.ok_or(err_msg("No such field"))?;
							
							let field_tname = match &field.typ.desc {
								TypeDesc::Referenced(name) => name.as_ref(),
								_ => "TODO4"
							};

							let (mut val, is_c) = self.compile_value(
								&named.value, field_tname, &field.typ)?;
							is_complex |= is_c;

							if let ComponentMode::Optional = &field.mode {
								val = format!("Some({})", val);
							}

							is_complex = true;

							// TODO: Clone is only needed if it is a referenced
							// value and not an immediate.
							out += &format!("\t{}: {}.clone(),\n",
								Self::field_name(named.name.as_ref()),
								val);
						}
						out += "}";
						out
					},
					_ => {
						println!("{:#?}", v);
						return Err(format_err!("Failed {:?}", v)); }
				}
			},
			_ => { return Err(err_msg("failed 2")); }
		};

		if let BuiltinType::Any(_) = builtin_type.as_ref() {
			out = format!("asn_any!({})", out);
		}

		Ok((out, is_complex))
	}

	// TODO: Also implement enumerated.

	fn compile_choice(&self, name: &str, prefixes: &[TypePrefix],
					  choice: &ChoiceType, ctx: &Context)
	-> Result<LineBuilder> {

		let modname = name.to_ascii_lowercase();
		let inner_ctx = ctx.inner(&modname);

		let mut lines = LineBuilder::new();
		let mut outer = LineBuilder::new();

		lines.add(format!("#[derive(Debug, Clone)]"));
		lines.add(format!("pub enum {} {{", name));

		let mut typenames = vec![];

		for t in &choice.types.types {
			let cname = Self::case_name(&t.name.to_string());
			
			let (typ, l) = self.compile_type(&cname, &t.typ, &inner_ctx)?;

			typenames.push(typ.clone());

			lines.add(format!("\t{}({}),", cname, typ));
			outer.append(l);
		}

		lines.add("}");
		lines.nl();

		if self.can_to_string_choice(choice) {
			impl_to_string(&mut lines, name, |l| {
				l.add("match self {");
				l.indented(|l| {
					for t in &choice.types.types {
						l.add(format!("{}::{}(v) => v.to_string(),",
										name,
										Self::case_name(t.name.as_ref())));
					}
				});
				l.add("}");
			});
			lines.nl();
		}

		impl_der_readable(&mut lines, name, |l| {
			l.add("r_.read_choice(|r_| {");
			l.indented(|l| {
				for (t, tname) in choice.types.types.iter().zip(typenames.iter()) {
					l.add("{");
					l.indented(|l| {
						l.add("let v = r_.read_option(|r_| {");
						l.indented(|l| {
							l.add(format!("{}::read_der(r_)",
										  tname.replace("<", "::<")));
							self.compile_type_prefixes(EncodingMode::Read, &t.typ.prefixes, l).unwrap();
						});
						l.add("})?;");
						l.nl();
						l.add("if let Some(v) = v {");
						l.add(format!("\treturn Ok({}::{}(v));",
								name,
							  Self::case_name(t.name.as_ref())));
						l.add("}");
					});
					l.add("}");
				}

				l.nl();
				l.add("Err(\"No matching choice type\".into())")
			});
			l.add("})");
		});
		lines.nl();

		impl_der_writeable(&mut lines, name, |l| {
			l.add("w_.write_choice(|w_| {");
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
						field_lines.add("v.write_der(w_);");

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

	// fn compile_component_fields(&self, list: &[&ComponentField],
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

	fn compile_type_assign(&self, a: &TypeAssignment) -> Result<LineBuilder> {

		let name = a.name.as_ref().replace("-", "_");
		// let modname = name.to_ascii_lowercase();
		// let ctx = Context::new().inner(&modname);

		if let TypeDesc::Builtin(t) = &a.typ.desc {
			if let BuiltinType::Choice(c) = t.as_ref() {
				return self.compile_choice(
					&name, &a.typ.prefixes, &c, &Context::new()
				);
			}
			else if let BuiltinType::Sequence(t) = t.as_ref() {
				return self.compile_struct(
					&name, &a.typ.prefixes, &t.types,
					&Context::new(), false
				);
			}
			else if let BuiltinType::Set(t) = t.as_ref() {
				// TODO: Separate flag for SET and SEQUENCE
				return self.compile_struct(
					&name, &a.typ.prefixes, &t.types,
					&Context::new(), true
				);
			}
			else if let BuiltinType::Integer(t) = t.as_ref() {
				// TODO: This doesn't support prefixed and constraints.
				if let Some(vals) = &t.values {
					return self.compile_int_enum(&name, vals, false);
				}
			} else if let BuiltinType::Enumerated(t) = t.as_ref() {
				return self.compile_enumerated(&name, t);
			}

			// TODO: SET/SEQUENCE OF should be implemented as wrapped types.
		}

		// Because any other type may have constraints/prefixes (or may have 
		// constraints/prefixes in the future). We avoid doing
		// 'pub type Name = Type' and instead wrap the value in a struct.
		// TODO: Pass in constraints and prefixes if any.
		self.compile_wrapped_type(&name, &a.typ)


	}

	fn create(file: Bytes, parent: Rc<RefCell<CompilerInner>>) -> Result<Self> {
		let (module, _) = complete(ModuleDefinition::parse)(file)?;
		
		// TODO: Is the default automatic?
		let default_tagging =
			match module.default_mode.clone().unwrap_or(TagDefault::Explicit) {
				TagDefault::Explicit => TagMode::Explicit,
				TagDefault::Implicit => TagMode::Implicit,
				TagDefault::Automatic => {
					return Err(err_msg("Automatic tagging not supported"));
				}
			};
		
		Ok(Self {
			module, parent, default_tagging
		})
	}

	pub fn compile(&self) -> Result<String> {
		// let (module, _) = complete(ModuleDefinition::parse)(file)?;
		let mut lines = LineBuilder::new();

		lines.add("// AUTOGENERATED. DO NOT EDIT DIRECTLY.");
		lines.nl();
		
		// NOTE: None of these symbols will be allowed as typenames.
		lines.add("use ::std::convert::{From, Into};");
		lines.add("use ::common::errors::*;");
		lines.add("use ::asn::builtin::*;");
		lines.add("use ::asn::encoding::*;");
		lines.add("use ::math::big::BigInt;");

		// TODO: Step one should be to handle all imports and builtin imports.
		const skip_assignments: &'static [&'static str] = &[
			"UniversalString", "BMPString", "UTF8String"];

		let body = match &self.module.body {
			Some(v) => v,
			_ => { return Ok(String::new()); }
		};

		// TODO: Only import the specified symbols (exluding any in
		// skip_assignments)
		for s in &body.imports {
			lines.add(format!("use super::{}::*;",
							  s.module.name.as_ref().replace("-", "_")));
		}

		// TODO: Exports should define whether we use 'pub' in assignments.

		// 
		lines.nl();

		for a in &body.assignments {
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

		let mut compiler = Compiler::new();

		for dirent in std::fs::read_dir(input_dir).unwrap() {
			let path = dirent.unwrap().path();
			println!("{:?}", path);
			// TODO: Use the extension method.
			if path.extension().unwrap_or(std::ffi::OsStr::new("")).to_str().unwrap() != "asn1" {
				continue;
			}

			compiler.add(path).unwrap();
		}

		println!("Compiling all...");
		compiler.compile_all().unwrap();
		
	}
}
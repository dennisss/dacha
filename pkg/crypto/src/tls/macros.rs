#[macro_export]
macro_rules! tls_enum_u8 {
	($name:ident => { $( $case:ident ( $val:expr ) ),* , (255) }) => {
		#[derive(Clone, Copy, Debug, PartialEq, Eq)]
		pub enum $name {
			$(
				$case,
			)*
			Unknown(u8)
		}

		impl $name {
			pub fn to_u8(&self) -> u8 {
				match self {
					$(
						$name::$case => $val,
					)*
					$name::Unknown(v) => *v
				}
			}

			pub fn from_u8(v: u8) -> Self {
				match v {
					$(
						$val => $name::$case,
					)*
					_ => $name::Unknown(v)
				}
			}

			parser!(pub parse<Self> => {
				map(as_bytes(be_u8), |v| Self::from_u8(v))
			});

			pub fn serialize(&self, out: &mut ::std::vec::Vec<u8>) {
				out.push(self.to_u8());
			}
		}
	};
}

#[macro_export]
macro_rules! tls_struct {
	($name:ident => { $( $typ:ident $field:ident );* ; }) => {
		#[derive(Debug, Clone)]
		pub struct $name {
			$(
				pub $field: $typ,
			)*
		}

		impl $name {
			parser!(pub parse<Self> => { seq!(c => {
				$(
					let $field = c.next($typ::parse)?;
				)*

				Ok(Self { $( $field, )* })
			}) });

			pub fn serialize(&self, out: &mut ::std::vec::Vec<u8>) {
				$(
					self.$field.serialize(out);
				)*
			}
		}
	};
}

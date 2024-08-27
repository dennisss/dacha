#[macro_export]
macro_rules! define_command_enum {
    ($( $name:ident ),*) => {
      #[derive(Clone, Debug)]
      pub enum Command {
        $( $name($name), )*
      }

      impl CommandCodec for Command {
        fn from_command_words(
            command: CommandWord,
            params: &mut LineParameters,
        ) -> Result<Self> {
            // TODO: Debug assert that none of the commands overlap between the command types.

            match command {
                $(
                    $name::COMMAND => {
                        Ok(Self::$name($name::from_command_words(command, params)?))
                    }
                )*
                _ => Err(format_err!("Unknown command: {}", command.to_string()))
            }
        }

        fn to_command_words(&self, out: &mut Vec<Word>) {
            match self {
                $(
                    Self::$name(v) => v.to_command_words(out),
                )*
            }
        }
      }
    };
}

#[macro_export]
macro_rules! define_command {
    ($(
        #[$meta:meta])*
        pub struct $name:ident ($command_word:literal) {
            $( $(#[$field_meta:meta])* $field:ident ($key:literal): $typ:ty ),*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $name {
            $(
                $(#[$field_meta])*
                pub $field: $typ,
            )*
        }

        impl $name {
            pub const COMMAND: CommandWord = command_word!($command_word);
        }

        impl CommandCodec for $name {

            fn from_command_words(
                command: CommandWord,
                params: &mut LineParameters,
            ) -> Result<Self> {
                // TODO: Check the command.
                if command != Self::COMMAND {
                    return Err(err_msg("from_words called for wrong command"));
                }

                Self::from_param_words('-', params)
            }

            fn to_command_words(&self, out: &mut Vec<Word>) {
                out.push(Word {
                    key: Self::COMMAND.group,
                    value: WordValue::RealValue(Self::COMMAND.number)
                });

                self.to_param_words('-', out);
            }
        }

        impl FromWords for $name {
            fn from_param_words(key: char, params: &mut LineParameters) -> Result<Self> {
                // NOTE: 'key' is unused

                $(
                    let $field = <$typ as FromWords>::from_param_words($key, params)
                        .map_err(|e| format_err!("Parsing param '{}': {}", $key, e))?;
                )*

                Ok(Self {
                    $(
                        $field,
                    )*
                })
            }
        }

        impl ToWords for $name {
            fn to_param_words(&self, key: char, out: &mut Vec<Word>) {
                // NOTE: 'key' is unused.
                $(
                    self.$field.to_param_words($key, out);
                )*
            }

        }
    };
}

#[macro_export]
macro_rules! define_unparsed_command {
    ($(#[$meta:meta])* pub struct $name:ident ($command_word:literal)) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $name {
            pub words: Vec<Word>
        }

        impl $name {
            pub const COMMAND: CommandWord = command_word!($command_word);
        }

        impl CommandCodec for $name {

            fn from_command_words(
                command: CommandWord,
                params: &mut LineParameters,
            ) -> Result<Self> {
                // TODO: Check the command.
                if command != Self::COMMAND {
                    return Err(err_msg("from_words called for wrong command"));
                }

                let words = params.take_all()?;

                Ok(Self { words })
            }

            fn to_command_words(&self, out: &mut Vec<Word>) {
                out.push(Word {
                    key: Self::COMMAND.group,
                    value: WordValue::RealValue(Self::COMMAND.number)
                });

                for word in &self.words {
                    out.push(word.clone());
                }
            }
        }
    };
}

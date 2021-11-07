use common::args::*;
use common::errors::*;

/// Network port argument which pulls its value either from a command line flag
/// or from an environment variable.
///
/// Example:
/// Suppose we define our applications arguments as:
///   #[derive(Args)]
///   struct Args {
///       my_port: rpc_util::NamedPortArg
///   }
///
/// When we parse the arguments using common::args::parse(),
/// - If --my_port=NUMBER, we will populate my_port with the NUMBER value.
/// - if --my_port=some-identifier, we will try to find the numerical value of
///   the port from the environment variable named "PORT_SOME_IDENTIFIER".
pub struct NamedPortArg {
    value: u16,
}

impl NamedPortArg {
    pub fn value(&self) -> u16 {
        self.value
    }
}

impl ArgType for NamedPortArg {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self>
    where
        Self: Sized,
    {
        let s = match raw_arg {
            RawArgValue::String(s) => s,
            RawArgValue::Bool(_) => {
                return Err(err_msg("Invalid port value"));
            }
        };

        if let Ok(value) = s.parse::<u16>() {
            return Ok(Self { value });
        }

        let env_name = format!("PORT_{}", s.to_uppercase().replace("-", "_"));
        if let Ok(value) = std::env::var(env_name) {
            return Ok(Self {
                value: value.parse::<u16>()?,
            });
        }

        Err(format_err!("Can't find port with name: {}", s))
    }
}

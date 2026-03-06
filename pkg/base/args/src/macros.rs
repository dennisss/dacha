
#[macro_export]
macro_rules! define_arg_command {
    ($name:ident { $( $(#[$case_meta:meta])* $struct:ident = $val:expr),* $(,)? }) => {
        #[derive(Args)]
        pub enum $name {
            $(
                $(#[$case_meta])*
                #[arg(name = $val)]
                $struct($struct),
            )*
        }

        impl $name {
            pub async fn run(self) -> Result<()> {
                match self {
                    $(
                        Self::$struct(cmd) => cmd.run().await,
                    )*
                }
            }
        }
    }
}

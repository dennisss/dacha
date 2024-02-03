#[macro_export]
macro_rules! lock {
    ($var:ident <= $permit:expr, $e:expr) => {{
        let mut $var = $permit.enter();
        let ret = (|| $e)();
        $var.exit();
        ret
    }};
}

#[macro_export]
macro_rules! lock_async {
    ($var:ident <= $permit:expr, $e:expr) => {{
        let mut $var = $permit.enter();
        let ret = async { $e }.await;
        $var.exit();
        ret
    }};
}

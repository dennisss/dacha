pub struct TableId(pub u32);

#[macro_export]
macro_rules! table_id {
    ($num: expr) => {{
        $num
    }};
}

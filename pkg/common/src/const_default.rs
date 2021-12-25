#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;

pub trait ConstDefault {
    const DEFAULT: Self;
}

macro_rules! impl_const_default {
    ($t:ident, $v:expr) => {
        impl ConstDefault for $t {
            const DEFAULT: Self = $v;
        }
    };
}

impl_const_default!(bool, false);
impl_const_default!(u8, 0);
impl_const_default!(i8, 0);
impl_const_default!(u16, 0);
impl_const_default!(i16, 0);
impl_const_default!(u32, 0);
impl_const_default!(i32, 0);
impl_const_default!(u64, 0);
impl_const_default!(i64, 0);
impl_const_default!(usize, 0);
impl_const_default!(isize, 0);
impl_const_default!(f32, 0.0);
impl_const_default!(f64, 0.0);
#[cfg(feature = "alloc")]
impl_const_default!(String, String::new());

impl<T> ConstDefault for Option<T> {
    const DEFAULT: Self = None;
}

#[cfg(feature = "alloc")]
impl<T> ConstDefault for Vec<T> {
    const DEFAULT: Self = Vec::new();
}

macro_rules! impl_array {
    ($size:expr) => {
        impl<T: ConstDefault> ConstDefault for [T; $size] {
            const DEFAULT: Self = [T::DEFAULT; $size];
        }
    };
}

// TODO: Automate the generation of these with a macro
/*
for (let i = 1; i <= 256; i++) {
    console.log("impl_array!(" + i + ");");
}
*/
impl_array!(1);
impl_array!(2);
impl_array!(3);
impl_array!(4);
impl_array!(5);
impl_array!(6);
impl_array!(7);
impl_array!(8);
impl_array!(9);
impl_array!(10);
impl_array!(11);
impl_array!(12);
impl_array!(13);
impl_array!(14);
impl_array!(15);
impl_array!(16);
impl_array!(17);
impl_array!(18);
impl_array!(19);
impl_array!(20);
impl_array!(21);
impl_array!(22);
impl_array!(23);
impl_array!(24);
impl_array!(25);
impl_array!(26);
impl_array!(27);
impl_array!(28);
impl_array!(29);
impl_array!(30);
impl_array!(31);
impl_array!(32);
impl_array!(33);
impl_array!(34);
impl_array!(35);
impl_array!(36);
impl_array!(37);
impl_array!(38);
impl_array!(39);
impl_array!(40);
impl_array!(41);
impl_array!(42);
impl_array!(43);
impl_array!(44);
impl_array!(45);
impl_array!(46);
impl_array!(47);
impl_array!(48);
impl_array!(49);
impl_array!(50);
impl_array!(51);
impl_array!(52);
impl_array!(53);
impl_array!(54);
impl_array!(55);
impl_array!(56);
impl_array!(57);
impl_array!(58);
impl_array!(59);
impl_array!(60);
impl_array!(61);
impl_array!(62);
impl_array!(63);
impl_array!(64);
impl_array!(65);
impl_array!(66);
impl_array!(67);
impl_array!(68);
impl_array!(69);
impl_array!(70);
impl_array!(71);
impl_array!(72);
impl_array!(73);
impl_array!(74);
impl_array!(75);
impl_array!(76);
impl_array!(77);
impl_array!(78);
impl_array!(79);
impl_array!(80);
impl_array!(81);
impl_array!(82);
impl_array!(83);
impl_array!(84);
impl_array!(85);
impl_array!(86);
impl_array!(87);
impl_array!(88);
impl_array!(89);
impl_array!(90);
impl_array!(91);
impl_array!(92);
impl_array!(93);
impl_array!(94);
impl_array!(95);
impl_array!(96);
impl_array!(97);
impl_array!(98);
impl_array!(99);
impl_array!(100);
impl_array!(101);
impl_array!(102);
impl_array!(103);
impl_array!(104);
impl_array!(105);
impl_array!(106);
impl_array!(107);
impl_array!(108);
impl_array!(109);
impl_array!(110);
impl_array!(111);
impl_array!(112);
impl_array!(113);
impl_array!(114);
impl_array!(115);
impl_array!(116);
impl_array!(117);
impl_array!(118);
impl_array!(119);
impl_array!(120);
impl_array!(121);
impl_array!(122);
impl_array!(123);
impl_array!(124);
impl_array!(125);
impl_array!(126);
impl_array!(127);
impl_array!(128);
impl_array!(129);
impl_array!(130);
impl_array!(131);
impl_array!(132);
impl_array!(133);
impl_array!(134);
impl_array!(135);
impl_array!(136);
impl_array!(137);
impl_array!(138);
impl_array!(139);
impl_array!(140);
impl_array!(141);
impl_array!(142);
impl_array!(143);
impl_array!(144);
impl_array!(145);
impl_array!(146);
impl_array!(147);
impl_array!(148);
impl_array!(149);
impl_array!(150);
impl_array!(151);
impl_array!(152);
impl_array!(153);
impl_array!(154);
impl_array!(155);
impl_array!(156);
impl_array!(157);
impl_array!(158);
impl_array!(159);
impl_array!(160);
impl_array!(161);
impl_array!(162);
impl_array!(163);
impl_array!(164);
impl_array!(165);
impl_array!(166);
impl_array!(167);
impl_array!(168);
impl_array!(169);
impl_array!(170);
impl_array!(171);
impl_array!(172);
impl_array!(173);
impl_array!(174);
impl_array!(175);
impl_array!(176);
impl_array!(177);
impl_array!(178);
impl_array!(179);
impl_array!(180);
impl_array!(181);
impl_array!(182);
impl_array!(183);
impl_array!(184);
impl_array!(185);
impl_array!(186);
impl_array!(187);
impl_array!(188);
impl_array!(189);
impl_array!(190);
impl_array!(191);
impl_array!(192);
impl_array!(193);
impl_array!(194);
impl_array!(195);
impl_array!(196);
impl_array!(197);
impl_array!(198);
impl_array!(199);
impl_array!(200);
impl_array!(201);
impl_array!(202);
impl_array!(203);
impl_array!(204);
impl_array!(205);
impl_array!(206);
impl_array!(207);
impl_array!(208);
impl_array!(209);
impl_array!(210);
impl_array!(211);
impl_array!(212);
impl_array!(213);
impl_array!(214);
impl_array!(215);
impl_array!(216);
impl_array!(217);
impl_array!(218);
impl_array!(219);
impl_array!(220);
impl_array!(221);
impl_array!(222);
impl_array!(223);
impl_array!(224);
impl_array!(225);
impl_array!(226);
impl_array!(227);
impl_array!(228);
impl_array!(229);
impl_array!(230);
impl_array!(231);
impl_array!(232);
impl_array!(233);
impl_array!(234);
impl_array!(235);
impl_array!(236);
impl_array!(237);
impl_array!(238);
impl_array!(239);
impl_array!(240);
impl_array!(241);
impl_array!(242);
impl_array!(243);
impl_array!(244);
impl_array!(245);
impl_array!(246);
impl_array!(247);
impl_array!(248);
impl_array!(249);
impl_array!(250);
impl_array!(251);
impl_array!(252);
impl_array!(253);
impl_array!(254);
impl_array!(255);
impl_array!(256);

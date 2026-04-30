use optionize_macros::optionize;

#[optionize(name(OptionizedStruct))]
#[derive(Debug)]
struct MyStruct {
    field: i32,
    ofield: Option<i32>,
}

pub fn main() {
    let o = OptionizedStruct { field: Some(0), ofield: None };
    println!("{:?}", o);
}

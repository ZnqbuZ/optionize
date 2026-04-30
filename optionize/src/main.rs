use optionize_macros::optionize;

#[optionize(name = "Optionized{}")]
#[derive(Debug)]
struct MyStruct {
    field: i32,
    ofield: Option<i32>,
}

pub fn main() {
    let o = OptionizedMyStruct { field: Some(0), ofield: None };
    println!("{:?}", o);
}

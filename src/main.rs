use std::fs::File;

use serde_json::{Value, from_reader};


fn main() {
    let f = File::open("components.json").unwrap();
    let x: Value = from_reader(f).unwrap();
    println!("Hello, world! {:?}", x);
}

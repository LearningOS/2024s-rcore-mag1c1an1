use std::io::{Result, Write};
use std::fs::{File, read_dir};

fn main() {
    println!("cargo:rerun-if-changed=../ci-user/user/src/");
    println!("cargo:rerun-if-changed={}", TARGET_PATH);
}

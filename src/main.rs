mod fs;
use crate::fs::state::FileSystem;

fn main() {
    let test_path = "./test_disk.db";
    FileSystem::cli(test_path);
}

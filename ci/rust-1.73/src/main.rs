// Opens a CHD and reads its first sector, so the job proves the build links and runs.
fn main() {
    let path = std::env::args().nth(1).expect("a .chd path");
    let chd = libchdman_rs::Chd::open(&path, false, None).expect("open the CHD");
    let mut sector = vec![0u8; 512];
    chd.read_bytes(0, &mut sector).expect("read the first sector");
    println!("CHD v{}, {} bytes", chd.version(), chd.logical_bytes());
}

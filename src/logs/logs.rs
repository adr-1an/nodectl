pub fn verbose(v: bool, msg: &str) {
    if v {
        println!("[VERBOSE] {msg}")
    }
}

pub fn error(msg: &str) {
    println!("[ERROR] {msg}")
}

pub fn warn(msg: &str) {
    println!("[WARN] {msg}")
}

pub fn info(msg: &str) {
    println!("[INFO] {msg}")
}
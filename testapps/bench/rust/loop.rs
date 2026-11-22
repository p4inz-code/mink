fn main() {
    let mut s: i64 = 0;
    let n: i64 = std::hint::black_box(30_000_000);
    let mut i: i64 = 0;
    while i < n { s += i * i % 7; i += 1; }
    println!("{}", s);
}

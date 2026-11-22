fn main() {
    let n: usize = std::hint::black_box(2_000_000);
    let mut flags = vec![1i64; n + 1];
    let mut p = 2;
    while p * p <= n {
        if flags[p] == 1 { let mut m = p * p; while m <= n { flags[m] = 0; m += p; } }
        p += 1;
    }
    let mut count = 0;
    for i in 2..=n { if flags[i] == 1 { count += 1; } }
    println!("{}", count);
}

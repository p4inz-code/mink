fn main() {
    let (mut best, mut best_start) = (0i64, 0i64);
    let limit: i64 = std::hint::black_box(100_000);
    let mut start = 1;
    while start < limit {
        let (mut n, mut steps) = (start, 0i64);
        while n != 1 { if n % 2 == 0 { n /= 2 } else { n = 3 * n + 1 } steps += 1; }
        if steps > best { best = steps; best_start = start; }
        start += 1;
    }
    println!("{}", best_start);
    println!("{}", best);
}

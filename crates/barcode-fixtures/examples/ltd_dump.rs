//! Oracle harness: reads one 13-digit value per stdin line, prints the 46
//! run widths comma-separated. Compare against zint BARCODE_RSS_LTD renders
//! (tools/compare_ltd.py) — the fixtures encoder must be bit-exact.

fn main() {
    for line in std::io::stdin().lines() {
        let v: u64 = line.unwrap().trim().parse().expect("13-digit value");
        let runs = barcode_fixtures::databar_ltd_runs(v);
        let s: Vec<String> = runs.iter().map(|r| r.to_string()).collect();
        println!("{}", s.join(","));
    }
}

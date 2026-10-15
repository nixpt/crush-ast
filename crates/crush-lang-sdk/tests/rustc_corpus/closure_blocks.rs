fn main() {
    let add = |x: i64| {
        let y = x + 1;
        println!("step={}", y);
        y + 1
    };
    println!("{}", add(40));
    let choose = |x: i64| {
        if x > 0 {
            let y = x + 1;
            y
        } else {
            return 0;
        }
    };
    println!("{} {}", choose(41), choose(-1));
    let early = || return 42;
    println!("{}", early());
    let mut total = 0;
    let mut bump = || {
        total += 1;
        total
    };
    println!("{} {}", bump(), bump());
}

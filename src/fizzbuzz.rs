pub fn run() -> std::io::Result<()> {
    println!("Enter a positive whole number:");

    let mut input: String = String::new();
    let keyboard: std::io::Stdin = std::io::stdin();
    let read_result: std::io::Result<usize> = keyboard.read_line(&mut input);
    read_result.expect("Could not read input");

    let trimmed_input: &str = input.trim();
    let number_result: Result<u64, std::num::ParseIntError> = trimmed_input.parse();
    let upper_limit: u64 = number_result.expect("Please enter a whole number");

    if upper_limit == 0 {
        println!("Please enter a number greater than 0.");
        return Ok(());
    }

    for number in 1..=upper_limit {
        if number % 15 == 0 {
            println!("FizzBuzz");
        } else if number % 3 == 0 {
            println!("Fizz");
        } else if number % 5 == 0 {
            println!("Buzz");
        } else {
            println!("{number}");
        }
    }

    return Ok(());
}

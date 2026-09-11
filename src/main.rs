mod app;
mod fizzbuzz;
mod tui;

#[cfg(test)]
mod tests;

fn main() -> std::io::Result<()> {
    for argument in std::env::args().skip(1) {
        if argument == "--fizzbuzz" {
            return fizzbuzz::run();
        }
    }

    return app::run();
}

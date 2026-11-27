use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let mut output = io::stdout().lock();
    writeln!(output, "TC_PID:{}:END", std::process::id())?;
    output.flush()?;
    for line in io::stdin().lock().lines() {
        writeln!(output, "TC_RESULT:{}:END", line?)?;
        output.flush()?;
    }
    Ok(())
}

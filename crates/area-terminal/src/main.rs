//! Area Terminal - Standalone test binary

use area_terminal::Terminal;
use std::io::{self, Read};

fn main() -> anyhow::Result<()> {
    println!("Area Terminal Test");
    
    let mut term = Terminal::new(80, 24, "/bin/bash")?;
    
    println!("Terminal spawned. Press Ctrl+C to exit.");
    println!("Type commands and press Enter:");
    
    // Simple test loop
    let mut buf = vec![0u8; 1024];
    loop {
        // Process PTY output
        term.process_pty_output()?;
        
        // Print grid (simple dump)
        let cells = term.get_grid_content();
        // Just print first row for demo
        let first_row: String = cells
            .iter()
            .filter(|c| c.row == 0)
            .map(|c| c.c)
            .collect();
        
        if !first_row.trim().is_empty() {
            println!("Row 0: {}", first_row);
        }
        
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
